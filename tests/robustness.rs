//! Hostile input must produce an error, never a panic.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// Deterministic LCG, so the corpus is identical on every run.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() >> 8) as usize % n.max(1)
    }
}

const ALPHABET: &[&str] = &[
    "\\",
    "{",
    "}",
    "^",
    "_",
    "&",
    "\\\\",
    "[",
    "]",
    "(",
    ")",
    // Commands, whole and truncated.
    "\\frac",
    "\\sqrt",
    "\\left",
    "\\right",
    "\\begin",
    "\\end",
    "\\text",
    "\\color",
    "\\sum",
    "\\int",
    "\\limits",
    "\\not",
    "\\mathbb",
    "\\fra",
    "\\begi",
    "\\lef",
    "\\",
    "\\ ",
    "\\%",
    "\\@",
    "matrix",
    "aligned",
    "cases",
    "align*",
    "nosuchenv",
    "a",
    "x",
    "1",
    "0",
    " ",
    "+",
    "-",
    "=",
    ",",
    ".",
    "'",
    "\"",
    // Non-LaTeX and multi-byte characters, to catch byte-wise slicing.
    "α",
    "∑",
    "𝔸",
    "🜛",
    "\u{0}",
    "\u{7f}",
    "\t",
    "\n",
    "\r",
    "%",
    "#",
    "$",
    "~",
];

fn concoct(rng: &mut Rng, pieces: usize) -> String {
    let mut out = String::new();
    for _ in 0..pieces {
        if let Some(piece) = ALPHABET.get(rng.below(ALPHABET.len())) {
            out.push_str(piece);
        }
    }
    out
}

/// Fixed seeds and lengths, so a failure is reproducible from its seed.
fn corpus() -> Vec<String> {
    let mut out = Vec::new();
    for seed in 0..400u32 {
        let mut rng = Rng(seed.wrapping_mul(2_654_435_761).wrapping_add(1));
        for pieces in [1usize, 2, 5, 13, 40] {
            out.push(concoct(&mut rng, pieces));
        }
    }

    // Shapes that have historically broken parsers.
    for hand in [
        "",
        " ",
        "{",
        "}",
        "{}}{",
        "\\",
        "\\\\",
        "^",
        "_",
        "^^^^^^",
        "_^_^_^",
        "x^",
        "x_",
        "\\frac",
        "\\frac{",
        "\\frac{}",
        "\\frac{}{}",
        "\\left",
        "\\left(",
        "\\right)",
        "\\left(\\right",
        "\\begin",
        "\\begin{",
        "\\begin{matrix}",
        "\\end{matrix}",
        "\\begin{matrix}\\end{cases}",
        "\\text",
        "\\text{",
        "\\text{{}",
        "\\color",
        "\\color{}",
        "\\color{#}",
        "\\color{rgb(}",
        "\\not",
        "\\not\\not\\not",
        "&&&&",
        "\\\\\\\\\\\\",
        "\u{0}",
        "\u{feff}",
        "🜛^🜛_🜛",
        "\\sqrt[",
        "\\sqrt[]",
        "\\sqrt[]{}",
    ] {
        out.push(hand.to_owned());
    }
    out
}

/// Reaching the end is the assertion.
#[test]
fn nothing_in_the_corpus_panics_the_parser() {
    for source in corpus() {
        // `parse_with` turns on spans, which are byte offsets into the source.
        let _ = eeltex::parse(&source);
        let _ = eeltex::parse_with(&source, &eeltex::ParseOptions::default().with_spans(true));
    }
}

#[test]
fn a_parse_error_points_somewhere_the_source_can_be_split() {
    for source in corpus() {
        let Err(eeltex::Error::Parse { offset, .. }) = eeltex::parse(&source) else {
            continue;
        };
        assert!(
            offset <= source.len(),
            "{source:?} failed at byte {offset}, past its {} bytes",
            source.len()
        );
        assert!(
            source.is_char_boundary(offset),
            "{source:?} failed at byte {offset}, which is inside a character"
        );
        let _ = &source[..offset];
    }
}

#[test]
fn repair_always_produces_something_that_parses() {
    for source in corpus() {
        let repaired = eeltex::partial::repair(&source);
        assert!(
            eeltex::parse(&repaired).is_ok(),
            "repair({source:?}) gave {repaired:?}, which does not parse"
        );
    }
}

#[test]
fn repair_is_idempotent_on_hostile_input() {
    for source in corpus() {
        let once = eeltex::partial::repair(&source);
        let twice = eeltex::partial::repair(&once);
        assert_eq!(once, twice, "repairing {source:?} twice changed it");
    }
}

#[test]
fn every_prefix_of_the_corpus_repairs_and_parses() {
    for source in corpus() {
        for (at, _) in source.char_indices() {
            let prefix = &source[..at];
            let repaired = eeltex::partial::repair(prefix);
            assert!(
                eeltex::parse(&repaired).is_ok(),
                "the prefix {prefix:?} repaired to {repaired:?}, which does not parse"
            );
        }
    }
}

#[cfg(feature = "embed-math-font")]
#[test]
fn nothing_that_parses_panics_the_layout() {
    let fonts = eeltex::Fonts::embedded().expect("the embedded font parses");

    for source in corpus() {
        let Ok(expr) = eeltex::parse(&source) else {
            continue;
        };
        for size in [1e-3f32, 1.0, 16.0, 4096.0] {
            for style in [eeltex::MathStyle::Display, eeltex::MathStyle::Text] {
                let options = eeltex::Options::default()
                    .with_font_size(size)
                    .with_style(style);
                if let Ok(scene) = eeltex::layout(&expr, &fonts, &options) {
                    assert!(
                        scene.width.is_finite()
                            && scene.ascent.is_finite()
                            && scene.descent.is_finite(),
                        "{source:?} at {size} laid out to a non-finite box"
                    );
                }
            }
        }
    }
}

#[cfg(feature = "embed-math-font")]
#[test]
fn a_nonsense_font_size_does_not_produce_a_nonsense_scene() {
    let fonts = eeltex::Fonts::embedded().expect("the embedded font parses");
    let expr = eeltex::parse(r"\frac{a}{b}").expect("it parses");

    for size in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0, 0.0] {
        let options = eeltex::Options::default().with_font_size(size);
        match eeltex::layout(&expr, &fonts, &options) {
            Err(_) => {}
            Ok(scene) => assert!(
                scene.width.is_finite() && scene.ascent.is_finite() && scene.descent.is_finite(),
                "a font size of {size} was accepted and gave a non-finite box"
            ),
        }
    }
}

#[test]
fn rubbish_bytes_are_not_a_font() {
    assert!(eeltex::Fonts::new(&[]).is_err());
    assert!(eeltex::Fonts::new(&[0u8; 16]).is_err());
    assert!(eeltex::Fonts::new(&[0xffu8; 4096]).is_err());
    // An OpenType header with nothing behind it.
    let mut truncated = b"OTTO\x00\x0c\x00\x80\x00\x03\x00\x20".to_vec();
    truncated.resize(64, 0);
    assert!(eeltex::Fonts::new(&truncated).is_err());
}

#[cfg(all(feature = "svg", feature = "embed-math-font"))]
#[test]
fn svg_output_never_contains_a_non_finite_number() {
    let fonts = eeltex::Fonts::embedded().expect("the embedded font parses");

    for source in corpus() {
        let Ok(expr) = eeltex::parse(&source) else {
            continue;
        };
        for size in [1e-3f32, 16.0, 4096.0] {
            let options = eeltex::Options::default().with_font_size(size);
            let Ok(scene) = eeltex::layout(&expr, &fonts, &options) else {
                continue;
            };
            let Ok(svg) = eeltex::export::svg::to_svg(
                &scene,
                &fonts,
                &eeltex::export::svg::SvgOptions::default(),
            ) else {
                continue;
            };
            assert!(
                !svg.contains("NaN") && !svg.contains("inf"),
                "{source:?} at {size} wrote a non-finite number into the document"
            );
        }
    }
}

#[cfg(all(feature = "raster", feature = "embed-math-font"))]
#[test]
fn the_rasteriser_survives_absurd_sizes() {
    let fonts = eeltex::Fonts::embedded().expect("the embedded font parses");
    let scene = eeltex::render(
        r"\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}",
        &fonts,
        &eeltex::Options::default(),
    )
    .expect("the quadratic lays out");

    for size in [
        0.0,
        -1.0,
        1e-6,
        0.5,
        1.0,
        16.0,
        1e5,
        1e30,
        f32::NAN,
        f32::INFINITY,
    ] {
        for glyph in &scene.glyphs {
            if let Some(coverage) =
                eeltex::export::raster::rasterise_glyph(&fonts.math, glyph.glyph, size)
            {
                assert_eq!(
                    coverage.data.len(),
                    coverage.width as usize * coverage.height as usize,
                    "a {size}px bitmap's buffer does not match its dimensions"
                );
            }
        }
    }
}

#[cfg(all(feature = "sdf", feature = "embed-math-font"))]
#[test]
fn the_distance_field_survives_absurd_parameters() {
    let fonts = eeltex::Fonts::embedded().expect("the embedded font parses");
    let scene = eeltex::render(r"\sqrt{x}", &fonts, &eeltex::Options::default())
        .expect("a radical lays out");

    for size in [0.0, -1.0, 1e-6, 48.0, 1e30, f32::NAN] {
        for spread in [0.0, -4.0, 1e-6, 4.0, 1e30, f32::NAN] {
            for glyph in &scene.glyphs {
                if let Some(field) =
                    eeltex::export::sdf::sdf_glyph(&fonts.math, glyph.glyph, size, spread)
                {
                    assert_eq!(
                        field.data.len(),
                        field.width as usize * field.height as usize,
                        "a {size}px/{spread} field's buffer does not match its dimensions"
                    );
                }
            }
        }
    }
}

#[cfg(all(feature = "atlas", feature = "sdf", feature = "embed-math-font"))]
#[test]
fn an_atlas_too_small_for_its_own_patches_still_terminates() {
    use eeltex::export::atlas::{Atlas, Source};

    let fonts = eeltex::Fonts::embedded().expect("the embedded font parses");
    let scene = eeltex::render(
        r"\frac{a}{b} + \sqrt{x^2}",
        &fonts,
        &eeltex::Options::default(),
    )
    .expect("it lays out");

    for (width, height) in [
        (1, 1),
        (2, 2),
        (8, 8),
        (16, 16),
        (31, 31),
        (64, 64),
        (33, 1),
    ] {
        let mut atlas = Atlas::new(width, height, Source::signed_distance());
        atlas.prepare(&scene, &fonts);
        let packed = atlas.len();
        atlas.prepare(&scene, &fonts);
        assert_eq!(
            packed,
            atlas.len(),
            "a {width}x{height} atlas packed differently the second time"
        );
        assert_eq!(
            atlas.data().len(),
            atlas.width() as usize * atlas.height() as usize,
            "a {width}x{height} atlas's buffer does not match its dimensions"
        );
    }
}
