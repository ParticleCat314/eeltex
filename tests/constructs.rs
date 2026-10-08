//! Shape properties of `\binom`, `\overset`, braces, arrows, `\not`, `\phantom`, class overrides and `\substack`.

#![cfg(feature = "embed-math-font")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use eeltex::{Fonts, Options, Scene};

fn fonts() -> Fonts<'static> {
    Fonts::embedded().expect("the embedded font parses")
}

fn scene(src: &str) -> Scene {
    eeltex::render(src, &fonts(), &Options::default())
        .unwrap_or_else(|e| panic!("{src} did not lay out: {e}"))
}

fn width(src: &str) -> f32 {
    scene(src).width
}

#[test]
fn a_binomial_has_no_bar() {
    let binom = scene(r"\binom{n}{k}");
    let frac = scene(r"\frac{n}{k}");
    assert!(
        binom.rules.is_empty(),
        "\\binom drew {} rule(s)",
        binom.rules.len()
    );
    assert_eq!(frac.rules.len(), 1, "\\frac stopped drawing its bar");
    assert_eq!(binom.glyphs.len(), frac.glyphs.len() + 2);
}

#[test]
fn a_binomial_is_taller_than_its_contents() {
    assert!(scene(r"\binom{n}{k}").height() > scene(r"(n)").height() * 1.4);
}

#[test]
fn the_binomial_styles_differ() {
    assert!(
        width(r"\dbinom{n}{k}") > width(r"\tbinom{n}{k}"),
        "\\dbinom is no bigger than \\tbinom"
    );
}

#[test]
fn a_phantom_is_room_without_ink() {
    let ghost = scene(r"\phantom{xyz}");
    let real = scene(r"xyz");
    assert!(ghost.glyphs.is_empty(), "a phantom drew something");
    assert!(ghost.rules.is_empty());
    assert!((ghost.width - real.width).abs() < 0.01, "widths differ");
    assert!((ghost.ascent - real.ascent).abs() < 0.01);
    assert!((ghost.descent - real.descent).abs() < 0.01);
}

#[test]
fn the_two_halves_of_a_phantom_keep_one_dimension_each() {
    let across = scene(r"\hphantom{xyz}");
    assert!((across.width - width("xyz")).abs() < 0.01);
    assert_eq!(across.height(), 0.0, "\\hphantom kept its height");

    let down = scene(r"\vphantom{X}");
    assert_eq!(down.width, 0.0, "\\vphantom kept its width");
    assert!((down.height() - scene("X").height()).abs() < 0.01);
}

#[test]
fn the_class_overrides_reach_the_spacing_table() {
    // `{…}` forces ordinary, so the difference is exactly the two gaps the class adds.
    let added = |src: &str, ord: &str| width(src) - width(ord);
    let binary = added(r"a\mathbin{x}b", "axb");
    let relation = added(r"a\mathrel{x}b", "axb");
    assert!(binary > 0.0, "\\mathbin added no space");
    assert!(
        relation > binary,
        "\\mathrel ({relation}) added no more than \\mathbin ({binary})"
    );
    assert!(
        (binary - added("a+b", "a{+}b")).abs() < 0.01,
        "\\mathbin ({binary}) did not add what + adds ({})",
        added("a+b", "a{+}b")
    );
    assert!(
        (relation - added("a=b", "a{=}b")).abs() < 0.01,
        "\\mathrel ({relation}) did not add what = adds ({})",
        added("a=b", "a{=}b")
    );
}

#[test]
fn a_class_override_draws_the_same_glyphs() {
    let plain = scene(r"\star");
    let bound = scene(r"\mathbin{\star}");
    assert_eq!(plain.glyphs.len(), bound.glyphs.len());
    assert_eq!(
        plain.glyphs.first().map(|g| g.glyph),
        bound.glyphs.first().map(|g| g.glyph)
    );
}

#[test]
fn an_overset_label_sits_above_the_base() {
    let s = scene(r"\overset{a}{=}");
    assert_eq!(s.glyphs.len(), 2);
    let base = s.glyphs.first().unwrap();
    let label = s.glyphs.get(1).unwrap();
    // y is down, so above means a smaller y.
    assert!(
        label.y < base.y,
        "the label is at {} and the base at {}",
        label.y,
        base.y
    );
    assert!(label.size < base.size);
}

#[test]
fn an_underset_label_sits_below_the_base() {
    let s = scene(r"\underset{a}{=}");
    let base = s.glyphs.first().unwrap();
    let label = s.glyphs.get(1).unwrap();
    assert!(label.y > base.y);
    assert!(label.size < base.size);
}

#[test]
fn stackrel_is_a_relation_and_overset_is_not() {
    let relation = width(r"a\stackrel{f}{g}b");
    let ordinary = width(r"a\overset{f}{g}b");
    assert!(
        relation > ordinary,
        "\\stackrel ({relation}) spaced no wider than \\overset ({ordinary})"
    );
}

#[test]
fn a_brace_stretches_to_its_base() {
    let narrow = scene(r"\overbrace{a}");
    let wide = scene(r"\overbrace{abcdefgh}");
    let brace_of = |s: &Scene| s.glyphs.last().map(|g| g.glyph);
    assert_ne!(
        brace_of(&narrow),
        brace_of(&wide),
        "the brace did not stretch: the same glyph covered both bases"
    );
}

#[test]
fn a_brace_clears_what_it_covers() {
    let fonts = fonts();
    let s = scene(r"\overbrace{a+b}");
    // Ink, not boxes: a box says nothing about where the strokes are.
    let ink_top = |g: &eeltex::GlyphInstance| {
        let top = fonts
            .math
            .bbox(g.glyph)
            .map_or(0.0, |b| fonts.math.px(f32::from(b.y_max), g.size));
        g.y - top
    };
    let ink_bottom = |g: &eeltex::GlyphInstance| {
        let bottom = fonts
            .math
            .bbox(g.glyph)
            .map_or(0.0, |b| fonts.math.px(f32::from(b.y_min), g.size));
        g.y - bottom
    };
    let (brace, base) = s.glyphs.split_last().expect("glyphs");
    let base_top = base.iter().map(ink_top).fold(f32::INFINITY, f32::min);
    assert!(
        ink_bottom(brace) < base_top,
        "the brace's ink reaches down to {} and the base's up to {} — they overlap",
        ink_bottom(brace),
        base_top
    );
}

#[test]
fn a_brace_takes_its_script_as_a_label() {
    let s = scene(r"\underbrace{x+y}_{n}");
    // The label is the last glyph emitted and the only one at script size.
    let base_size = s.glyphs.first().unwrap().size;
    let label = s
        .glyphs
        .iter()
        .find(|g| g.size < base_size)
        .expect("no smaller glyph — the label was not set at script size");
    let base_bottom = s
        .glyphs
        .iter()
        .filter(|g| g.size == base_size)
        .map(|g| g.y)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        label.y > base_bottom,
        "the label is at y {} and the base at {}",
        label.y,
        base_bottom
    );
    assert!(
        label.x > 0.0 && label.x < s.width,
        "the label at x {} is outside the brace's span (0..{})",
        label.x,
        s.width
    );
}

#[test]
fn a_script_on_the_other_side_of_a_brace_is_still_a_script() {
    let s = scene(r"\overbrace{x}_{i}");
    let base_size = s.glyphs.first().unwrap().size;
    let script = s.glyphs.iter().find(|g| g.size < base_size).unwrap();
    assert!(
        script.x >= scene("x").width - 0.01,
        "the subscript was tucked under the brace instead of set beside it"
    );
}

#[test]
fn an_extensible_arrow_grows_to_its_label() {
    let short = width(r"\xrightarrow{f}");
    let long = width(r"\xrightarrow{\text{a much longer label}}");
    assert!(
        long > short * 2.0,
        "the arrow did not stretch: {short} against {long}"
    );
    // Measured within one scene: the label is set at script size.
    let s = scene(r"\xrightarrow{\text{a much longer label}}");
    let arrow_size = s.glyphs.iter().map(|g| g.size).fold(0.0f32, f32::max);
    let span = |wanted_arrow: bool| {
        s.glyphs
            .iter()
            .filter(|g| (g.size == arrow_size) == wanted_arrow)
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), g| {
                (lo.min(g.x), hi.max(g.x))
            })
    };
    let (arrow_left, arrow_right) = span(true);
    let (label_left, label_right) = span(false);
    assert!(
        arrow_left <= label_left && arrow_right >= label_right,
        "the arrow spans {arrow_left}..{arrow_right} and its label {label_left}..{label_right}"
    );
}

#[test]
fn an_unlabelled_extensible_arrow_is_still_an_arrow() {
    let bare = scene(r"\xrightarrow{}");
    assert!(!bare.glyphs.is_empty());
    assert!(bare.width >= scene(r"\rightarrow").width - 0.01);
}

#[test]
fn an_extensible_arrow_takes_a_label_on_each_side() {
    let s = scene(r"\xrightarrow[g]{f}");
    let both = scene(r"\xrightarrow{f}");
    assert!(s.descent > both.descent, "the lower label added no depth");
    assert!(s.ascent >= both.ascent - 0.01);
}

#[test]
fn an_extensible_arrow_is_a_relation() {
    let arrow = width(r"A\xrightarrow{f}B") - width(r"\xrightarrow{f}");
    let plain = width(r"A\to B") - width(r"\to");
    assert!(
        (arrow - plain).abs() < 0.01,
        "the arrow spaced as {arrow} where a relation spaces as {plain}"
    );
}

#[test]
fn a_negation_overlays_its_base() {
    let s = scene(r"\not=");
    assert_eq!(s.glyphs.len(), 2, "expected the base and the overlay");
    let base = s.glyphs.first().unwrap();
    let slash = s.glyphs.get(1).unwrap();
    assert!(
        (base.y - slash.y).abs() < 0.01,
        "the overlay is not on the baseline"
    );
    assert!(
        slash.x < base.x + scene("=").width,
        "the overlay at x {} starts past the base",
        slash.x
    );
}

#[test]
fn a_negation_keeps_the_class_it_negates() {
    let negated = width(r"a\not=b");
    let plain = width("a=b");
    // The overlay adds no advance.
    assert!(
        (negated - plain).abs() < 0.01,
        "\\not= ({negated}) spaces differently from = ({plain})"
    );
}

#[test]
fn a_substack_is_one_centred_column() {
    let two = scene(r"\substack{a\\bb}");
    let one = scene(r"\substack{a}");
    assert!(two.height() > one.height() * 1.5, "the rows did not stack");
    assert!(
        (two.width - scene(r"\substack{bb}").width).abs() < 0.01,
        "the stack is not as wide as its widest row"
    );
}

#[test]
fn a_substack_is_set_at_script_size() {
    let stacked = scene(r"\substack{a}");
    let plain = scene("a");
    let size_of = |s: &Scene| s.glyphs.first().map(|g| g.size).unwrap_or_default();
    assert!(
        size_of(&stacked) < size_of(&plain),
        "a substack was set at full size"
    );
}

#[test]
fn a_substack_works_as_a_limit() {
    let s = scene(r"\sum_{\substack{i<j \\ j<k}} a_{ij}");
    assert!(s.glyphs.len() > 8);
    let biggest = s
        .glyphs
        .iter()
        .max_by(|a, b| a.size.total_cmp(&b.size))
        .unwrap();
    let lowest = s
        .glyphs
        .iter()
        .map(|g| g.y)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(lowest > biggest.y, "nothing was set below the operator");
}
