//! Turning a glyph outline into a signed distance field.

use ttf_parser::{GlyphId, OutlineBuilder};

use crate::export::outline::{flatten_cubic, flatten_quad};
use crate::font::{MathFont, TextFont};

/// A signed distance field and where it sits relative to the pen origin.
#[derive(Debug, Clone)]
pub struct SignedField {
    pub width: u32,
    pub height: u32,
    /// Offset from the pen origin to the field's left edge, in pixels at [`SignedField::size`].
    pub left: i32,
    /// Offset from the pen origin to the field's top edge, y **down**.
    pub top: i32,
    /// Pixels per em this was sampled at.
    pub size: f32,
    /// Distance in pixels at [`SignedField::size`] where the encoding saturates; also the padding.
    pub spread: f32,
    /// `width * height` samples, row-major.
    ///
    /// 128 is the outline, above is inside; `d = (value / 255 - 0.5) * 2 * spread` in pixels.
    pub data: Vec<u8>,
}

impl SignedField {
    /// The signed distance at a sample, in pixels, positive inside.
    #[must_use]
    pub fn distance(&self, x: u32, y: u32) -> f32 {
        let index = y as usize * self.width as usize + x as usize;
        let value = self.data.get(index).copied().unwrap_or(0);
        (f32::from(value) / 255.0 - 0.5) * 2.0 * self.spread
    }
}

/// A degenerate outline could otherwise ask for an unbounded field.
const MAX_EDGE: u32 = 4096;

/// Cap on flattened segments; sampling cost is quadratic.
const MAX_SEGMENTS: usize = 65_536;

/// Cap on the row-bucket entries [`sample`] allocates, against hostile fonts.
const MAX_BUCKET_ENTRIES: usize = 1 << 22;

/// Cap on [`sample`]'s work: `width` × bucket entries segment measurements.
const MAX_SAMPLE_WORK: u64 = 1 << 27;

/// The spread to use when a consumer has no reason to pick one.
pub const DEFAULT_SPREAD: f32 = 3.0;

/// Build a distance field for a glyph of the maths font.
///
/// `size_px` is the sampling resolution, not the display size; 48 or 64 suits text.
/// `None` if the glyph has no outline.
#[must_use]
pub fn sdf_glyph(
    font: &MathFont<'_>,
    glyph: GlyphId,
    size_px: f32,
    spread_px: f32,
) -> Option<SignedField> {
    build(font.face(), font.units_per_em(), glyph, size_px, spread_px)
}

/// Build a distance field for a glyph of the text font. See [`sdf_glyph`].
#[must_use]
pub fn sdf_text_glyph(
    font: &TextFont<'_>,
    glyph: GlyphId,
    size_px: f32,
    spread_px: f32,
) -> Option<SignedField> {
    build(font.face(), font.units_per_em(), glyph, size_px, spread_px)
}

fn build(
    face: &ttf_parser::Face<'_>,
    upem: f32,
    glyph: GlyphId,
    size_px: f32,
    spread_px: f32,
) -> Option<SignedField> {
    let usable = |v: f32| v > 0.0 && v.is_finite();
    if !(usable(size_px) && usable(upem) && usable(spread_px)) {
        return None;
    }
    let scale = size_px / upem;
    let bbox = face.glyph_bounding_box(glyph)?;

    // Pad by the spread so the shader never samples a clamped edge.
    let pad = spread_px.ceil() + 1.0;
    let left = (f32::from(bbox.x_min) * scale).floor() - pad;
    // y is flipped here, so the *top* of the field comes from `y_max`.
    let top = (-f32::from(bbox.y_max) * scale).floor() - pad;
    let right = (f32::from(bbox.x_max) * scale).ceil() + pad;
    let bottom = (-f32::from(bbox.y_min) * scale).ceil() + pad;

    let width = (right - left).max(1.0) as u32;
    let height = (bottom - top).max(1.0) as u32;
    if width > MAX_EDGE || height > MAX_EDGE {
        return None;
    }

    let mut collector = Collector::new(left, top, scale);
    face.outline_glyph(glyph, &mut collector)?;
    collector.close_contour();
    let segments = collector.segments;
    if segments.is_empty() || segments.len() > MAX_SEGMENTS {
        return None;
    }

    Some(SignedField {
        width,
        height,
        left: left as i32,
        top: top as i32,
        size: size_px,
        spread: spread_px,
        data: sample(&segments, width, height, spread_px)?,
    })
}

/// One flattened edge, in field-local pixels, y-down.
#[derive(Clone, Copy)]
struct Segment {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Segment {
    fn distance_squared(self, px: f32, py: f32) -> f32 {
        let dx = self.x1 - self.x0;
        let dy = self.y1 - self.y0;
        let len_squared = dx * dx + dy * dy;
        let t = if len_squared <= f32::EPSILON {
            0.0
        } else {
            (((px - self.x0) * dx + (py - self.y0) * dy) / len_squared).clamp(0.0, 1.0)
        };
        let ox = px - (self.x0 + t * dx);
        let oy = py - (self.y0 + t * dy);
        ox * ox + oy * oy
    }

    /// Winding contribution for a ray from `(px, py)` towards `+x`.
    ///
    /// Half-open on `y`, so a vertex shared by two edges is counted exactly once.
    fn winding(self, px: f32, py: f32) -> i32 {
        if (self.y0 <= py) == (self.y1 <= py) {
            return 0;
        }
        let t = (py - self.y0) / (self.y1 - self.y0);
        if self.x0 + t * (self.x1 - self.x0) <= px {
            return 0;
        }
        if self.y1 > self.y0 { 1 } else { -1 }
    }
}

/// `None` when the work would exceed [`MAX_BUCKET_ENTRIES`] or [`MAX_SAMPLE_WORK`].
fn sample(segments: &[Segment], width: u32, height: u32, spread: f32) -> Option<Vec<u8>> {
    // Segments are bucketed per scanline within `spread`, in one flat table: count, then fill.
    let rows = height as usize;
    let span = |segment: &Segment| {
        let top = (segment.y0.min(segment.y1) - spread).floor().max(0.0) as usize;
        let bottom = (segment.y0.max(segment.y1) + spread).ceil().max(0.0) as usize;
        (top.min(rows), bottom.min(rows))
    };
    let mut offsets = vec![0usize; rows + 1];
    let mut entries = 0usize;
    for segment in segments {
        let (top, bottom) = span(segment);
        entries = entries.saturating_add(bottom.saturating_sub(top));
        if entries > MAX_BUCKET_ENTRIES {
            return None;
        }
        for count in offsets.iter_mut().take(bottom + 1).skip(top + 1) {
            *count += 1;
        }
    }
    if (width as u64).saturating_mul(entries as u64) > MAX_SAMPLE_WORK {
        return None;
    }
    let mut total = 0;
    for offset in &mut offsets {
        total += *offset;
        *offset = total;
    }
    let mut buckets = vec![0u32; entries];
    let mut cursors = offsets.clone();
    for (index, segment) in segments.iter().enumerate() {
        let (top, bottom) = span(segment);
        for cursor in cursors.iter_mut().take(bottom).skip(top) {
            if let Some(slot) = buckets.get_mut(*cursor) {
                *slot = index as u32;
            }
            *cursor += 1;
        }
    }

    let mut data = vec![0u8; (width as usize) * (height as usize)];
    for y in 0..rows {
        let from = offsets.get(y).copied().unwrap_or(0);
        let to = offsets.get(y + 1).copied().unwrap_or(from);
        let Some(row) = buckets.get(from..to) else {
            continue;
        };
        let py = y as f32 + 0.5;
        for x in 0..width as usize {
            let px = x as f32 + 0.5;
            let mut nearest = f32::INFINITY;
            let mut winding = 0i32;
            for index in row {
                let Some(segment) = segments.get(*index as usize) else {
                    continue;
                };
                nearest = nearest.min(segment.distance_squared(px, py));
                winding += segment.winding(px, py);
            }
            // Non-zero winding, to match the rasteriser's fill rule.
            let sign = if winding != 0 { 1.0 } else { -1.0 };
            let signed = sign * nearest.sqrt();
            let encoded = (0.5 + 0.5 * (signed / spread).clamp(-1.0, 1.0)) * 255.0;
            if let Some(cell) = data.get_mut(y * width as usize + x) {
                *cell = encoded.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Some(data)
}

struct Collector {
    segments: Vec<Segment>,
    left: f32,
    top: f32,
    scale: f32,
    start: (f32, f32),
    current: (f32, f32),
}

impl Collector {
    fn new(left: f32, top: f32, scale: f32) -> Collector {
        Collector {
            segments: Vec::new(),
            left,
            top,
            scale,
            start: (0.0, 0.0),
            current: (0.0, 0.0),
        }
    }

    /// Font design units, y-up, to field pixels, y-down.
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.scale - self.left, -y * self.scale - self.top)
    }

    fn edge(&mut self, to: (f32, f32)) {
        if self.segments.len() < MAX_SEGMENTS {
            self.segments.push(Segment {
                x0: self.current.0,
                y0: self.current.1,
                x1: to.0,
                y1: to.1,
            });
        }
        self.current = to;
    }

    /// Close the contour if left open; a gap would invert the winding sign past it.
    fn close_contour(&mut self) {
        let (sx, sy) = self.start;
        let (cx, cy) = self.current;
        if (cx - sx).abs() > f32::EPSILON || (cy - sy).abs() > f32::EPSILON {
            self.edge(self.start);
        }
    }
}

impl OutlineBuilder for Collector {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close_contour();
        let p = self.point(x, y);
        self.start = p;
        self.current = p;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.point(x, y);
        self.edge(p);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let c = self.point(x1, y1);
        let p = self.point(x, y);
        flatten_quad(self.current, c, p, &mut |q| self.edge(q));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let c1 = self.point(x1, y1);
        let c2 = self.point(x2, y2);
        let p = self.point(x, y);
        flatten_cubic(self.current, c1, c2, p, &mut |q| self.edge(q));
    }

    fn close(&mut self) {
        self.close_contour();
    }
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
    fn the_padding_saturates_outside() {
        let fonts = fonts();
        let glyph = fonts.math.glyph_for('X').unwrap();
        let field = sdf_glyph(&fonts.math, glyph, 48.0, DEFAULT_SPREAD).expect("X has an outline");
        for (x, y) in [
            (0, 0),
            (field.width - 1, 0),
            (0, field.height - 1),
            (field.width - 1, field.height - 1),
        ] {
            let d = field.distance(x, y);
            assert!(d <= -DEFAULT_SPREAD, "corner ({x}, {y}) is only {d} px out");
        }
    }

    #[test]
    fn inside_is_positive_and_a_counter_is_not() {
        let fonts = fonts();
        let glyph = fonts.math.glyph_for('o').unwrap();
        let field = sdf_glyph(&fonts.math, glyph, 64.0, 2.0).expect("o has an outline");

        let centre = field.distance(field.width / 2, field.height / 2);
        assert!(centre < 0.0, "the middle of an `o` reads as ink: {centre}");

        // The maximum on the scanline is a stem, whatever the width of the `o`.
        let y = field.height / 2;
        let stem = (0..field.width)
            .map(|x| field.distance(x, y))
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(stem > 0.0, "no scanline of an `o` crosses ink: {stem}");
    }

    #[test]
    fn the_edge_is_where_the_sample_size_says_it_is() {
        let fonts = fonts();
        let glyph = fonts.math.glyph_for('I').unwrap();
        let coarse = sdf_glyph(&fonts.math, glyph, 32.0, 2.0).unwrap();
        let fine = sdf_glyph(&fonts.math, glyph, 128.0, 8.0).unwrap();

        let crossing = |f: &SignedField| -> f32 {
            let y = f.height / 2;
            for x in 0..f.width {
                if f.distance(x, y) >= 0.0 {
                    return (f.left as f32 + x as f32) / f.size;
                }
            }
            f32::NAN
        };
        let a = crossing(&coarse);
        let b = crossing(&fine);
        assert!(
            (a - b).abs() < 0.02,
            "the left edge of an I moved from {a} em to {b} em with the sample size"
        );
    }

    #[test]
    fn degenerate_requests_are_refused() {
        let fonts = fonts();
        let space = fonts.math.glyph_for(' ').unwrap();
        assert!(sdf_glyph(&fonts.math, space, 48.0, DEFAULT_SPREAD).is_none());

        let glyph = fonts.math.glyph_for('X').unwrap();
        assert!(sdf_glyph(&fonts.math, glyph, 0.0, DEFAULT_SPREAD).is_none());
        assert!(sdf_glyph(&fonts.math, glyph, 48.0, 0.0).is_none());
        assert!(sdf_glyph(&fonts.math, glyph, 48.0, -1.0).is_none());
        assert!(sdf_glyph(&fonts.math, glyph, 1.0e9, DEFAULT_SPREAD).is_none());
    }
}
