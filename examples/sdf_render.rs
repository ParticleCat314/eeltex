//! Render a formula through an SDF atlas on the CPU, magnified, as a fragment shader would.
//!
//! ```text
//! cargo run --example sdf_render --features atlas,sdf,embed-math-font -- \
//!     '\sum_{n=1}^{\infty} \frac{1}{n^2}' out.pgm 8
//! ```
//!
//! Writes `out.pgm` (the formula) and `out.atlas.pgm` (the atlas texture).

use std::error::Error;

use eeltex::export::atlas::{Atlas, Source};
use eeltex::{Fonts, Options};

/// Layout size, magnified afterwards.
const LAYOUT_SIZE: f32 = 18.0;
const SAMPLE_SIZE: f32 = 48.0;
const SPREAD: f32 = 4.0;
const PADDING: f32 = 12.0;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let Some(source) = args.next() else {
        return Err("usage: sdf_render '<latex>' [out.pgm] [magnification]".into());
    };
    let out = args.next().unwrap_or_else(|| "out.pgm".to_owned());
    let zoom = args
        .next()
        .and_then(|z| z.parse().ok())
        .unwrap_or(6.0_f32)
        .clamp(0.25, 64.0);

    let fonts = Fonts::embedded()?;
    let options = Options::default().with_font_size(LAYOUT_SIZE);
    let scene = eeltex::render(&source, &fonts, &options)?;
    if !scene.missing.is_empty() {
        eprintln!("no glyph for {:?}", scene.missing);
    }

    let field = Source::SignedDistance {
        sample_size: SAMPLE_SIZE,
        spread: SPREAD,
    };
    let mut atlas = Atlas::new(512, 4096, field);
    atlas.prepare(&scene, &fonts);
    eprintln!(
        "{} glyph(s) in a {}x{} atlas; drawing at {zoom}x",
        atlas.len(),
        atlas.width(),
        atlas.height(),
    );

    // Magnify the scene rather than laying it out again.
    let scene = scene.scaled(zoom);

    let width = (scene.width + 2.0 * PADDING).ceil().max(1.0) as usize;
    let height = (scene.height() + 2.0 * PADDING).ceil().max(1.0) as usize;
    let origin = (PADDING, PADDING + scene.ascent);
    let mut image = vec![0u8; width * height];

    for glyph in &scene.glyphs {
        let Some(quad) = atlas.quad(glyph) else {
            continue;
        };
        let quad_w = quad.max[0] - quad.min[0];
        let quad_h = quad.max[1] - quad.min[1];
        let uv_w = quad.uv_max[0] - quad.uv_min[0];
        let uv_h = quad.uv_max[1] - quad.uv_min[1];
        // Texel coverage of one output pixel sets the antialiasing band width (a shader's `fwidth`).
        let texels_per_pixel = uv_w * atlas.width() as f32 / quad_w;
        let edge = (0.5 * texels_per_pixel / SPREAD).clamp(0.001, 0.5);

        let x0 = (origin.0 + quad.min[0]).floor().max(0.0) as usize;
        let y0 = (origin.1 + quad.min[1]).floor().max(0.0) as usize;
        let x1 = (origin.0 + quad.max[0]).ceil().max(0.0) as usize;
        let y1 = (origin.1 + quad.max[1]).ceil().max(0.0) as usize;

        for y in y0..y1.min(height) {
            for x in x0..x1.min(width) {
                let u = ((x as f32 + 0.5 - origin.0) - quad.min[0]) / quad_w;
                let v = ((y as f32 + 0.5 - origin.1) - quad.min[1]) / quad_h;
                if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
                    continue;
                }
                let d = sample(&atlas, quad.uv_min[0] + u * uv_w, quad.uv_min[1] + v * uv_h);
                let alpha = smoothstep(0.5 - edge, 0.5 + edge, d);
                if let Some(cell) = image.get_mut(y * width + x) {
                    *cell = (*cell).max((alpha * 255.0) as u8);
                }
            }
        }
    }

    for rule in &scene.rules {
        let x0 = (origin.0 + rule.x).round().max(0.0) as usize;
        let y0 = (origin.1 + rule.y).round().max(0.0) as usize;
        let x1 = (origin.0 + rule.x + rule.width).round().max(0.0) as usize;
        // A hairline still gets one row.
        let y1 = ((origin.1 + rule.y + rule.height).round().max(0.0) as usize).max(y0 + 1);
        for y in y0..y1.min(height) {
            for x in x0..x1.min(width) {
                if let Some(cell) = image.get_mut(y * width + x) {
                    *cell = 255;
                }
            }
        }
    }

    let atlas_path = format!("{}.atlas.pgm", out.trim_end_matches(".pgm"));
    write_pgm(&out, width, height, &image)?;
    write_pgm(
        &atlas_path,
        atlas.width() as usize,
        atlas.height() as usize,
        atlas.data(),
    )?;
    eprintln!("wrote {out} ({width}x{height}) and {atlas_path}");
    Ok(())
}

/// Bilinear atlas lookup, as a GPU sampler would do.
fn sample(atlas: &Atlas, u: f32, v: f32) -> f32 {
    let width = atlas.width() as f32;
    let height = atlas.height() as f32;
    let x = (u * width - 0.5).clamp(0.0, width - 1.0);
    let y = (v * height - 0.5).clamp(0.0, height - 1.0);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |cx: f32, cy: f32| {
        let cx = cx.min(width - 1.0) as usize;
        let cy = cy.min(height - 1.0) as usize;
        let index = cy * atlas.width() as usize + cx;
        f32::from(atlas.data().get(index).copied().unwrap_or(0)) / 255.0
    };
    let top = at(x0, y0) * (1.0 - fx) + at(x0 + 1.0, y0) * fx;
    let bottom = at(x0, y0 + 1.0) * (1.0 - fx) + at(x0 + 1.0, y0 + 1.0) * fx;
    top * (1.0 - fy) + bottom * fy
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn write_pgm(path: &str, width: usize, height: usize, data: &[u8]) -> std::io::Result<()> {
    let mut pgm = format!("P5\n{width} {height}\n255\n").into_bytes();
    pgm.extend_from_slice(data);
    std::fs::write(path, pgm)
}
