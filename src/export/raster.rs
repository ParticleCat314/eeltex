//! Turning a glyph outline into an alpha bitmap.

use std::collections::HashMap;

use ttf_parser::{GlyphId, OutlineBuilder};

use crate::attrs::{Paint, PaintId};
use crate::error::Error;
use crate::export::outline::{flatten_cubic, flatten_quad};
use crate::font::{Fonts, MathFont, TextFont};
use crate::scene::{FontKind, Scene};

/// An 8-bit alpha bitmap and where it sits relative to the pen origin.
#[derive(Debug, Clone)]
pub struct Coverage {
    pub width: u32,
    pub height: u32,
    /// Offset from the pen origin to the bitmap's left edge, in pixels.
    pub left: i32,
    /// Offset from the pen origin to the bitmap's top edge, y down.
    pub top: i32,
    /// `width * height` coverage values, row-major, 0 to 255.
    pub data: Vec<u8>,
}

/// A degenerate outline could otherwise ask for an unbounded bitmap.
const MAX_EDGE: u32 = 4096;

/// Rasterise a glyph from the maths font at `size_px` pixels per em.
///
/// `None` if the glyph has no outline.
#[must_use]
pub fn rasterise_glyph(font: &MathFont<'_>, glyph: GlyphId, size_px: f32) -> Option<Coverage> {
    rasterise(font.face(), font.units_per_em(), glyph, size_px)
}

/// Rasterise a glyph from the text font. See [`rasterise_glyph`].
#[must_use]
pub fn rasterise_text_glyph(font: &TextFont<'_>, glyph: GlyphId, size_px: f32) -> Option<Coverage> {
    rasterise(font.face(), font.units_per_em(), glyph, size_px)
}

fn rasterise(
    face: &ttf_parser::Face<'_>,
    upem: f32,
    glyph: GlyphId,
    size_px: f32,
) -> Option<Coverage> {
    if size_px <= 0.0 || !size_px.is_finite() || upem <= 0.0 || !upem.is_finite() {
        return None;
    }
    let scale = size_px / upem;
    let bbox = face.glyph_bounding_box(glyph)?;

    let left = (f32::from(bbox.x_min) * scale).floor() - 1.0;
    // y is flipped here, so the *top* of the bitmap comes from `y_max`.
    let top = (-f32::from(bbox.y_max) * scale).floor() - 1.0;
    let right = (f32::from(bbox.x_max) * scale).ceil() + 1.0;
    let bottom = (-f32::from(bbox.y_min) * scale).ceil() + 1.0;

    let width = (right - left).max(1.0) as u32;
    let height = (bottom - top).max(1.0) as u32;
    if width > MAX_EDGE || height > MAX_EDGE {
        return None;
    }

    let mut sink = Sink::new(width, height, left, top, scale);
    face.outline_glyph(glyph, &mut sink)?;
    sink.close_contour();

    Some(Coverage {
        width,
        height,
        left: left as i32,
        top: top as i32,
        data: sink.finish(),
    })
}

struct Sink {
    /// `width + 1` columns: the last pixel's remainder spills one cell right.
    stride: usize,
    width: u32,
    height: u32,
    area: Vec<f32>,
    /// Origin of the bitmap in pen-space pixels, y-down.
    left: f32,
    top: f32,
    scale: f32,
    start: (f32, f32),
    current: (f32, f32),
}

impl Sink {
    fn new(width: u32, height: u32, left: f32, top: f32, scale: f32) -> Sink {
        let stride = width as usize + 1;
        Sink {
            stride,
            width,
            height,
            area: vec![0.0; stride * height as usize],
            left,
            top,
            scale,
            start: (0.0, 0.0),
            current: (0.0, 0.0),
        }
    }

    /// Font design units, y-up, to bitmap pixels, y-down.
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.scale - self.left, -y * self.scale - self.top)
    }

    fn close_contour(&mut self) {
        let (sx, sy) = self.start;
        let (cx, cy) = self.current;
        if (cx - sx).abs() > f32::EPSILON || (cy - sy).abs() > f32::EPSILON {
            self.line(self.current, self.start);
            self.current = self.start;
        }
    }

    fn add(&mut self, index: usize, value: f32) {
        if let Some(cell) = self.area.get_mut(index) {
            *cell += value;
        }
    }

    fn line(&mut self, from: (f32, f32), to: (f32, f32)) {
        if (from.1 - to.1).abs() <= f32::EPSILON {
            return;
        }
        let (dir, p0, p1) = if from.1 < to.1 {
            (1.0f32, from, to)
        } else {
            (-1.0f32, to, from)
        };
        let dxdy = (p1.0 - p0.0) / (p1.1 - p0.1);
        let mut x = p0.0;
        if p0.1 < 0.0 {
            x -= p0.1 * dxdy;
        }

        let first = p0.1.max(0.0).floor() as usize;
        let last = (p1.1.ceil().max(0.0) as usize).min(self.height as usize);
        for y in first..last {
            let row = y * self.stride;
            let line_top = (y as f32).max(p0.1);
            let line_bottom = ((y + 1) as f32).min(p1.1);
            let dy = line_bottom - line_top;
            if dy <= 0.0 {
                continue;
            }
            let next = x + dxdy * dy;
            let weight = dy * dir;

            let (x0, x1) = if x < next { (x, next) } else { (next, x) };
            let x0 = x0.max(0.0).min(self.width as f32);
            let x1 = x1.max(0.0).min(self.width as f32);
            let x0_floor = x0.floor();
            let x1_ceil = x1.ceil();
            let x0i = x0_floor as usize;
            let x1i = x1_ceil as usize;

            if x1i <= x0i + 1 {
                let mid = 0.5 * (x0 + x1) - x0_floor;
                self.add(row + x0i, weight * (1.0 - mid));
                self.add(row + x0i + 1, weight * mid);
            } else {
                let span = (x1 - x0).recip();
                let head = x0 - x0_floor;
                let a0 = 0.5 * span * (1.0 - head) * (1.0 - head);
                let tail = x1 - x1_ceil + 1.0;
                let am = 0.5 * span * tail * tail;
                self.add(row + x0i, weight * a0);
                if x1i == x0i + 2 {
                    self.add(row + x0i + 1, weight * (1.0 - a0 - am));
                } else {
                    let a1 = span * (1.5 - head);
                    self.add(row + x0i + 1, weight * (a1 - a0));
                    for xi in (x0i + 2)..(x1i - 1) {
                        self.add(row + xi, weight * span);
                    }
                    let a2 = a1 + (x1i - x0i - 3) as f32 * span;
                    self.add(row + x1i - 1, weight * (1.0 - a2 - am));
                }
                self.add(row + x1i, weight * am);
            }
            x = next;
        }
    }

    fn finish(self) -> Vec<u8> {
        let width = self.width as usize;
        let mut out = Vec::with_capacity(width * self.height as usize);
        for row in self.area.chunks_exact(self.stride) {
            let mut running = 0.0f32;
            out.extend(row.iter().take(width).map(|delta| {
                running += delta;
                // `abs` gives the non-zero fill rule.
                (running.abs().min(1.0) * 255.0) as u8
            }));
        }
        out
    }
}

impl OutlineBuilder for Sink {
    fn move_to(&mut self, x: f32, y: f32) {
        // An unclosed contour would bleed coverage across the scanline.
        self.close_contour();
        let p = self.point(x, y);
        self.start = p;
        self.current = p;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.point(x, y);
        self.line(self.current, p);
        self.current = p;
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let c = self.point(x1, y1);
        let p = self.point(x, y);
        flatten_quad(self.current, c, p, &mut |q| {
            self.line(self.current, q);
            self.current = q;
        });
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let c1 = self.point(x1, y1);
        let c2 = self.point(x2, y2);
        let p = self.point(x, y);
        flatten_cubic(self.current, c1, c2, p, &mut |q| {
            self.line(self.current, q);
            self.current = q;
        });
    }

    fn close(&mut self) {
        self.close_contour();
    }
}

/// A whole formula composited into one straight-alpha (not premultiplied) RGBA bitmap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes.
    pub data: Vec<u8>,
}

impl Bitmap {
    /// The pixel at `(x, y)`, or `None` outside the bitmap.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let at = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        self.data.get(at..at + 4)?.try_into().ok()
    }

    fn blend(&mut self, x: i32, y: i32, colour: [u8; 4], coverage: u8) {
        let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
            return;
        };
        if x >= self.width || y >= self.height {
            return;
        }
        let src_a = u32::from(colour[3]) * u32::from(coverage) / 255;
        if src_a == 0 {
            return;
        }
        let at = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        let Some([rgb @ .., alpha]) = self.data.get_mut(at..at + 4) else {
            return;
        };
        let dst_a = u32::from(*alpha);
        // u32 so rounding is identical on every target.
        let out_a = src_a + dst_a * (255 - src_a) / 255;
        for (under, &over) in rgb.iter_mut().zip(&colour) {
            let mixed = (u32::from(over) * src_a + u32::from(*under) * dst_a * (255 - src_a) / 255)
                .checked_div(out_a)
                .unwrap_or(0);
            *under = mixed.min(255) as u8;
        }
        *alpha = out_a.min(255) as u8;
    }
}

/// How a scene is coloured when it is composited.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Ink {
    /// Paint for items with no colour of their own ([`PaintId::INHERIT`](crate::PaintId)).
    pub fill: Paint,
    /// What the bitmap starts as. `None` leaves it transparent.
    pub background: Option<Paint>,
    /// Blank space around the formula, in pixels.
    pub padding: f32,
}

impl Default for Ink {
    fn default() -> Ink {
        Ink {
            fill: Paint::rgb(0, 0, 0),
            background: None,
            padding: 0.0,
        }
    }
}

impl Ink {
    #[must_use]
    pub fn with_fill(mut self, fill: Paint) -> Ink {
        self.fill = fill;
        self
    }

    #[must_use]
    pub fn with_background(mut self, background: Paint) -> Ink {
        self.background = Some(background);
        self
    }

    #[must_use]
    pub fn with_padding(mut self, padding: f32) -> Ink {
        self.padding = padding;
        self
    }
}

/// Composite a whole scene into one bitmap.
///
/// # Errors
///
/// [`Error::BadMetrics`] if the scene holds non-finite geometry, or if its
/// extent does not fit a bitmap.
pub fn rasterise_scene(scene: &Scene, fonts: &Fonts<'_>, ink: &Ink) -> Result<Bitmap, Error> {
    if !scene.is_finite() || !ink.padding.is_finite() || ink.padding < 0.0 {
        return Err(Error::BadMetrics);
    }
    let pad = ink.padding;
    let width = (scene.width + pad * 2.0).ceil();
    let height = (scene.ascent + scene.descent + pad * 2.0).ceil();
    let fits = 0.0..=f32::from(u16::MAX);
    if !fits.contains(&width) || !fits.contains(&height) {
        return Err(Error::BadMetrics);
    }
    let width = (width as u32).max(1);
    let height = (height as u32).max(1);

    let mut bitmap = Bitmap {
        width,
        height,
        data: vec![0; (width as usize) * (height as usize) * 4],
    };
    if let Some(background) = ink.background {
        for pixel in bitmap.data.chunks_exact_mut(4) {
            pixel.copy_from_slice(&background.rgba);
        }
    }

    let origin_x = pad;
    let origin_y = scene.ascent + pad;

    for rule in &scene.backgrounds {
        let colour = colour_of(scene, rule.paint(), rule.alpha(), ink);
        fill_rule(&mut bitmap, rule, origin_x, origin_y, colour);
    }

    let mut cache: HashMap<(FontKind, GlyphId, u32), Option<Coverage>> = HashMap::new();
    for instance in &scene.glyphs {
        let colour = colour_of(scene, instance.paint(), instance.alpha(), ink);
        // Keyed on the size in 64ths.
        let key = (
            instance.font,
            instance.glyph,
            (instance.size * 64.0).round().max(0.0) as u32,
        );
        let coverage = cache.entry(key).or_insert_with(|| match instance.font {
            FontKind::Math => rasterise_glyph(&fonts.math, instance.glyph, instance.size),
            FontKind::Text => {
                rasterise_text_glyph(fonts.text.as_ref()?, instance.glyph, instance.size)
            }
        });
        let Some(coverage) = coverage else { continue };
        let left = (origin_x + instance.x).round() as i32 + coverage.left;
        let top = (origin_y + instance.y).round() as i32 + coverage.top;
        for row in 0..coverage.height {
            for column in 0..coverage.width {
                let at = (row as usize) * (coverage.width as usize) + (column as usize);
                let Some(&value) = coverage.data.get(at) else {
                    continue;
                };
                bitmap.blend(left + column as i32, top + row as i32, colour, value);
            }
        }
    }

    for rule in &scene.rules {
        let colour = colour_of(scene, rule.paint(), rule.alpha(), ink);
        fill_rule(&mut bitmap, rule, origin_x, origin_y, colour);
    }
    for stroke in &scene.strokes {
        let colour = colour_of(scene, stroke.paint(), stroke.alpha(), ink);
        fill_stroke(&mut bitmap, stroke, origin_x, origin_y, colour);
    }
    Ok(bitmap)
}

fn fill_rule(bitmap: &mut Bitmap, rule: &crate::scene::Rule, ox: f32, oy: f32, colour: [u8; 4]) {
    let left = (ox + rule.x).round() as i32;
    let top = (oy + rule.y).round() as i32;
    let across = (rule.width.round() as i32).max(1);
    let down = (rule.height.round() as i32).max(1);
    for row in 0..down {
        for column in 0..across {
            bitmap.blend(left + column, top + row, colour, 255);
        }
    }
}

/// A stroke's quadrilateral, anti-aliased by sampling each pixel on a 4 × 4 grid.
fn fill_stroke(
    bitmap: &mut Bitmap,
    stroke: &crate::scene::Stroke,
    ox: f32,
    oy: f32,
    colour: [u8; 4],
) {
    const GRID: usize = 4;
    let corners = stroke.corners().map(|[x, y]| [x + ox, y + oy]);
    let inside = |px: f32, py: f32| {
        let mut sign = 0.0f32;
        for (&[ax, ay], &[bx, by]) in corners.iter().zip(corners.iter().cycle().skip(1)) {
            let cross = (bx - ax) * (py - ay) - (by - ay) * (px - ax);
            if cross != 0.0 {
                if sign != 0.0 && cross.signum() != sign {
                    return false;
                }
                sign = cross.signum();
            }
        }
        true
    };
    let xs = corners.map(|c| c[0]);
    let ys = corners.map(|c| c[1]);
    let left = xs.iter().copied().fold(f32::INFINITY, f32::min).floor() as i32;
    let right = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
    let top = ys.iter().copied().fold(f32::INFINITY, f32::min).floor() as i32;
    let bottom = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
    for y in top..bottom {
        for x in left..right {
            let mut hits = 0u32;
            for sy in 0..GRID {
                for sx in 0..GRID {
                    let px = x as f32 + (sx as f32 + 0.5) / GRID as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / GRID as f32;
                    if inside(px, py) {
                        hits += 1;
                    }
                }
            }
            if hits > 0 {
                let coverage = (hits * 255 / (GRID * GRID) as u32) as u8;
                bitmap.blend(x, y, colour, coverage);
            }
        }
    }
}

/// Without `attrs` every item inherits `ink.fill`.
#[allow(unused_variables)]
fn colour_of(scene: &Scene, paint: PaintId, alpha: f32, ink: &Ink) -> [u8; 4] {
    let base = ink.fill;
    #[cfg(feature = "attrs")]
    let base = scene.resolve(paint).unwrap_or(base);
    let mut rgba = base.rgba;
    rgba[3] = (f32::from(rgba[3]) * alpha.clamp(0.0, 1.0)).round() as u8;
    rgba
}

#[cfg(test)]
#[cfg(feature = "embed-math-font")]
mod tests {
    use super::*;
    use crate::Fonts;

    fn fonts() -> Fonts<'static> {
        Fonts::embedded().unwrap()
    }

    #[test]
    fn a_letter_rasterises_to_plausible_ink() {
        let fonts = fonts();
        let glyph = fonts.math.glyph_for('X').unwrap();
        let c = rasterise_glyph(&fonts.math, glyph, 64.0).expect("X has an outline");
        assert!(c.width > 8 && c.height > 8, "{} x {}", c.width, c.height);
        assert_eq!(c.data.len(), (c.width * c.height) as usize);

        let inked = c.data.iter().filter(|v| **v > 128).count() as f32;
        let total = c.data.len() as f32;
        let ratio = inked / total;
        assert!(
            (0.08..0.75).contains(&ratio),
            "an X covering {ratio} of its box is not an X"
        );
        assert!(c.data.iter().any(|v| (1..255).contains(v)), "no soft edges");
    }

    /// The counter of an `o` must stay empty; catches a wrong fill rule.
    #[test]
    fn a_counter_stays_hollow() {
        let fonts = fonts();
        let glyph = fonts.math.glyph_for('o').unwrap();
        let c = rasterise_glyph(&fonts.math, glyph, 64.0).expect("o has an outline");
        let centre = (c.height / 2) as usize * c.width as usize + (c.width / 2) as usize;
        let value = c.data.get(centre).copied().unwrap_or(255);
        assert!(value < 64, "the middle of an `o` is not hollow: {value}");
    }

    #[test]
    fn a_space_rasterises_to_nothing() {
        let fonts = fonts();
        let glyph = fonts.math.glyph_for(' ').unwrap();
        assert!(rasterise_glyph(&fonts.math, glyph, 64.0).is_none());
    }

    #[test]
    fn absurd_sizes_are_refused() {
        let fonts = fonts();
        let glyph = fonts.math.glyph_for('X').unwrap();
        assert!(rasterise_glyph(&fonts.math, glyph, 0.0).is_none());
        assert!(rasterise_glyph(&fonts.math, glyph, -8.0).is_none());
        assert!(rasterise_glyph(&fonts.math, glyph, 1.0e9).is_none());
    }
}

#[cfg(test)]
#[cfg(feature = "embed-math-font")]
mod scene_tests {
    use super::*;
    use crate::{Fonts, Options};

    fn scene(source: &str) -> (Scene, Fonts<'static>) {
        let fonts = Fonts::embedded().expect("the embedded font parses");
        let options = Options::default().with_font_size(40.0);
        let scene = crate::render(source, &fonts, &options).expect("lays out");
        (scene, fonts)
    }

    #[test]
    fn the_bitmap_is_the_size_of_the_formula() {
        let (scene, fonts) = scene(r"\frac{a}{b}");
        let plain = rasterise_scene(&scene, &fonts, &Ink::default()).expect("rasterises");
        assert_eq!(plain.width, scene.width.ceil() as u32);
        assert_eq!(plain.height, (scene.ascent + scene.descent).ceil() as u32);
        assert_eq!(plain.data.len(), (plain.width * plain.height * 4) as usize);

        let padded =
            rasterise_scene(&scene, &fonts, &Ink::default().with_padding(7.0)).expect("rasterises");
        assert_eq!(padded.width, plain.width + 14);
        assert_eq!(padded.height, plain.height + 14);
    }

    #[test]
    fn an_empty_formula_still_makes_a_bitmap() {
        let (scene, fonts) = scene("");
        let bitmap = rasterise_scene(&scene, &fonts, &Ink::default()).expect("rasterises");
        assert!(bitmap.width >= 1 && bitmap.height >= 1);
        assert!(bitmap.data.iter().all(|byte| *byte == 0), "not transparent");
    }

    #[test]
    fn the_ink_is_inside_the_formula_and_the_margin_is_clear() {
        let (scene, fonts) = scene("x");
        let ink = Ink::default().with_padding(6.0);
        let bitmap = rasterise_scene(&scene, &fonts, &ink).expect("rasterises");

        let opaque = |x: u32, y: u32| bitmap.pixel(x, y).is_some_and(|p| p[3] > 0);
        assert!(
            (0..bitmap.width).any(|x| (0..bitmap.height).any(|y| opaque(x, y))),
            "nothing was drawn at all"
        );
        for x in 0..bitmap.width {
            for y in 0..bitmap.height {
                let inside = x >= 6 && y >= 6 && x < bitmap.width - 6 && y < bitmap.height - 6;
                if !inside {
                    assert!(!opaque(x, y), "ink at ({x}, {y}) is in the margin");
                }
            }
        }
    }

    #[test]
    fn a_background_is_opaque_everywhere() {
        let (scene, fonts) = scene("x");
        let ink = Ink::default()
            .with_fill(Paint::rgb(255, 255, 255))
            .with_background(Paint::rgb(0, 0, 40));
        let bitmap = rasterise_scene(&scene, &fonts, &ink).expect("rasterises");
        assert!(
            (0..bitmap.width * bitmap.height).all(|i| {
                bitmap
                    .pixel(i % bitmap.width, i / bitmap.width)
                    .is_some_and(|p| p[3] == 255)
            }),
            "the background left a transparent pixel"
        );
        assert!(
            (0..bitmap.width)
                .any(|x| (0..bitmap.height).any(|y| bitmap.pixel(x, y).is_some_and(|p| p[2] > 40))),
            "the white glyph did not composite over the dark background"
        );
    }

    #[test]
    fn a_rule_is_drawn_and_is_never_thinner_than_a_pixel() {
        let fonts = Fonts::embedded().expect("the embedded font parses");
        // Small enough that the bar's thickness rounds below one pixel.
        let options = Options::default().with_font_size(8.0);
        let scene = crate::render(r"\frac{a}{b}", &fonts, &options).expect("lays out");
        assert_eq!(scene.rules.len(), 1);
        assert!(scene.rules.first().is_some_and(|r| r.height < 1.0));

        let bitmap = rasterise_scene(&scene, &fonts, &Ink::default()).expect("rasterises");
        let bar = scene.rules.first().expect("the bar");
        let row = (scene.ascent + bar.y).round().max(0.0) as u32;
        let solid = (0..bar.width.round() as u32)
            .filter(|x| {
                bitmap
                    .pixel(x + bar.x.round() as u32, row)
                    .is_some_and(|p| p[3] == 255)
            })
            .count();
        assert!(solid > 0, "the fraction bar was not drawn");
    }

    #[cfg(feature = "attrs")]
    #[test]
    fn a_coloured_sub_expression_is_drawn_in_its_own_colour() {
        let fonts = Fonts::embedded().expect("the embedded font parses");
        let options = Options::default().with_font_size(40.0);
        let scene = crate::render(r"\textcolor{#ff0000}{x}", &fonts, &options).expect("lays out");
        let ink = Ink::default().with_fill(Paint::rgb(0, 0, 255));
        let bitmap = rasterise_scene(&scene, &fonts, &ink).expect("rasterises");
        let reddest = (0..bitmap.width)
            .flat_map(|x| (0..bitmap.height).map(move |y| (x, y)))
            .filter_map(|(x, y)| bitmap.pixel(x, y))
            .filter(|p| p[3] > 200)
            .max_by_key(|p| p[0]);
        let reddest = reddest.expect("some opaque ink");
        assert!(reddest[0] > 200 && reddest[2] < 60, "not red: {reddest:?}");
    }

    #[test]
    fn absurd_input_is_refused() {
        let (scene, fonts) = scene("x");
        let bad = Ink::default().with_padding(f32::NAN);
        assert!(matches!(
            rasterise_scene(&scene, &fonts, &bad),
            Err(Error::BadMetrics)
        ));
        let huge = Ink::default().with_padding(100_000.0);
        assert!(matches!(
            rasterise_scene(&scene, &fonts, &huge),
            Err(Error::BadMetrics)
        ));
    }
}
