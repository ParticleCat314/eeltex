//! Shape properties of `\middle`, `\genfrac`, `\cfrac`, starred matrices, `array` alignment and lengths.

#![cfg(feature = "embed-math-font")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use eeltex::{Expr, Fonts, Options, Scene, parse};

fn fonts() -> Fonts<'static> {
    Fonts::embedded().expect("the embedded font parses")
}

fn scene(src: &str) -> Scene {
    eeltex::render(src, &fonts(), &Options::default())
        .unwrap_or_else(|e| panic!("{src} did not lay out: {e}"))
}

#[test]
fn a_middle_grows_with_its_pair() {
    let tall = scene(r"\left( \frac{a}{\frac{b}{c}} \middle| x \right)");
    let short = scene(r"\left( x \middle| y \right)");
    assert!(tall.height() > short.height() * 1.5);
    assert!(
        parse(r"x \middle| y").is_err(),
        "\\middle outside a pair parsed"
    );
}

#[test]
fn a_middle_lays_out_the_same_as_a_hand_sized_bar() {
    let middle = scene(r"\left( \frac{a}{b} \middle| \frac{c}{d} \right)");
    let plain = scene(r"\left( \frac{a}{b} \right)");
    assert!((middle.height() - plain.height()).abs() < 0.5);
}

#[test]
fn genfrac_spells_every_fraction() {
    assert_eq!(parse(r"\genfrac{}{}{}{}{a}{b}"), parse(r"\frac{a}{b}"));
    assert_eq!(
        parse(r"\genfrac{(}{)}{0pt}{}{n}{k}"),
        parse(r"\binom{n}{k}")
    );
    assert_eq!(parse(r"\genfrac{}{}{}{0}{a}{b}"), parse(r"\dfrac{a}{b}"));
    assert_eq!(parse(r"{a \atop b}"), parse(r"{\genfrac{}{}{0pt}{}{a}{b}}"));
    assert!(scene(r"\genfrac{}{}{0pt}{}{a}{b}").rules.is_empty());
}

#[test]
fn a_continued_fraction_is_displayed_at_every_level() {
    let continued = scene(r"\cfrac{1}{1+\cfrac{1}{x}}");
    let plain = scene(r"\frac{1}{1+\frac{1}{x}}");
    assert!(continued.height() > plain.height());
}

#[cfg(feature = "attrs")]
#[test]
fn array_columns_align_as_the_spec_says() {
    let left = scene(r"\begin{array}{l} x \\ www \end{array}");
    let right = scene(r"\begin{array}{r} x \\ www \end{array}");
    assert!(left.glyphs[0].x < right.glyphs[0].x);
    let starred = scene(r"\begin{pmatrix*}[r] x \\ www \end{pmatrix*}");
    let plain = scene(r"\begin{pmatrix} x \\ www \end{pmatrix}");
    assert!(starred.glyphs[0].x > plain.glyphs[0].x);
}

#[test]
fn lengths_read_in_every_unit() {
    for (src, mu) in [
        (r"\hspace{1em}", 18),
        (r"\kern 2em", 36),
        (r"\mkern18mu", 18),
        (r"\mskip 9mu plus 2fill", 9),
        (r"\hskip-0.5em", -9),
        (r"\kern{10pt}", 18),
        (r"\enspace", 9),
        ("~", 6),
    ] {
        assert_eq!(parse(src), Ok(Expr::Space(mu)), "{src}");
    }
    assert!(parse(r"\kern 3").is_err());
}

#[cfg(not(feature = "formatting"))]
#[test]
fn formatting_commands_say_which_feature_they_need() {
    for src in [r"\boxed{x}", r"\cancel{x}", r"\Large x", r"\rule{1em}{1em}"] {
        let error = parse(src).unwrap_err().to_string();
        assert!(error.contains("`formatting` feature"), "{src}: {error}");
    }
    let error = parse(r"\begin{array}{c|c} a & b \end{array}")
        .unwrap_err()
        .to_string();
    assert!(error.contains("`formatting` feature"), "{error}");
}
