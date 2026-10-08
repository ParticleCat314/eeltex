//! Where the caret goes on the page, and what a click means.

use crate::attrs::{Node, NodeId};
use crate::edit::Editor;
use crate::edit::cursor::{self, Cursor, Step};
use crate::edit::ops::Selection;
use crate::edit::tree::{EditId, Item, Slot};
use crate::scene::{Rule, Scene};

/// Where to draw the caret, in scene space: pixels, y down, origin on the baseline at the left edge.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Caret {
    pub x: f32,
    /// The top edge.
    pub y: f32,
    pub height: f32,
    pub thickness: f32,
}

impl Caret {
    #[must_use]
    pub fn into_rule(self) -> Rule {
        Rule::new(self.x, self.y, self.thickness, self.height)
    }
}

/// The value paired with `id` in a list sorted by id.
fn look_up<T: Copy>(pairs: &[(EditId, T)], id: EditId) -> Option<T> {
    let at = pairs.binary_search_by_key(&id, |(id, _)| *id).ok()?;
    pairs.get(at).map(|(_, value)| *value)
}

/// Every node in a scene, sorted by the edit id its span carries.
fn node_index(scene: &Scene) -> Vec<(EditId, NodeId)> {
    let mut out: Vec<_> = scene
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.span.is_known())
        .filter_map(|(index, node)| {
            Some((EditId(node.span.start), NodeId(u32::try_from(index).ok()?)))
        })
        .collect();
    out.sort_by_key(|(id, _)| *id);
    out
}

fn find_slot(slot: &Slot, id: EditId) -> Option<&Slot> {
    if slot.id == id {
        return Some(slot);
    }
    slot.items.iter().find_map(|item| {
        item.slots_in_order()
            .into_iter()
            .find_map(|name| find_slot(item.slot(name)?, id))
    })
}

/// A slot (`child: None`) or item `child` of the slot at `path`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Anchor {
    id: EditId,
    path: Vec<Step>,
    child: Option<usize>,
}

/// Every id in the tree with where it is, sorted for binary search.
fn anchors(root: &Slot) -> Vec<Anchor> {
    fn walk(slot: &Slot, path: &[Step], out: &mut Vec<Anchor>) {
        out.push(Anchor {
            id: slot.id,
            path: path.to_vec(),
            child: None,
        });
        for (index, item) in slot.items.iter().enumerate() {
            out.push(Anchor {
                id: item.id,
                path: path.to_vec(),
                child: Some(index),
            });
            for name in item.slots_in_order() {
                let Some(inner) = item.slot(name) else {
                    continue;
                };
                let mut deeper = path.to_vec();
                deeper.push(Step::new(index, name));
                walk(inner, &deeper, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &[], &mut out);
    out.sort_by_key(|a| a.id);
    out
}

fn find_anchor(table: &[Anchor], id: EditId) -> Option<&Anchor> {
    let at = table.binary_search_by_key(&id, |a| a.id).ok()?;
    table.get(at)
}

impl Editor {
    /// The node an id was laid out as; `None` under [`SpanMode::Bytes`](super::SpanMode).
    #[must_use]
    pub fn node_of(&self, scene: &Scene, id: EditId) -> Option<NodeId> {
        if !id.is_some() || !self.ids_in_spans() {
            return None;
        }
        scene.node_at_offset(id.0)
    }

    /// The id a node came from; `None` under [`SpanMode::Bytes`](super::SpanMode).
    #[must_use]
    pub fn id_at(&self, scene: &Scene, node: NodeId) -> Option<EditId> {
        if !self.ids_in_spans() {
            return None;
        }
        let span = scene.node(node)?.span;
        span.is_known().then_some(EditId(span.start))
    }

    /// Whether a node is the box drawn where a slot is empty.
    #[must_use]
    pub fn is_placeholder(&self, scene: &Scene, node: NodeId) -> bool {
        let Some(id) = self.id_at(scene, node) else {
            return false;
        };
        find_slot(&self.root, id).is_some_and(Slot::is_empty)
    }

    /// Where to draw the caret; `None` unless the scene was laid out from this editor with id spans.
    #[must_use]
    pub fn caret(&self, scene: &Scene) -> Option<Caret> {
        if !self.ids_in_spans() {
            return None;
        }
        let index = node_index(scene);
        // Measured against the tree as drawn, which includes a `\name` being typed.
        let (root, cursor) = self.drawn();
        let slot = cursor::resolve(&root, &cursor.path)?;
        let slot_node = look_up(&index, slot.id)?;
        let x = caret_x(scene, &index, slot, cursor.index, slot_node)?;

        let groups = baseline_groups(&root);
        let (y, height) = line_of(scene, &groups, slot.id, slot_node)?;
        Some(Caret {
            x,
            y,
            height,
            // From the formula's size, not the line's: the same width in a script as on the baseline.
            thickness: (scene.font_size * 0.04).max(1.0),
        })
    }

    /// The cursor a click at this point means.
    #[must_use]
    pub fn cursor_at(&self, scene: &Scene, x: f32, y: f32) -> Option<Cursor> {
        if !self.ids_in_spans() {
            return None;
        }
        let table = anchors(&self.root);
        let index = node_index(scene);
        let Some(hit) = scene.hit(x, y) else {
            // Nothing under the pointer: land at the nearest position in the outermost slot.
            return Some(Cursor::at(
                Vec::new(),
                nearest_gap(scene, &index, &self.root, x),
            ));
        };
        for node in scene.ancestors(hit) {
            let Some(anchor) = self
                .id_at(scene, node)
                .and_then(|id| find_anchor(&table, id))
            else {
                continue;
            };
            return Some(match anchor.child {
                // An item: before or after it, by which half was clicked.
                Some(child) => {
                    let after = scene.node(node).is_some_and(|n| x > n.x + n.width * 0.5);
                    Cursor::at(anchor.path.clone(), child + usize::from(after))
                }
                None => {
                    let slot = cursor::resolve(&self.root, &anchor.path)?;
                    Cursor::at(anchor.path.clone(), nearest_gap(scene, &index, slot, x))
                }
            });
        }
        None
    }

    /// Select the sub-expression a node stands for.
    pub fn select_node(&mut self, scene: &Scene, node: NodeId) -> bool {
        let Some(id) = self.id_at(scene, node) else {
            return false;
        };
        let table = anchors(&self.root);
        let Some(anchor) = find_anchor(&table, id) else {
            return false;
        };
        let (start, end) = match anchor.child {
            Some(child) => (child, child + 1),
            None => (
                0,
                cursor::resolve(&self.root, &anchor.path).map_or(0, Slot::len),
            ),
        };
        self.anchor = Some(Cursor::at(anchor.path.clone(), start));
        self.cursor = Cursor::at(anchor.path.clone(), end);
        end > start
    }
}

/// Every slot's id paired with the slot whose baseline it sits on, sorted.
fn baseline_groups(root: &Slot) -> Vec<(EditId, EditId)> {
    fn walk(slot: &Slot, group: EditId, out: &mut Vec<(EditId, EditId)>) {
        out.push((slot.id, group));
        for item in &slot.items {
            for name in item.slots_in_order() {
                let Some(inner) = item.slot(name) else {
                    continue;
                };
                let inner_group = if name.breaks_baseline() {
                    inner.id
                } else {
                    group
                };
                walk(inner, inner_group, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root.id, &mut out);
    out.sort_unstable_by_key(|(id, _)| *id);
    out
}

/// The baseline and size of the glyphs sitting in a slot's *own* run.
/// A slot node inherits its parent's `Node::baseline`, so this reads its baseline group instead.
fn own_run(
    scene: &Scene,
    groups: &[(EditId, EditId)],
    slot: EditId,
    slot_node: NodeId,
) -> Option<(f32, f32)> {
    let want = look_up(groups, slot)?;
    let mut baseline = None;
    let mut size: f32 = 0.0;
    for glyph in scene.glyphs_of(slot_node) {
        if nearest_group(scene, groups, glyph.node()) != Some(want) {
            continue;
        }
        baseline.get_or_insert(glyph.y);
        size = size.max(glyph.size);
    }
    Some((baseline?, size))
}

/// The baseline group of the innermost slot at or above `node`.
fn nearest_group(scene: &Scene, groups: &[(EditId, EditId)], node: NodeId) -> Option<EditId> {
    scene.ancestors(node).find_map(|id| {
        let span = scene.node(id)?.span;
        span.is_known()
            .then(|| look_up(groups, EditId(span.start)))
            .flatten()
    })
}

/// Top edge and height of one line of text in a slot.
fn line_of(
    scene: &Scene,
    groups: &[(EditId, EditId)],
    slot: EditId,
    slot_node: NodeId,
) -> Option<(f32, f32)> {
    match own_run(scene, groups, slot, slot_node) {
        // Ascender to a little below the baseline, like a text cursor.
        Some((baseline, size)) => Some((baseline - size * 0.8, size * 1.05)),
        // Nothing sits on this slot's baseline: be as tall as its box.
        None => {
            let node = scene.node(slot_node)?;
            Some((node.y, node.height))
        }
    }
}

fn item_node<'a>(scene: &'a Scene, index: &[(EditId, NodeId)], item: &Item) -> Option<&'a Node> {
    scene.node(look_up(index, item.id)?)
}

fn caret_x(
    scene: &Scene,
    index: &[(EditId, NodeId)],
    slot: &Slot,
    at: usize,
    slot_node: NodeId,
) -> Option<f32> {
    if slot.items.is_empty() {
        // The placeholder's box is the slot's box.
        let node = scene.node(slot_node)?;
        return Some(node.x + node.width * 0.5);
    }
    // Item boxes use glyph advances, so the right edge behind is the pen position.
    if let Some(before) = at.checked_sub(1).and_then(|i| slot.items.get(i)) {
        if let Some(node) = item_node(scene, index, before) {
            return Some(node.x + node.width);
        }
    }
    if let Some(after) = slot.items.get(at) {
        if let Some(node) = item_node(scene, index, after) {
            return Some(node.x);
        }
    }
    scene.node(slot_node).map(|n| n.x)
}

/// Which gap between a slot's items a click at `x` is nearest.
fn nearest_gap(scene: &Scene, index: &[(EditId, NodeId)], slot: &Slot, x: f32) -> usize {
    let mut best = 0;
    let mut distance = f32::INFINITY;
    for at in 0..=slot.items.len() {
        let gap = match (
            at.checked_sub(1).and_then(|i| slot.items.get(i)),
            slot.items.get(at),
        ) {
            (Some(before), _) => item_node(scene, index, before).map(|n| n.x + n.width),
            (None, Some(after)) => item_node(scene, index, after).map(|n| n.x),
            (None, None) => None,
        };
        let Some(gap) = gap else { continue };
        let d = (gap - x).abs();
        if d < distance {
            distance = d;
            best = at;
        }
    }
    best
}

impl Selection {
    /// The nodes the selected items were laid out as.
    #[must_use]
    pub fn nodes(&self, editor: &Editor, scene: &Scene) -> Vec<NodeId> {
        let Some(slot) = cursor::resolve(editor.root(), &self.path) else {
            return Vec::new();
        };
        slot.items
            .get(self.start..self.end.min(slot.items.len()))
            .unwrap_or_default()
            .iter()
            .filter_map(|item| editor.node_of(scene, item.id))
            .collect()
    }

    /// One rectangle covering the whole range, as `[x, y, width, height]` in scene space.
    #[must_use]
    pub fn bounds(&self, editor: &Editor, scene: &Scene) -> Option<[f32; 4]> {
        if self.is_empty() || !editor.ids_in_spans() {
            return None;
        }
        let index = node_index(scene);
        let groups = baseline_groups(editor.root());
        let slot = cursor::resolve(editor.root(), &self.path)?;
        let slot_node = look_up(&index, slot.id)?;

        let left = caret_x(scene, &index, slot, self.start, slot_node)?;
        let right = caret_x(scene, &index, slot, self.end, slot_node)?;
        let (mut top, height) = line_of(scene, &groups, slot.id, slot_node)?;
        let mut bottom = top + height;
        for node in self.nodes(editor, scene) {
            let Some(node) = scene.node(node).filter(|n| n.has_ink()) else {
                continue;
            };
            top = top.min(node.y);
            bottom = bottom.max(node.y + node.height);
        }
        Some([
            left.min(right),
            top,
            (right - left).abs(),
            (bottom - top).max(0.0),
        ])
    }
}

#[cfg(feature = "embed-math-font")]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{Command, Motion, SlotName};
    use crate::{Fonts, Options};

    fn laid_out(editor: &Editor) -> Scene {
        let fonts = Fonts::embedded().expect("the embedded font parses");
        editor
            .layout(&fonts, &Options::default().with_font_size(32.0))
            .expect("lays out")
    }

    #[test]
    fn every_id_in_the_tree_reaches_a_node() {
        for source in [
            r"\frac{a}{b}",
            r"x^2_i",
            r"\sqrt[3]{x+1}",
            r"\frac{}{}",
            r"\begin{pmatrix} 1 & 2 \\ 3 & 4 \end{pmatrix}",
        ] {
            let editor = Editor::from_tex(source).expect("parses");
            let scene = laid_out(&editor);
            for anchor in anchors(editor.root()) {
                assert!(
                    editor.node_of(&scene, anchor.id).is_some(),
                    "{source}: {anchor:?} has no node"
                );
            }
        }
    }

    /// A space has no ink, so only its spacer says where it ends.
    #[test]
    fn the_caret_moves_past_a_space() {
        for spaces in [" ", "  "] {
            let mut editor = Editor::new();
            editor.insert_str("ab");
            let before = editor.caret(&laid_out(&editor)).expect("a caret").x;
            editor.insert_str(spaces);
            let scene = laid_out(&editor);
            let after = editor.caret(&scene).expect("a caret").x;
            assert!(after > before + 1.0, "{spaces:?}: {before} -> {after}");
            assert!(
                (after - scene.width).abs() < 0.01,
                "{spaces:?}: not at the end"
            );
        }
        let mut editor = Editor::from_tex(r"a\quad b").expect("parses");
        editor.set_cursor(cursor::Cursor::at(Vec::new(), 2));
        let scene = laid_out(&editor);
        let caret = editor.caret(&scene).expect("a caret").x;
        let b = scene.nodes.last().expect("b's node").x;
        assert!(
            (caret - b).abs() < 0.01,
            "{caret} is not where b starts ({b})"
        );
    }

    #[test]
    fn the_caret_is_as_thick_in_a_script_as_on_the_baseline() {
        let editor = Editor::from_tex(r"x^{2} + \frac{\sum_{i} a}{b}").expect("parses");
        let scene = laid_out(&editor);
        let mut widths = Vec::new();
        for cursor in cursor::positions(editor.root()) {
            let mut at = editor.clone();
            at.set_cursor(cursor);
            if let Some(caret) = at.caret(&scene) {
                widths.push(caret.thickness);
            }
        }
        assert!(widths.len() > 4);
        assert!(
            widths.iter().all(|w| (w - widths[0]).abs() < 1e-6),
            "{widths:?}"
        );
    }

    #[test]
    fn the_caret_is_drawable_at_every_position() {
        for source in [r"\frac{a}{b}", "x^2", r"\sqrt{x}", r"\frac{}{}", "abc"] {
            let editor = Editor::from_tex(source).expect("parses");
            let scene = laid_out(&editor);
            for cursor in cursor::positions(editor.root()) {
                let mut at = editor.clone();
                at.set_cursor(cursor.clone());
                let caret = at.caret(&scene).unwrap_or_else(|| {
                    panic!("{source}: no caret at {cursor:?}");
                });
                assert!(caret.height > 0.0, "{source}: flat caret at {cursor:?}");
                assert!(caret.thickness > 0.0);
                assert!(caret.x.is_finite() && caret.y.is_finite());
                assert!(
                    caret.x >= -1.0 && caret.x <= scene.width + 1.0,
                    "{source}: caret at {} is outside 0..{}",
                    caret.x,
                    scene.width
                );
            }
        }
    }

    #[test]
    fn the_caret_advances_across_a_run() {
        let editor = Editor::from_tex("abcd").expect("parses");
        let scene = laid_out(&editor);
        let mut last = f32::NEG_INFINITY;
        for index in 0..=4 {
            let mut at = editor.clone();
            at.set_cursor(Cursor::at(Vec::new(), index));
            let caret = at.caret(&scene).expect("a caret");
            assert!(caret.x >= last, "went backwards at {index}");
            last = caret.x;
        }
    }

    #[test]
    fn a_caret_stands_on_the_baseline_of_its_own_slot() {
        let editor = Editor::from_tex("x^2").expect("parses");
        let scene = laid_out(&editor);
        let base = scene.glyphs.first().expect("the x");
        let script = scene.glyphs.get(1).expect("the 2");
        assert!(script.y < base.y, "the script is not raised");

        let caret_in = |path: Vec<Step>| {
            let mut at = editor.clone();
            at.set_cursor(Cursor::at(path, 0));
            at.caret(&scene).expect("a caret")
        };
        for (caret, glyph) in [
            (caret_in(vec![Step::new(0, SlotName::Base)]), base),
            (caret_in(vec![Step::new(0, SlotName::Sup)]), script),
        ] {
            assert!(
                caret.y < glyph.y && caret.y + caret.height > glyph.y,
                "a caret at {}..{} does not cross the baseline at {}",
                caret.y,
                caret.y + caret.height,
                glyph.y
            );
        }
    }

    #[test]
    fn a_numerator_and_a_denominator_get_separate_carets() {
        let editor = Editor::from_tex(r"\frac{a}{b}").expect("parses");
        let scene = laid_out(&editor);
        let caret_in = |name| {
            let mut at = editor.clone();
            at.set_cursor(Cursor::at(vec![Step::new(0, name)], 0));
            at.caret(&scene).expect("a caret")
        };
        let num = caret_in(SlotName::Num);
        let den = caret_in(SlotName::Den);
        assert!(
            num.y + num.height <= den.y,
            "the numerator's caret ({}..{}) reaches into the denominator's ({}..)",
            num.y,
            num.y + num.height,
            den.y
        );
    }

    #[test]
    fn a_caret_beside_a_construct_is_sized_to_it() {
        let fraction = Editor::from_tex(r"\frac{a}{b}").expect("parses");
        let scene = laid_out(&fraction);
        let beside = fraction.caret(&scene).expect("a caret");
        assert!(
            beside.height >= scene.ascent + scene.descent - 1.0,
            "a caret beside a fraction is only {} tall",
            beside.height
        );

        // A script does not break the line its base sits on.
        let plain = Editor::from_tex("x").expect("parses");
        let scripted = Editor::from_tex("x^2").expect("parses");
        let plain = plain.caret(&laid_out(&plain)).expect("a caret");
        let scripted = scripted.caret(&laid_out(&scripted)).expect("a caret");
        assert_eq!(
            (plain.y, plain.height),
            (scripted.y, scripted.height),
            "a superscript changed the caret beside its base"
        );
    }

    #[test]
    fn a_caret_in_a_script_is_script_sized() {
        let editor = Editor::from_tex("x^2").expect("parses");
        let scene = laid_out(&editor);

        let mut outer = editor.clone();
        outer.set_cursor(Cursor::at(Vec::new(), 0));
        let mut inner = editor.clone();
        inner.set_cursor(Cursor::at(vec![Step::new(0, SlotName::Sup)], 0));

        let outer = outer.caret(&scene).expect("a caret");
        let inner = inner.caret(&scene).expect("a caret");
        assert!(
            inner.height < outer.height,
            "{} is not smaller than {}",
            inner.height,
            outer.height
        );
    }

    #[test]
    fn an_empty_slot_can_be_clicked_into() {
        let editor = Editor::from_tex(r"\frac{a}{}").expect("parses");
        let scene = laid_out(&editor);
        let den = editor
            .root()
            .items
            .first()
            .and_then(|i| i.slot(SlotName::Den))
            .expect("a denominator");
        let node = editor.node_of(&scene, den.id).expect("a node");
        let boxed = scene.node(node).expect("a box");
        assert!(boxed.has_ink(), "the empty denominator has no box");
        assert!(editor.is_placeholder(&scene, node));

        let cursor = editor
            .cursor_at(
                &scene,
                boxed.x + boxed.width * 0.5,
                boxed.y + boxed.height * 0.5,
            )
            .expect("a cursor");
        assert_eq!(
            cursor.path.last().map(|s| s.slot),
            Some(SlotName::Den),
            "clicking the empty denominator landed at {cursor:?}"
        );
    }

    #[test]
    fn a_click_finds_the_position_it_was_drawn_at() {
        let editor = Editor::from_tex(r"\frac{ab}{c}+x").expect("parses");
        let scene = laid_out(&editor);
        for cursor in cursor::positions(editor.root()) {
            let mut at = editor.clone();
            at.set_cursor(cursor.clone());
            let caret = at.caret(&scene).expect("a caret");
            // A hair inside the caret, so the hit lands on ink.
            let found = editor.cursor_at(&scene, caret.x - 0.5, caret.y + caret.height * 0.5);
            let Some(found) = found else { continue };
            assert!(
                cursor::resolve(editor.root(), &found.path).is_some(),
                "clicking at {cursor:?} gave {found:?}, which does not resolve"
            );
        }
    }

    #[test]
    fn a_selection_resolves_to_the_nodes_an_effect_wants() {
        let mut editor = Editor::from_tex("abc").expect("parses");
        let scene = laid_out(&editor);
        editor.apply(Command::Move(Motion::Home));
        editor.apply(Command::Extend(Motion::Right));
        editor.apply(Command::Extend(Motion::Right));
        let range = editor.selection().expect("a selection");
        let nodes = range.nodes(&editor, &scene);
        assert_eq!(nodes.len(), 2);
        assert!(range.bounds(&editor, &scene).is_some());
        for node in nodes {
            assert!(scene.node(node).is_some_and(Node::has_ink));
        }
    }

    #[test]
    fn a_selection_is_one_rectangle_level_with_the_caret() {
        let mut editor = Editor::from_tex("a+b").expect("parses");
        let scene = laid_out(&editor);
        let line = editor.caret(&scene).expect("a caret");

        editor.apply(Command::Move(Motion::Home));
        for _ in 0..3 {
            editor.apply(Command::Extend(Motion::Right));
        }
        let range = editor.selection().expect("a selection");
        assert_eq!(range.len(), 3);
        let [x, y, width, height] = range.bounds(&editor, &scene).expect("a box");

        assert!((y - line.y).abs() < 0.01, "the highlight is off the line");
        assert!((height - line.height).abs() < 0.01, "the wrong height");
        assert!(x <= 0.01, "starts at {x}, not at the left edge");
        assert!(
            (x + width - scene.width).abs() < 0.01,
            "{width} wide, but the formula is {}",
            scene.width
        );
    }

    #[test]
    fn a_selection_over_a_kern_is_not_dragged_to_the_origin() {
        let mut editor = Editor::from_tex(r"ab \, cd").expect("parses");
        let scene = laid_out(&editor);
        editor.apply(Command::Move(Motion::End));
        for _ in 0..3 {
            editor.apply(Command::Extend(Motion::Left));
        }
        let range = editor.selection().expect("a selection");
        let [x, _, width, _] = range.bounds(&editor, &scene).expect("a box");
        assert!(x > 0.01, "the highlight was dragged back to the origin");
        assert!(width > 0.0 && x + width <= scene.width + 0.01);
    }

    #[test]
    fn a_selection_over_a_fraction_covers_it() {
        let mut editor = Editor::from_tex(r"x\frac{a}{b}").expect("parses");
        let scene = laid_out(&editor);
        editor.apply(Command::SelectAll);
        let range = editor.selection().expect("a selection");
        let [_, y, _, height] = range.bounds(&editor, &scene).expect("a box");
        assert!(
            y <= -scene.ascent + 0.01 && y + height >= scene.descent - 0.01,
            "{y}..{} does not cover {}..{}",
            y + height,
            -scene.ascent,
            scene.descent
        );
    }

    #[test]
    fn nothing_answers_from_a_byte_offset_read_as_an_id() {
        use crate::edit::{EditOptions, SpanMode};
        let mut editor = Editor::with_options(EditOptions::default().with_spans(SpanMode::Bytes));
        editor.set_tex("abcdefgh").expect("parses");
        let scene = laid_out(&editor);
        assert!(!editor.ids_in_spans());
        assert!(scene.nodes.len() > 4, "the scene has no nodes to confuse");

        assert!(editor.caret(&scene).is_none());
        assert!(editor.cursor_at(&scene, 5.0, 0.0).is_none());
        assert!(editor.node_of(&scene, EditId(1)).is_none());
        for index in 0..scene.nodes.len() {
            let Ok(index) = u32::try_from(index) else {
                continue;
            };
            let node = NodeId(index);
            assert!(
                editor.id_at(&scene, node).is_none(),
                "node {index} answered"
            );
            assert!(!editor.is_placeholder(&scene, node));
            assert!(!editor.clone().select_node(&scene, node));
        }
    }

    #[test]
    fn a_half_typed_command_is_drawn_where_it_is_being_typed() {
        let mut editor = Editor::from_tex("x+").expect("parses");
        let plain = laid_out(&editor);
        let settled = editor.caret(&plain).expect("a caret").x;

        let mut wider = settled;
        for ch in ['\\', 'i', 'n', 't'] {
            editor.apply(Command::Insert(ch));
            let scene = laid_out(&editor);
            assert!(
                scene.width > plain.width,
                "the half-typed command is not being drawn"
            );
            let caret = editor.caret(&scene).expect("a caret").x;
            assert!(
                caret > wider - 0.01,
                "the caret went backwards over the command being typed"
            );
            wider = caret;
            assert_eq!(editor.tex(), "x+");
        }

        editor.apply(Command::Submit);
        assert_eq!(editor.tex(), r"x+\int");
        let scene = laid_out(&editor);
        assert!(editor.pending().is_none());
        let caret = editor.caret(&scene).expect("a caret").x;
        assert!(caret < wider && caret > settled);
    }

    #[test]
    fn a_click_past_the_formula_lands_at_the_nearest_end() {
        let editor = Editor::from_tex("abc").expect("parses");
        let scene = laid_out(&editor);
        let far = editor
            .cursor_at(&scene, scene.width + 500.0, 0.0)
            .expect("a cursor");
        assert_eq!(far, Cursor::at(Vec::new(), 3));
        let before = editor.cursor_at(&scene, -500.0, 0.0).expect("a cursor");
        assert_eq!(before, Cursor::at(Vec::new(), 0));
    }

    #[test]
    fn the_caret_becomes_a_rule_any_backend_can_draw() {
        let editor = Editor::from_tex("x").expect("parses");
        let mut scene = laid_out(&editor);
        let before = scene.rules.len();
        let caret = editor.caret(&scene).expect("a caret");
        scene.rules.push(caret.into_rule());
        assert_eq!(scene.rules.len(), before + 1);
        let rule = scene.rules.last().expect("the caret");
        assert!(rule.width > 0.0 && rule.height > 0.0);
    }
}
