//! Render a formula to an SVG file.
//!
//! ```text
//! cargo run --example render_svg --features svg,embed-math-font -- \
//!     'x^2 + \frac{a}{b}' out.svg
//! ```

use std::error::Error;

use eeltex::export::svg::{SvgOptions, to_svg};
use eeltex::{Fonts, MathStyle, Options};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(source), Some(path)) = (args.next(), args.next()) else {
        return Err(
            "usage: render_svg '<latex maths>' <out.svg> [font size] [display|text]".into(),
        );
    };
    let font_size = args.next().and_then(|s| s.parse().ok()).unwrap_or(48.0);
    let style = match args.next().as_deref() {
        Some("text") => MathStyle::Text,
        _ => MathStyle::Display,
    };

    let fonts = Fonts::embedded()?;
    let options = Options::default()
        .with_font_size(font_size)
        .with_style(style);
    let scene = eeltex::render(&source, &fonts, &options)?;
    if !scene.missing.is_empty() {
        eprintln!("note: no glyph for {:?}", scene.missing);
    }

    std::fs::write(&path, to_svg(&scene, &fonts, &SvgOptions::default())?)?;
    println!(
        "{path}: {:.1} x {:.1} px, {} glyphs, {} rules",
        scene.width,
        scene.height(),
        scene.glyphs.len(),
        scene.rules.len()
    );
    Ok(())
}
