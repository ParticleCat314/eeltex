//! Where the caret is, and how it moves.

use crate::edit::tree::{Item, Slot, SlotName};

/// One descent: into item `child` of the current slot, then into its `slot`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub child: usize,
    pub slot: SlotName,
}

impl Step {
    #[must_use]
    pub fn new(child: usize, slot: SlotName) -> Step {
        Step { child, slot }
    }
}

/// A position between items in some slot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cursor {
    /// From the root slot down. Empty means the cursor is in the root.
    pub path: Vec<Step>,
    /// In `0..=slot.len()`; `0` is before the first item.
    pub index: usize,
}

impl Cursor {
    #[must_use]
    pub fn start() -> Cursor {
        Cursor::default()
    }

    #[must_use]
    pub fn at(path: Vec<Step>, index: usize) -> Cursor {
        Cursor { path, index }
    }

    /// The path of the slot containing this one, and the step out of it.
    #[must_use]
    pub fn parent(&self) -> Option<(&[Step], Step)> {
        let (last, rest) = self.path.split_last()?;
        Some((rest, *last))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    /// Within the current slot.
    Home,
    End,
    /// Left past a run of symbols, stopping at the first structural boundary.
    WordLeft,
    WordRight,
}

/// The slot at `path`, or `None` if the path no longer resolves.
#[must_use]
pub fn resolve<'a>(root: &'a Slot, path: &[Step]) -> Option<&'a Slot> {
    let mut slot = root;
    for step in path {
        slot = slot.items.get(step.child)?.slot(step.slot)?;
    }
    Some(slot)
}

pub fn resolve_mut<'a>(root: &'a mut Slot, path: &[Step]) -> Option<&'a mut Slot> {
    let mut slot = root;
    for step in path {
        slot = slot.items.get_mut(step.child)?.slot_mut(step.slot)?;
    }
    Some(slot)
}

#[must_use]
pub fn item_at<'a>(root: &'a Slot, path: &[Step], child: usize) -> Option<&'a Item> {
    resolve(root, path)?.items.get(child)
}

/// Trim a cursor until it names a position that exists.
#[must_use]
pub fn clamp(root: &Slot, cursor: &Cursor) -> Cursor {
    let mut slot = root;
    let mut path = Vec::new();
    for step in &cursor.path {
        let Some(next) = slot.items.get(step.child).and_then(|i| i.slot(step.slot)) else {
            break;
        };
        slot = next;
        path.push(*step);
    }
    let index = if path.len() == cursor.path.len() {
        cursor.index.min(slot.len())
    } else {
        // The cursor was inside something now gone: land at the end of what remains.
        slot.len()
    };
    Cursor::at(path, index)
}

/// The cursor one motion away, or `None` if it cannot move that way.
#[must_use]
pub fn moved(root: &Slot, cursor: &Cursor, motion: Motion) -> Option<Cursor> {
    let next = match motion {
        Motion::Left => left(root, cursor),
        Motion::Right => right(root, cursor),
        Motion::Up => vertical(root, cursor, true),
        Motion::Down => vertical(root, cursor, false),
        Motion::Home => Some(Cursor::at(cursor.path.clone(), 0)),
        Motion::End => {
            let len = resolve(root, &cursor.path)?.len();
            Some(Cursor::at(cursor.path.clone(), len))
        }
        Motion::WordLeft => word(root, cursor, false),
        Motion::WordRight => word(root, cursor, true),
    }?;
    (next != *cursor).then_some(next)
}

fn right(root: &Slot, cursor: &Cursor) -> Option<Cursor> {
    let slot = resolve(root, &cursor.path)?;
    match slot.items.get(cursor.index) {
        Some(item) => match item.slots_in_order().first() {
            Some(name) => Some(enter(&cursor.path, cursor.index, *name, 0)),
            None => Some(Cursor::at(cursor.path.clone(), cursor.index + 1)),
        },
        None => ascend(root, cursor, true),
    }
}

fn left(root: &Slot, cursor: &Cursor) -> Option<Cursor> {
    let slot = resolve(root, &cursor.path)?;
    let Some(before) = cursor.index.checked_sub(1) else {
        return ascend(root, cursor, false);
    };
    let item = slot.items.get(before)?;
    match item.slots_in_order().last() {
        Some(name) => {
            let len = item.slot(*name)?.len();
            Some(enter(&cursor.path, before, *name, len))
        }
        None => Some(Cursor::at(cursor.path.clone(), before)),
    }
}

/// At `index` in the `name` slot of item `child` of the slot at `path`.
fn enter(path: &[Step], child: usize, name: SlotName, index: usize) -> Cursor {
    let mut path = path.to_vec();
    path.push(Step::new(child, name));
    Cursor::at(path, index)
}

/// To the item's next (or previous) slot if it has one, otherwise past the item itself.
fn ascend(root: &Slot, cursor: &Cursor, forwards: bool) -> Option<Cursor> {
    let (parent, step) = cursor.parent()?;
    let item = item_at(root, parent, step.child)?;
    let names = item.slots_in_order();
    let here = names.iter().position(|n| *n == step.slot)?;

    let sibling = if forwards {
        names.get(here + 1)
    } else {
        here.checked_sub(1).and_then(|i| names.get(i))
    };
    if let Some(name) = sibling {
        let index = if forwards { 0 } else { item.slot(*name)?.len() };
        return Some(enter(parent, step.child, *name, index));
    }

    let index = if forwards { step.child + 1 } else { step.child };
    Some(Cursor::at(parent.to_vec(), index))
}

/// To the slot of the enclosing item that sits above or below this one.
fn vertical(root: &Slot, cursor: &Cursor, up: bool) -> Option<Cursor> {
    let (parent, step) = cursor.parent()?;
    let item = item_at(root, parent, step.child)?;

    let target = match (step.slot, up) {
        (SlotName::Den, true) => Some(SlotName::Num),
        (SlotName::Num, false) => Some(SlotName::Den),
        (SlotName::Sub, true) => Some(SlotName::Sup),
        (SlotName::Sup, false) => Some(SlotName::Sub),
        (SlotName::Lower, true) => Some(SlotName::Upper),
        (SlotName::Upper, false) => Some(SlotName::Lower),
        (SlotName::Base, true) => Some(SlotName::Over),
        (SlotName::Base, false) => Some(SlotName::Under),
        (SlotName::Over, false) => Some(SlotName::Base),
        (SlotName::Under, true) => Some(SlotName::Base),
        (SlotName::Cell { row, col }, up) => {
            let row = if up {
                row.checked_sub(1)
            } else {
                row.checked_add(1)
            };
            row.map(|row| SlotName::Cell { row, col })
        }
        _ => None,
    };

    if let Some(name) = target {
        if let Some(slot) = item.slot(name) {
            // Keep the offset within the slot, so moving down a matrix column stays in place.
            let index = cursor.index.min(slot.len());
            return Some(enter(parent, step.child, name, index));
        }
    }

    // Nothing above or below inside this item: leave it, on the side it was nearest.
    let index = if cursor.index == 0 {
        step.child
    } else {
        step.child + 1
    };
    Some(Cursor::at(parent.to_vec(), index))
}

/// Past a run of leaf items in one go, stopping at a composite.
fn word(root: &Slot, cursor: &Cursor, forwards: bool) -> Option<Cursor> {
    let slot = resolve(root, &cursor.path)?;
    let mut index = cursor.index;
    loop {
        let next = if forwards {
            slot.items.get(index)
        } else {
            index.checked_sub(1).and_then(|i| slot.items.get(i))
        };
        let Some(item) = next else { break };
        if item.is_composite() {
            break;
        }
        index = if forwards { index + 1 } else { index - 1 };
    }
    if index != cursor.index {
        return Some(Cursor::at(cursor.path.clone(), index));
    }
    // Already against a composite: fall back to a single step.
    if forwards {
        right(root, cursor)
    } else {
        left(root, cursor)
    }
}

/// Every position in the tree, in the order [`Motion::Right`] visits them.
#[must_use]
pub fn positions(root: &Slot) -> Vec<Cursor> {
    std::iter::successors(Some(Cursor::start()), |c| moved(root, c, Motion::Right))
        // A backstop against a cycle; the parser's depth limit bounds real formulas.
        .take(100_001)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::lower::raise;
    use crate::edit::tree::IdGen;

    fn tree(src: &str) -> Slot {
        let mut ids = IdGen::default();
        raise(&crate::parse(src).expect("parses"), &mut ids)
    }

    const FORMULAE: &[&str] = &[
        "x",
        "",
        "abc",
        r"\frac{a}{b}",
        r"\frac{}{}",
        r"x^2_i",
        r"\sqrt[3]{x+1}",
        r"\sum_{n=1}^{\infty} \frac{1}{n^2}",
        r"\begin{pmatrix} 1 & 2 \\ 3 & 4 \end{pmatrix}",
        r"\left( \frac{a}{b} \right)",
        r"\underbrace{x+y}_{n}",
        r"\overset{a}{=}",
        r"\xrightarrow[f]{g}",
        r"\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}",
    ];

    #[test]
    fn a_walk_right_visits_every_position_exactly_once() {
        for src in FORMULAE {
            let root = tree(src);
            let seen = positions(&root);
            let mut sorted: Vec<_> = seen.iter().map(|c| (c.path.clone(), c.index)).collect();
            let before = sorted.len();
            sorted.sort_by(|a, b| {
                a.1.cmp(&b.1).then_with(|| {
                    a.0.iter()
                        .map(|s| s.child)
                        .collect::<Vec<_>>()
                        .cmp(&b.0.iter().map(|s| s.child).collect::<Vec<_>>())
                        .then_with(|| format!("{:?}", a.0).cmp(&format!("{:?}", b.0)))
                })
            });
            sorted.dedup();
            assert_eq!(sorted.len(), before, "{src}: a position was visited twice");
            for cursor in &seen {
                assert!(
                    resolve(&root, &cursor.path).is_some(),
                    "{src}: {cursor:?} does not resolve"
                );
            }
        }
    }

    #[test]
    fn a_walk_left_retraces_the_walk_right() {
        for src in FORMULAE {
            let root = tree(src);
            let forwards = positions(&root);
            let mut backwards = vec![forwards.last().cloned().unwrap_or_default()];
            let mut cursor = backwards.first().cloned().unwrap_or_default();
            while let Some(next) = moved(&root, &cursor, Motion::Left) {
                backwards.push(next.clone());
                cursor = next;
            }
            backwards.reverse();
            assert_eq!(backwards, forwards, "{src}");
        }
    }

    #[test]
    fn even_an_empty_formula_has_somewhere_to_be() {
        assert_eq!(positions(&tree("")), vec![Cursor::start()]);
        assert_eq!(positions(&tree("x")).len(), 2);
    }

    #[test]
    fn up_and_down_pair_the_slots_a_reader_means() {
        let root = tree(r"\frac{a}{b}");
        let num = Cursor::at(vec![Step::new(0, SlotName::Num)], 1);
        let down = moved(&root, &num, Motion::Down).expect("moves");
        assert_eq!(down.path.last().map(|s| s.slot), Some(SlotName::Den));
        let up = moved(&root, &down, Motion::Up).expect("moves");
        assert_eq!(up, num);
    }

    #[test]
    fn a_vertical_move_never_creates_a_slot() {
        use crate::edit::lower::{Lowering, lower};
        let root = tree("x_i");
        let sub = Cursor::at(vec![Step::new(0, SlotName::Sub)], 1);
        let up = moved(&root, &sub, Motion::Up).expect("moves");
        assert!(up.path.is_empty(), "did not leave the scripts: {up:?}");
        assert_eq!(crate::to_tex(&lower(&root, Lowering::TEX)), "x_{i}");
    }

    #[test]
    fn up_from_the_top_row_leaves_the_matrix_as_down_from_the_bottom_does() {
        let root = tree(r"\begin{matrix}a&b\\c&d\end{matrix}");
        let cell = |row| Cursor::at(vec![Step::new(0, SlotName::Cell { row, col: 0 })], 1);
        let up = moved(&root, &cell(0), Motion::Up).expect("moves");
        let down = moved(&root, &cell(1), Motion::Down).expect("moves");
        assert_eq!(up, Cursor::at(Vec::new(), 1));
        assert_eq!(down, up);
    }

    #[test]
    fn a_stale_path_clamps_to_what_is_left() {
        let root = tree("x");
        let stale = Cursor::at(
            vec![Step::new(4, SlotName::Num), Step::new(0, SlotName::Den)],
            9,
        );
        let clamped = clamp(&root, &stale);
        assert!(clamped.path.is_empty());
        assert_eq!(clamped.index, 1);
        assert!(resolve(&root, &clamped.path).is_some());
    }

    #[test]
    fn home_and_end_do_not_leave_the_slot() {
        let root = tree(r"\frac{abc}{d}");
        let inside = Cursor::at(vec![Step::new(0, SlotName::Num)], 1);
        let home = moved(&root, &inside, Motion::Home).expect("moves");
        assert_eq!(home, Cursor::at(vec![Step::new(0, SlotName::Num)], 0));
        let end = moved(&root, &inside, Motion::End).expect("moves");
        assert_eq!(end, Cursor::at(vec![Step::new(0, SlotName::Num)], 3));
        assert!(moved(&root, &home, Motion::Home).is_none());
    }
}
