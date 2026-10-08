//! The node arena, hit-testing, source spans and colour.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(all(feature = "attrs", feature = "embed-math-font"))]

use eeltex::{Fonts, NodeId, NodeKind, Options, Paint, ParseOptions, Scene};

fn fonts() -> Fonts<'static> {
    Fonts::embedded().expect("the embedded font parses")
}

fn scene(src: &str) -> Scene {
    eeltex::render(src, &fonts(), &Options::default().with_font_size(40.0))
        .unwrap_or_else(|e| panic!("{src:?}: {e}"))
}

fn spanned(src: &str) -> Scene {
    let expr = eeltex::parse_with(src, &ParseOptions::default().with_spans(true))
        .unwrap_or_else(|e| panic!("{src:?}: {e}"));
    eeltex::layout(&expr, &fonts(), &Options::default().with_font_size(40.0))
        .unwrap_or_else(|e| panic!("{src:?}: {e}"))
}

fn first_of(scene: &Scene, kind: NodeKind) -> NodeId {
    scene
        .nodes
        .iter()
        .position(|n| n.kind == kind)
        .map(|i| NodeId(i as u32))
        .unwrap_or_else(|| panic!("no {kind:?} node in {:?}", kinds(scene)))
}

fn kinds(scene: &Scene) -> Vec<NodeKind> {
    scene.nodes.iter().map(|n| n.kind).collect()
}

#[test]
fn every_item_belongs_to_a_node_that_exists() {
    for source in [
        "x",
        "\\frac{a}{b}",
        "\\sqrt[3]{x + 1}",
        "\\left( \\sum_{n=1}^{k} \\frac{1}{n} \\right)",
        "\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}",
        "\\hat{x} \\text{ and } \\mathbb{R}",
    ] {
        let s = scene(source);
        assert!(!s.nodes.is_empty(), "{source}: no nodes at all");
        for glyph in &s.glyphs {
            assert!(
                s.node(glyph.node()).is_some(),
                "{source}: glyph {:?} points at a node that is not there",
                glyph.glyph
            );
        }
        for rule in &s.rules {
            assert!(
                s.node(rule.node()).is_some(),
                "{source}: a rule points at a node that is not there"
            );
        }
        // Parents come before children, and only the root has none.
        for (index, node) in s.nodes.iter().enumerate() {
            match node.parent {
                None => assert_eq!(index, 0, "{source}: node {index} is a second root"),
                Some(parent) => assert!(
                    parent.index() < index,
                    "{source}: node {index}'s parent {parent:?} comes after it"
                ),
            }
        }
    }
}

#[test]
fn a_node_covers_everything_beneath_it() {
    let s = scene("\\frac{a + b}{c} = \\sqrt{d}");
    for (index, node) in s.nodes.iter().enumerate() {
        let id = NodeId(index as u32);
        for glyph in s.glyphs_of(id) {
            assert!(
                glyph.x >= node.x - 0.01 && glyph.x <= node.x + node.width + 0.01,
                "node {index} ({:?}) box {:?} does not hold a glyph at x {}",
                node.kind,
                (node.x, node.width),
                glyph.x
            );
        }
    }

    let root = s.node(NodeId::ROOT).expect("a root");
    assert!(root.parent.is_none());
    assert!(
        root.width >= s.width - 0.5 && root.height >= s.height() - 0.5,
        "the root box {:?} is smaller than the scene {:?}",
        (root.width, root.height),
        (s.width, s.height())
    );
    assert_eq!(
        s.glyphs_of(NodeId::ROOT).count(),
        s.glyphs.len(),
        "the root does not own every glyph"
    );
}

#[test]
fn a_point_in_the_numerator_hits_the_numerator() {
    let s = scene("\\frac{abc}{xyz}");
    let frac = first_of(&s, NodeKind::Frac);
    let bar = s.rules.first().copied().expect("the fraction bar");

    // A glyph of the numerator is anything above the bar.
    let numerator_glyph = s
        .glyphs
        .iter()
        .find(|g| g.y < bar.y)
        .copied()
        .expect("a numerator glyph");
    let hit = s
        .hit(numerator_glyph.x + 1.0, numerator_glyph.y - 1.0)
        .expect("the pointer landed on nothing");

    assert!(
        s.is_within(hit, frac),
        "hit {hit:?} is not inside the fraction {frac:?}"
    );
    assert_ne!(hit, frac, "the hit stopped at the fraction itself");
    assert!(
        s.is_within(numerator_glyph.node(), hit) || s.is_within(hit, numerator_glyph.node()),
        "the hit and the glyph under the pointer are in different subtrees"
    );

    assert_eq!(s.hit(-100.0, 0.0), None);
    assert_eq!(s.hit(s.width + 100.0, 0.0), None);
}

#[test]
fn a_script_wins_over_the_expression_containing_it() {
    let s = scene("x^2");
    let sup = s
        .glyphs
        .iter()
        .find(|g| g.y < 0.0)
        .copied()
        .expect("the superscript");
    let hit = s.hit(sup.x + 1.0, sup.y - 1.0).expect("landed on nothing");
    let node = s.node(hit).expect("a real node");
    assert!(
        node.width < s.width,
        "the hit box {:?} is the whole formula {}",
        node.width,
        s.width
    );
    assert_eq!(hit, sup.node(), "the hit is not the superscript's own node");
}

#[test]
fn spans_are_off_unless_asked_for() {
    let plain = scene("x + y");
    assert!(
        plain.nodes.iter().all(|n| !n.span.is_known()),
        "spans appeared without being asked for"
    );
    assert_eq!(plain.node_at_offset(0), None);

    let marked = spanned("x + y");
    assert!(
        marked.nodes.iter().any(|n| n.span.is_known()),
        "no node carries a span"
    );
    // Same geometry either way: a span is metadata, not layout.
    assert!((marked.width - plain.width).abs() < 1e-4);
    assert_eq!(marked.glyphs.len(), plain.glyphs.len());
}

#[test]
fn an_offset_maps_to_the_glyphs_it_produced() {
    let source = "ab + \\frac{c}{d}";
    let s = spanned(source);

    let node = s.node_at_offset(1).expect("nothing covers the 'b'");
    let span = s.node(node).expect("a real node").span;
    assert_eq!(
        source.get(span.start as usize..span.end as usize),
        Some("b"),
        "the innermost node for offset 1 spans {span:?}"
    );

    // The `\frac` starts at offset 5 and runs to the end.
    let frac = s.node_at_offset(5).expect("nothing covers the \\frac");
    let span = s.node(frac).expect("a real node").span;
    assert_eq!(span.start, 5);
    assert_eq!(span.end as usize, source.len());
    assert!(
        s.glyphs_of(frac).count() == 2,
        "the \\frac owns {} glyphs, not two",
        s.glyphs_of(frac).count()
    );
    assert_eq!(s.rules_of(frac).count(), 1, "the bar is not the \\frac's");
}

#[test]
fn scaling_and_translating_carry_the_node_boxes() {
    let s = scene("\\frac{a}{b}");
    let big = s.scaled(3.0);
    assert_eq!(big.nodes.len(), s.nodes.len());
    for (before, after) in s.nodes.iter().zip(&big.nodes) {
        assert!((after.x - before.x * 3.0).abs() < 1e-3);
        assert!((after.width - before.width * 3.0).abs() < 1e-3);
        assert!((after.baseline - before.baseline * 3.0).abs() < 1e-3);
    }

    let moved = s.translated(10.0, 5.0);
    for (before, after) in s.nodes.iter().zip(&moved.nodes) {
        assert!((after.x - (before.x + 10.0)).abs() < 1e-3);
        assert!((after.y - (before.y + 5.0)).abs() < 1e-3);
        assert!((after.width - before.width).abs() < 1e-3);
    }
    let hit_here = s.hit(s.width / 2.0, 0.0);
    let hit_there = moved.hit(s.width / 2.0 + 10.0, 5.0);
    assert_eq!(hit_here, hit_there);
}

#[test]
fn the_tree_has_the_shape_of_the_source() {
    let s = scene("\\sqrt{\\frac{a}{b}}");
    let radical = first_of(&s, NodeKind::Radical);
    let frac = first_of(&s, NodeKind::Frac);
    assert!(
        s.is_within(frac, radical),
        "the fraction is not inside the radical: {:?}",
        kinds(&s)
    );
    assert!(!s.is_within(radical, frac));

    let s = scene("\\sum_{n=1}^{k} x");
    assert!(kinds(&s).contains(&NodeKind::BigOp));
    let s = scene("\\hat{x}");
    assert!(kinds(&s).contains(&NodeKind::Accent));
    let s = scene("\\text{hi}");
    assert!(kinds(&s).contains(&NodeKind::Text));
    let s = scene("\\left( x \\right)");
    assert!(kinds(&s).contains(&NodeKind::Delimited));
}

#[test]
fn a_delimiter_belongs_to_the_pair_and_not_to_the_body() {
    let s = scene("\\left( \\frac{a}{b} \\right)");
    let delimited = first_of(&s, NodeKind::Delimited);
    let frac = first_of(&s, NodeKind::Frac);

    let brackets: Vec<_> = s.glyphs.iter().filter(|g| g.node() == delimited).collect();
    assert!(
        brackets.len() >= 2,
        "the pair owns {} glyphs, not its two brackets",
        brackets.len()
    );
    assert!(
        brackets.iter().all(|g| !s.is_within(g.node(), frac)),
        "a bracket was claimed by the fraction"
    );
}

#[test]
fn a_colour_wrapper_paints_what_it_wraps_and_no_more() {
    let s = scene("a \\textcolor{red}{bc} d");
    let red = Paint::rgb(255, 0, 0);
    let painted: Vec<_> = s.glyphs.iter().map(|g| s.resolve(g.paint())).collect();
    assert_eq!(
        painted,
        vec![None, Some(red), Some(red), None],
        "the wrong glyphs are red"
    );
    assert!(
        s.paints.len() == 2,
        "the table holds {:?} rather than inherit plus red",
        s.paints
    );
}

#[test]
fn the_innermost_colour_wins() {
    let s = scene("\\textcolor{red}{a \\textcolor{blue}{b} c}");
    let seen: Vec<_> = s
        .glyphs
        .iter()
        .filter_map(|g| s.resolve(g.paint()))
        .collect();
    assert_eq!(
        seen,
        vec![
            Paint::rgb(255, 0, 0),
            Paint::rgb(0, 0, 255),
            Paint::rgb(255, 0, 0)
        ]
    );
}

#[test]
fn colour_does_not_move_anything() {
    let plain = scene("\\frac{a + b}{c}");
    let painted = scene("\\frac{\\textcolor{red}{a} + b}{\\color{blue} c}");
    assert_eq!(plain.glyphs.len(), painted.glyphs.len());
    for (a, b) in plain.glyphs.iter().zip(&painted.glyphs) {
        assert!((a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4);
    }
    assert!((plain.width - painted.width).abs() < 1e-4);
}

#[test]
fn tinting_a_node_after_layout_needs_no_relayout() {
    let mut s = scene("\\frac{abc}{xyz}");
    let before: Vec<_> = s.glyphs.iter().map(|g| (g.x, g.y)).collect();
    let frac = first_of(&s, NodeKind::Frac);

    let numerator = s
        .glyphs
        .iter()
        .find(|g| g.y < 0.0)
        .copied()
        .expect("a numerator glyph");
    let node = s.hit(numerator.x + 1.0, numerator.y - 1.0).expect("a hit");

    let gold = Paint::rgb(255, 200, 0);
    s.set_paint(node, gold);
    assert!(
        s.glyphs_of(node)
            .all(|g| s.resolve(g.paint()) == Some(gold)),
        "the node's own glyphs were not tinted"
    );
    assert!(
        s.glyphs.iter().any(|g| g.paint().is_inherit()),
        "tinting one node painted the whole formula"
    );
    assert_eq!(
        before,
        s.glyphs.iter().map(|g| (g.x, g.y)).collect::<Vec<_>>(),
        "tinting moved a glyph"
    );

    // The fraction covers the numerator, so tinting it reaches everything.
    s.set_paint(frac, gold);
    assert!(s.glyphs.iter().all(|g| s.resolve(g.paint()) == Some(gold)));
    assert!(s.rules.iter().all(|r| s.resolve(r.paint()) == Some(gold)));

    s.clear_paints();
    assert!(s.glyphs.iter().all(|g| g.paint().is_inherit()));
}

#[test]
fn an_explicit_tint_overrides_the_sources_own_colour() {
    let mut s = scene("\\textcolor{red}{x}");
    s.set_paint(NodeId::ROOT, Paint::rgb(0, 255, 0));
    assert!(
        s.glyphs
            .iter()
            .all(|g| s.resolve(g.paint()) == Some(Paint::rgb(0, 255, 0)))
    );
}
