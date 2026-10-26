//! Core lives at the crate root; everything feature-gated sits behind a namespace.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[test]
fn root_is_the_core_pipeline() {
    let expr = eeltex::parse(r"\frac{a}{b}").expect("it parses");
    assert_eq!(
        eeltex::parse_with(r"\frac{a}{b}", &eeltex::ParseOptions::default()),
        Ok(expr)
    );

    let _ = eeltex::Options::default().with_font_size(32.0);
    let _ = eeltex::MissingGlyph::Notdef;
    let _ = eeltex::Scene::default();
    let _ = eeltex::MathStyle::Display;
    let _ = eeltex::Error::TooDeep;
    let _ = eeltex::Limits::Auto;
    let _ = eeltex::Fonts::new(&[]);
    let _ = eeltex::Span::new(1, 3);
}

#[test]
fn repair_is_unfeatured() {
    assert_eq!(eeltex::partial::repair(r"\frac{a}{b}"), r"\frac{a}{b}");
    assert!(eeltex::partial::is_complete(r"\frac{a}{b}"));
}

#[cfg(feature = "svg")]
#[test]
fn svg_is_under_export() {
    let _ = eeltex::export::svg::SvgOptions::default();
    let _ = eeltex::export::svg::SvgSprite::new(eeltex::export::svg::SpriteOptions::default());
}

#[cfg(feature = "raster")]
#[test]
fn raster_is_under_export() {
    let _ = eeltex::export::raster::Ink::default();
}

#[cfg(feature = "atlas")]
#[test]
fn atlas_is_under_export() {
    #[cfg(feature = "sdf")]
    let _ = eeltex::export::atlas::Atlas::new(
        512,
        2048,
        eeltex::export::atlas::Source::signed_distance(),
    );
    #[cfg(feature = "raster")]
    let _ = eeltex::export::atlas::Atlas::new(512, 2048, eeltex::export::atlas::Source::Coverage);
}

#[cfg(feature = "syntax")]
#[test]
fn highlighting_is_under_syntax() {
    let spans = eeltex::syntax::highlight(r"\frac{a}{b}");
    assert_eq!(
        spans.first().map(|(_, kind)| *kind),
        Some(eeltex::syntax::TokenKind::Command)
    );
}
