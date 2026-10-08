//! Rasterise a formula into a grayscale PGM.
//!
//! ```text
//! cargo run --example raster_check --features raster,embed-math-font -- \
//!     '\sum_{n=1}^{\infty} \frac{1}{n^2}' out.pgm
//! ```

use std::error::Error;

use eeltex::export::raster::{Ink, rasterise_scene};
use eeltex::{Fonts, Options, Paint};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(source), Some(path)) = (args.next(), args.next()) else {
        return Err("usage: raster_check '<latex maths>' <out.pgm> [font size]".into());
    };
    let font_size = args.next().and_then(|s| s.parse().ok()).unwrap_or(40.0);

    let fonts = Fonts::embedded()?;
    let options = Options::default().with_font_size(font_size);
    let scene = eeltex::render(&source, &fonts, &options)?;

    let ink = Ink::default()
        .with_fill(Paint::rgb(0, 0, 0))
        .with_padding(4.0);
    let bitmap = rasterise_scene(&scene, &fonts, &ink)?;

    // Inverted alpha channel: black ink on white.
    let mut pgm = format!("P5\n{} {}\n255\n", bitmap.width, bitmap.height).into_bytes();
    pgm.extend(
        bitmap
            .data
            .iter()
            .skip(3)
            .step_by(4)
            .map(|alpha| 255 - alpha),
    );
    std::fs::write(&path, pgm)?;

    println!("{path}: {} x {}", bitmap.width, bitmap.height);
    Ok(())
}
