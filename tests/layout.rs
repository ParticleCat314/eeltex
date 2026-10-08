//! Layout invariants: relationships between parts, not pixel values.

// `clippy.toml`'s `allow-unwrap-in-tests` does not reach integration tests.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(feature = "embed-math-font")]

use eeltex::{Fonts, GlyphInstance, MathStyle, MissingGlyph, Options, Scene};

fn fonts() -> Fonts<'static> {
    Fonts::embedded().expect("the embedded font parses")
}

fn scene(src: &str) -> Scene {
    render(src, Options::default().with_font_size(40.0))
}

fn render(src: &str, options: Options) -> Scene {
    eeltex::render(src, &fonts(), &options).unwrap_or_else(|e| panic!("{src:?}: {e}"))
}

fn glyphs(scene: &Scene) -> &[GlyphInstance] {
    &scene.glyphs
}

#[test]
fn a_single_letter_has_ink_and_width() {
    let s = scene("x");
    assert_eq!(s.glyphs.len(), 1);
    assert!(s.width > 0.0, "no advance");
    assert!(s.ascent > 0.0, "no height above the baseline");
    assert!(s.descent >= 0.0);
    assert!(s.missing.is_empty());
}

#[test]
fn a_fraction_bar_separates_its_parts() {
    let s = scene("\\frac{a}{b}");
    assert_eq!(s.rules.len(), 1, "expected exactly one rule");
    let bar = s.rules.first().copied().expect("one rule");
    let g = glyphs(&s);
    let (num, den) = (
        g.first().copied().expect("numerator"),
        g.get(1).copied().expect("denominator"),
    );

    // y is down, so the numerator's baseline is above the bar (more negative).
    assert!(num.y < bar.y, "numerator {} not above bar {}", num.y, bar.y);
    assert!(
        den.y > bar.y,
        "denominator {} not below bar {}",
        den.y,
        bar.y
    );
    assert!(bar.height > 0.0, "invisible bar");
    assert!(
        (bar.width - s.width).abs() < 0.01,
        "bar {} does not span the fraction {}",
        bar.width,
        s.width
    );
}

#[test]
fn scripts_are_smaller_and_on_the_right_side_of_the_baseline() {
    let s = scene("x^2");
    let g = glyphs(&s);
    let (base, sup) = (g.first().copied().unwrap(), g.get(1).copied().unwrap());
    assert!(sup.size < base.size, "superscript was not scaled down");
    assert!(sup.y < 0.0, "superscript was not raised");

    let s = scene("x_2");
    let g = glyphs(&s);
    let (base, sub) = (g.first().copied().unwrap(), g.get(1).copied().unwrap());
    assert!(sub.size < base.size);
    assert!(sub.y > 0.0, "subscript was not lowered");
}

#[test]
fn script_scaling_bottoms_out() {
    let s = scene("x^{y^{z^{w}}}");
    let sizes: Vec<f32> = glyphs(&s).iter().map(|g| g.size).collect();
    let (Some(a), Some(b), Some(c), Some(d)) =
        (sizes.first(), sizes.get(1), sizes.get(2), sizes.get(3))
    else {
        panic!("expected four glyphs, got {sizes:?}");
    };
    assert!(b < a, "first script did not shrink");
    assert!(c < b, "second script did not shrink");
    assert_eq!(c, d, "scriptscript is the floor");
}

#[test]
fn a_superscript_and_subscript_clear_each_other() {
    let s = scene("x_2^3");
    let g = glyphs(&s);
    // By position: scene item order is unspecified.
    let sup = g
        .iter()
        .copied()
        .min_by(|a, b| a.y.total_cmp(&b.y))
        .unwrap();
    let sub = g
        .iter()
        .copied()
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .unwrap();
    assert!(sup.y < 0.0 && sub.y > 0.0);
    assert!(
        sub.y - sup.y > 0.0,
        "the scripts did not separate: sup {} sub {}",
        sup.y,
        sub.y
    );
}

#[test]
fn display_style_gives_a_sum_its_limits() {
    let display = render(
        "\\sum_{n=1}^{k}",
        Options::default()
            .with_font_size(40.0)
            .with_style(MathStyle::Display),
    );
    let text = render(
        "\\sum_{n=1}^{k}",
        Options::default()
            .with_font_size(40.0)
            .with_style(MathStyle::Text),
    );
    assert!(
        display.height() > text.height(),
        "display {} was not taller than text {}",
        display.height(),
        text.height()
    );
    assert!(
        display.width < text.width,
        "limits above and below should be narrower than limits beside"
    );
}

#[test]
fn an_integral_keeps_its_limits_beside_it() {
    let plain = scene("\\int_0^1");
    let forced = scene("\\int\\limits_0^1");
    assert!(
        forced.height() > plain.height(),
        "\\limits did not stack them: {} vs {}",
        forced.height(),
        plain.height()
    );
    assert!(forced.width < plain.width);
}

#[test]
fn an_integrals_subscript_tucks_under_the_sign() {
    for source in ["\\int_0^1", "\\oint_C^D", "\\iint_S^T"] {
        let s = scene(source);
        let g = glyphs(&s);
        let sign = g.first().copied().expect("the operator");
        let sup = g
            .iter()
            .copied()
            .min_by(|a, b| a.y.total_cmp(&b.y))
            .expect("the superscript");
        let sub = g
            .iter()
            .copied()
            .max_by(|a, b| a.y.total_cmp(&b.y))
            .expect("the subscript");

        // The bare sign's right edge: where a script would hang without the tuck.
        let bare = source.split(['_', '^']).next().expect("the command");
        let advance = scene(bare).width;
        assert!(
            sub.x < advance,
            "{source}: subscript at {} did not tuck under the sign's {advance}",
            sub.x
        );
        assert!(
            sup.x > sub.x,
            "{source}: superscript at {} is not right of the subscript at {}",
            sup.x,
            sub.x
        );
        assert!(sub.x > sign.x, "{source}: the subscript sits on the sign");
    }
}

#[test]
fn the_correction_comes_back_only_for_a_subscript() {
    let advance = scene("\\int").width;
    let highest = |s: &eeltex::Scene| {
        glyphs(s)
            .iter()
            .copied()
            .min_by(|a, b| a.y.total_cmp(&b.y))
            .expect("sup")
    };
    let lone = highest(&scene("\\int^b"));
    let paired = highest(&scene("\\int_a^b"));

    assert!(
        lone.x > advance,
        "a lone superscript at {} did not clear the slant past {advance}",
        lone.x
    );
    assert!(
        (paired.x - advance).abs() < 0.01,
        "a paired superscript at {} is not on the sign's edge {advance}",
        paired.x
    );
}

#[test]
fn delimiters_grow_only_when_they_need_to() {
    let plain = scene("(x)");
    let small = scene("\\left( x \\right)");
    assert!(
        (small.height() - plain.height()).abs() < 0.01,
        "a short sub-formula got stretched brackets: {} vs {}",
        small.height(),
        plain.height()
    );

    let large = scene("\\left( \\frac{\\frac{a}{b}}{c} \\right)");
    assert!(
        large.height() > small.height() * 1.5,
        "the bracket did not grow: {} vs {}",
        large.height(),
        small.height()
    );
    let small_open = glyphs(&small).first().copied().unwrap();
    let large_first = glyphs(&large).first().copied().unwrap();
    assert!(
        large_first.glyph != small_open.glyph,
        "the same `(` glyph was used at both sizes"
    );
}

#[test]
fn a_very_tall_bracket_is_assembled_from_pieces() {
    let tall = scene(
        "\\left( \\begin{matrix} a \\\\ b \\\\ c \\\\ d \\\\ e \\\\ f \\\\ g \\end{matrix} \\right)",
    );
    let opening: Vec<_> = glyphs(&tall).iter().filter(|g| g.x < 1.0).collect();
    assert!(
        opening.len() > 1,
        "expected an assembled bracket, got {} glyph(s)",
        opening.len()
    );
    let mut ys: Vec<f32> = opening.iter().map(|g| g.y).collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    ys.dedup();
    assert_eq!(ys.len(), opening.len(), "assembly pieces overlap exactly");
}

#[test]
fn an_absent_delimiter_takes_no_room() {
    let with = scene("\\left( x \\right)");
    let without = scene("\\left( x \\right.");
    assert!(without.width < with.width);
}

#[test]
fn layout_is_linear_in_the_font_size() {
    let source = "\\sum_{n=1}^{\\infty} \\frac{1}{n^2} = \\sqrt{\\frac{\\pi}{6}}";
    let small = render(source, Options::default().with_font_size(16.0));
    let large = render(source, Options::default().with_font_size(32.0));
    let scaled = small.scaled(2.0);

    assert!((large.width - scaled.width).abs() < 1e-3, "width diverged");
    assert!(
        (large.ascent - scaled.ascent).abs() < 1e-3,
        "ascent diverged"
    );
    assert!(
        (large.descent - scaled.descent).abs() < 1e-3,
        "descent diverged"
    );
    assert_eq!(large.glyphs.len(), scaled.glyphs.len());
    for (a, b) in large.glyphs.iter().zip(scaled.glyphs.iter()) {
        assert_eq!(a.glyph, b.glyph);
        assert!((a.x - b.x).abs() < 1e-3, "x diverged: {} vs {}", a.x, b.x);
        assert!((a.y - b.y).abs() < 1e-3, "y diverged: {} vs {}", a.y, b.y);
    }
}

#[test]
fn a_unary_minus_is_tighter_than_a_binary_one() {
    let unary = scene("-x");
    let binary = scene("y-x");
    let y_width = scene("y").width;
    assert!(
        binary.width - y_width > unary.width + 1.0,
        "the binary minus was not spaced: {} vs {}",
        binary.width - y_width,
        unary.width
    );
}

#[test]
fn script_styles_drop_the_spacing_around_an_operator() {
    let on_the_line = scene("a+b").width - scene("ab").width;
    let in_a_script = scene("x^{a+b}").width - scene("x^{ab}").width;

    let scale = 0.7; // roughly `scriptPercentScaleDown`
    assert!(
        in_a_script < on_the_line * scale,
        "the script kept its spacing: {in_a_script} vs {on_the_line} at full size"
    );
}

#[test]
fn a_radical_draws_a_visible_rule_over_its_radicand() {
    let s = scene("\\sqrt{x}");
    let rule = s.rules.first().copied().expect("a radical has a rule");
    assert!(rule.height > 0.0, "invisible radical rule");
    assert!(rule.width > 0.0);
    let radicand = glyphs(&s)
        .iter()
        .copied()
        .max_by(|a, b| a.x.total_cmp(&b.x))
        .expect("the x");
    assert!(
        rule.x <= radicand.x + 0.01,
        "the rule does not reach the radicand"
    );
    assert!(rule.x > 0.0, "no room was left for the surd");
}

#[test]
fn a_matrix_straddles_the_baseline() {
    let s = scene("\\begin{pmatrix} 1 & 2 \\\\ 3 & 4 \\end{pmatrix}");
    assert!(s.ascent > 0.0 && s.descent > 0.0, "not centred: {s:?}");
    let lopsided = (s.ascent - s.descent).abs() / s.height();
    assert!(lopsided < 0.35, "the block is off-centre by {lopsided}");
}

#[test]
fn align_lines_up_its_relations() {
    let s = scene("\\begin{aligned} f(x) &= x \\\\ y &= z \\end{aligned}");
    let g = glyphs(&s);

    let mut rows: Vec<f32> = g.iter().map(|i| i.y).collect();
    rows.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    rows.dedup();
    assert!(rows.len() >= 2, "expected two rows, got {rows:?}");

    let first_row_x: Vec<f32> = g
        .iter()
        .filter(|i| Some(i.y) == rows.first().copied())
        .map(|i| i.x)
        .collect();
    let second_row_x: Vec<f32> = g
        .iter()
        .filter(|i| Some(i.y) == rows.get(1).copied())
        .map(|i| i.x)
        .collect();
    // `f ( x ) = x` against `y = z`: the relations are the 5th and 2nd glyphs.
    let (Some(a), Some(b)) = (first_row_x.get(4), second_row_x.get(1)) else {
        panic!("unexpected glyph counts: {first_row_x:?} {second_row_x:?}");
    };
    assert!(
        (a - b).abs() < 0.01,
        "the relations did not line up: {a} vs {b}"
    );
}

#[test]
fn a_missing_glyph_is_reported_not_fatal() {
    // Fraktur digits fall back to plain ones, so use a glyph the font genuinely lacks.
    let source = "x + \\mathfrak{q}";
    let notdef = eeltex::render(source, &fonts(), &Options::default());
    assert!(notdef.is_ok(), "a missing glyph should not fail by default");

    let strict = Options::default().with_missing_glyph(MissingGlyph::Error);
    match (eeltex::render(source, &fonts(), &strict), notdef) {
        (Ok(_), Ok(s)) => assert!(s.missing.is_empty()),
        (Err(_), Ok(s)) => assert!(!s.missing.is_empty(), "failed without reporting why"),
        (a, b) => panic!("inconsistent: {a:?} {b:?}"),
    }
}

#[test]
fn a_missing_delimiter_follows_the_same_policy() {
    use eeltex::{Delim, Expr};
    // A supplementary private-use character no font draws.
    let expr = Expr::Delimited {
        left: Delim::Char('\u{10FFFD}'),
        body: Box::new(eeltex::parse("x").unwrap()),
        right: Delim::None,
    };
    let scene = eeltex::layout(&expr, &fonts(), &Options::default()).unwrap();
    assert_eq!(scene.missing, vec!['\u{10FFFD}']);

    let strict = Options::default().with_missing_glyph(MissingGlyph::Error);
    assert!(eeltex::layout(&expr, &fonts(), &strict).is_err());
}

#[test]
fn the_whole_corpus_lays_out_finitely() {
    let fonts = fonts();
    for src in [
        "\\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}",
        "\\int_{0}^{\\infty} e^{-x^2} \\, dx = \\frac{\\sqrt{\\pi}}{2}",
        "\\left( \\frac{\\partial f}{\\partial x} \\right)_{y}",
        "\\mathbb{R}^{n} \\text{ with } n \\geq 2",
        "\\hat{a} \\widetilde{bc} \\overline{d} \\underline{e}",
        "\\begin{cases} 1 & x > 0 \\\\ 0 & \\text{otherwise} \\end{cases}",
        "\\begin{Vmatrix} a & b \\\\ c & d \\end{Vmatrix}",
        "\\sqrt[3]{\\frac{x}{y}}",
        "f''(x) = \\lim_{h \\to 0} \\frac{f'(x+h) - f'(x)}{h}",
    ] {
        let scene = eeltex::render(src, &fonts, &Options::default())
            .unwrap_or_else(|e| panic!("{src:?}: {e}"));
        assert!(
            scene.width.is_finite() && scene.height().is_finite(),
            "{src:?}"
        );
        assert!(scene.width > 0.0, "{src:?} laid out to nothing");
    }
}

/// Lowest ink of a glyph instance in scene coordinates (y-down).
fn ink_bottom(fonts: &Fonts<'_>, glyph: &GlyphInstance) -> f32 {
    let bbox = fonts
        .math
        .bbox(glyph.glyph)
        .expect("an inked glyph has a bounding box");
    glyph.y - f32::from(bbox.y_min) * glyph.size / fonts.math.units_per_em()
}

#[test]
fn a_radical_hangs_no_further_below_than_it_clears_above() {
    for (radicand, root) in [
        ("x", "\\sqrt{x}"),
        ("\\sqrt{x}", "\\sqrt{\\sqrt{x}}"),
        ("\\frac{a}{b}", "\\sqrt{\\frac{a}{b}}"),
    ] {
        let inner = scene(radicand);
        let outer = scene(root);
        // The outermost rule is the highest on the page (smallest `y`).
        let bar = outer
            .rules
            .iter()
            .map(|r| r.y)
            .fold(f32::INFINITY, f32::min);
        assert!(bar.is_finite(), "{root}: a radical has a rule");

        // `Rule::y` is the top edge.
        let above = -bar - inner.ascent;
        let below = outer.descent - inner.descent;
        assert!(
            below <= above + 1e-3,
            "{root}: hangs {below} below its radicand but clears only {above} above it"
        );
    }
}

#[test]
fn an_accent_sits_above_its_base_however_tall() {
    let fonts = fonts();
    for base in ["a", "A", "f", "\\theta", "i"] {
        let plain = scene(base);
        let hatted = scene(&format!("\\hat{{{base}}}"));
        assert_eq!(hatted.glyphs.len(), 2, "{base}: expected a base and a hat");

        let accent = hatted.glyphs.last().copied().expect("the hat");
        assert!(
            ink_bottom(&fonts, &accent) <= -plain.ascent + 1e-3,
            "\\hat{{{base}}}: the hat is buried inside the base"
        );
        assert!(
            hatted.ascent > plain.ascent,
            "{base}: the hat added no height"
        );
    }
}

#[test]
fn a_tall_base_carries_its_accent_higher() {
    let fonts = fonts();
    let short = scene("\\hat{a}");
    let tall = scene("\\hat{A}");
    let hat = |s: &Scene| ink_bottom(&fonts, s.glyphs.last().expect("the hat"));
    assert!(
        hat(&tall) < hat(&short) - 1e-3,
        "the hat on an A sits no higher than the one on an a"
    );
}
