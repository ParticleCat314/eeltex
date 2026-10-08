//! The `formatting` feature: size switches, boxes, cancels, rules, `\smash`, the laps,
//! `\raisebox`, and the rules of `array`.

#![cfg(all(feature = "embed-math-font", feature = "formatting"))]
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
fn size_switches_scale_from_the_base_size() {
    let huge = scene(r"\Huge x").height();
    let normal = scene(r"x").height();
    let tiny = scene(r"\tiny x").height();
    assert!((huge / normal - 2.488).abs() < 0.05, "{huge} / {normal}");
    assert!((tiny / normal - 0.5).abs() < 0.05);
    // Not cumulative: `\small` inside `\Huge` is small.
    let nested = scene(r"\Huge {\small x}").height();
    assert!((nested / normal - 0.9).abs() < 0.05);
}

#[test]
fn a_box_is_wider_and_taller_than_what_it_holds() {
    let boxed = scene(r"\boxed{x}");
    let bare = scene("x");
    assert!(boxed.width > bare.width + 0.5 * 16.0);
    assert!(boxed.ascent > bare.ascent && boxed.descent > bare.descent);
    assert_eq!(boxed.rules.len(), 4);
}

#[test]
fn a_colour_box_fills_behind_and_frames_nothing() {
    let filled = scene(r"\colorbox{yellow}{x}");
    assert_eq!(filled.backgrounds.len(), 1);
    assert!(filled.rules.is_empty());
    let framed = scene(r"\fcolorbox{red}{yellow}{x}");
    assert_eq!(framed.backgrounds.len(), 1);
    assert_eq!(framed.rules.len(), 4);
    // Clearing paints must not turn the fill into ink over the text.
    let mut cleared = filled.clone();
    cleared.clear_paints();
    assert_eq!(cleared.backgrounds, filled.backgrounds);
}

#[test]
fn a_text_box_reads_its_maths() {
    let Ok(Expr::Enclose { body, .. }) = parse(r"\fbox{area $\pi r^2$}") else {
        panic!("\\fbox did not parse to an enclosure");
    };
    let Expr::Seq(items) = *body else {
        panic!("expected text then maths");
    };
    assert!(matches!(items.first(), Some(Expr::Text(text)) if text == "area "));
    assert_eq!(items.len(), 2);
    assert!(parse(r"\fbox{open $x}").is_err());
}

#[test]
fn cancels_draw_strokes_across_the_body() {
    assert_eq!(scene(r"\cancel{x}").strokes.len(), 1);
    assert_eq!(scene(r"\bcancel{x}").strokes.len(), 1);
    assert_eq!(scene(r"\xcancel{x}").strokes.len(), 2);
    // The shaft and the two barbs of the arrowhead.
    let to = scene(r"\cancelto{0}{x}");
    assert_eq!(to.strokes.len(), 3);
    assert!(to.width > scene("x").width, "the value was not set beside");
    let up = scene(r"\cancel{x}").strokes[0];
    assert!(
        up.x1 > up.x0 && up.y1 < up.y0,
        "\\cancel does not rise left to right"
    );
    assert_eq!(scene(r"\sout{x}").rules.len(), 1);
    assert_eq!(scene(r"\cancel{x}").width, scene("x").width);
}

#[test]
fn a_rule_is_as_big_as_it_says() {
    let size = Options::default().font_size;
    let rule = scene(r"\rule{2em}{1em}");
    assert_eq!(rule.rules.len(), 1);
    assert!((rule.width - 2.0 * size).abs() < 0.01);
    assert!((rule.ascent - size).abs() < 0.01);
    let lowered = scene(r"\rule[-0.5em]{1em}{1em}");
    assert!((lowered.descent - 0.5 * size).abs() < 0.01);
    let strut = scene(r"\strut");
    assert!(strut.rules.is_empty() && strut.width == 0.0 && strut.height() > size);
}

#[test]
fn smash_and_the_laps_give_up_a_dimension() {
    let frac = scene(r"\frac{a}{b}");
    let smashed = scene(r"\smash{\frac{a}{b}}");
    assert_eq!(smashed.height(), 0.0);
    assert_eq!(smashed.width, frac.width);
    assert_eq!(scene(r"\smash[b]{y}").descent, 0.0);
    assert!(scene(r"\smash[t]{y}").descent > 0.0);
    assert_eq!(scene(r"\mathrlap{xyz}").width, 0.0);
    let left = scene(r"\mathllap{x}");
    assert!(left.glyphs[0].x < 0.0, "\\llap did not hang left");
}

#[test]
fn raisebox_moves_ink_and_box_together() {
    let size = Options::default().font_size;
    let plain = scene("x");
    let raised = scene(r"\raisebox{1em}{x}");
    assert!((raised.ascent - plain.ascent - size).abs() < 0.01);
    assert!((plain.glyphs[0].y - raised.glyphs[0].y - size).abs() < 0.01);
}

#[test]
fn hlines_and_column_rules_cross_an_array() {
    let ruled = scene(r"\begin{array}{|c|c|} \hline a & b \\ \hline c & d \\ \hline \end{array}");
    let horizontal = ruled.rules.iter().filter(|r| r.width > r.height).count();
    let vertical = ruled.rules.iter().filter(|r| r.height > r.width).count();
    assert_eq!(horizontal, 3);
    assert_eq!(vertical, 3);
    let dashed = scene(r"\begin{array}{c} a \\ \hdashline b \end{array}");
    assert!(
        dashed.rules.len() > 3,
        "a dashed line drew {}",
        dashed.rules.len()
    );
    assert!(parse(r"a \hline b").is_err());
}
