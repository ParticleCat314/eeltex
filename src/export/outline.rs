//! Curve flattening and outline point sampling, shared by the export backends.

use ttf_parser::{Face, GlyphId, OutlineBuilder};

#[cfg(feature = "attrs")]
use crate::attrs::NodeId;
#[cfg(feature = "attrs")]
use crate::font::Fonts;
use crate::font::{MathFont, TextFont};
#[cfg(feature = "attrs")]
use crate::scene::{FontKind, Scene};

/// Curves are split until they are within this many pixels of straight.
const FLATNESS: f32 = 0.25;

fn segments(deviation: f32) -> u32 {
    if !deviation.is_finite() || deviation <= FLATNESS {
        return 1;
    }
    // Error falls as the square of the segment count.
    ((deviation / FLATNESS).sqrt().ceil() as u32).clamp(1, 64)
}

/// Emit the points of a flattened quadratic, excluding `a` and including `p`.
pub(crate) fn flatten_quad(
    a: (f32, f32),
    c: (f32, f32),
    p: (f32, f32),
    emit: &mut impl FnMut((f32, f32)),
) {
    // Upper bound on the quadratic's deviation from its chord.
    let deviation = (c.0 - 0.5 * (a.0 + p.0)).hypot(c.1 - 0.5 * (a.1 + p.1)) * 0.5;
    let steps = segments(deviation);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let u = 1.0 - t;
        emit((
            u * u * a.0 + 2.0 * u * t * c.0 + t * t * p.0,
            u * u * a.1 + 2.0 * u * t * c.1 + t * t * p.1,
        ));
    }
}

/// Emit the points of a flattened cubic, excluding `a` and including `p`.
pub(crate) fn flatten_cubic(
    a: (f32, f32),
    c1: (f32, f32),
    c2: (f32, f32),
    p: (f32, f32),
    emit: &mut impl FnMut((f32, f32)),
) {
    let deviation = (c1.0 - a.0)
        .hypot(c1.1 - a.1)
        .max((c2.0 - c1.0).hypot(c2.1 - c1.1))
        .max((p.0 - c2.0).hypot(p.1 - c2.1));
    let steps = segments(deviation);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let u = 1.0 - t;
        emit((
            u * u * u * a.0 + 3.0 * u * u * t * c1.0 + 3.0 * u * t * t * c2.0 + t * t * t * p.0,
            u * u * u * a.1 + 3.0 * u * u * t * c1.1 + 3.0 * u * t * t * c2.1 + t * t * t * p.1,
        ));
    }
}

/// A glyph could otherwise ask for an unbounded number of points.
const MAX_POINTS: usize = 20_000;

/// Points along a glyph's outline, at most `spacing` pixels apart.
///
/// Pixels, y down, relative to the pen origin. Empty for a glyph with no outline.
#[must_use]
pub fn glyph_points(font: &MathFont<'_>, glyph: GlyphId, size: f32, spacing: f32) -> Vec<[f32; 2]> {
    walk(font.face(), font.units_per_em(), glyph, size, spacing)
}

/// Points along a text-face glyph's outline. See [`glyph_points`].
#[must_use]
pub fn text_glyph_points(
    font: &TextFont<'_>,
    glyph: GlyphId,
    size: f32,
    spacing: f32,
) -> Vec<[f32; 2]> {
    walk(font.face(), font.units_per_em(), glyph, size, spacing)
}

/// Points along every outline in a scene, in the scene's own space.
///
/// `node` restricts it to one sub-expression. Rules contribute their four edges.
#[cfg(feature = "attrs")]
#[must_use]
pub fn scene_points(
    scene: &Scene,
    fonts: &Fonts<'_>,
    node: Option<NodeId>,
    spacing: f32,
) -> Vec<[f32; 2]> {
    let spacing = spacing.max(0.25);
    let mut out = Vec::new();
    let wanted = |item: NodeId| match node {
        None => true,
        Some(root) => scene.is_within(item, root),
    };

    for glyph in &scene.glyphs {
        if !wanted(glyph.node()) || out.len() >= MAX_POINTS {
            continue;
        }
        let local = match (glyph.font, fonts.text.as_ref()) {
            (FontKind::Text, Some(face)) => {
                text_glyph_points(face, glyph.glyph, glyph.size, spacing)
            }
            _ => glyph_points(&fonts.math, glyph.glyph, glyph.size, spacing),
        };
        out.extend(local.into_iter().map(|p| [glyph.x + p[0], glyph.y + p[1]]));
    }

    for stroke in &scene.strokes {
        if !wanted(stroke.node()) || out.len() >= MAX_POINTS {
            continue;
        }
        let corners = stroke.corners();
        for (&[ax, ay], &[bx, by]) in corners.iter().zip(corners.iter().cycle().skip(1)) {
            let length = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
            let steps = ((length / spacing).ceil() as usize).clamp(1, 4096);
            for step in 0..steps {
                let t = step as f32 / steps as f32;
                out.push([ax + (bx - ax) * t, ay + (by - ay) * t]);
            }
        }
    }

    for rule in &scene.rules {
        if !wanted(rule.node()) || out.len() >= MAX_POINTS {
            continue;
        }
        let (w, h) = (rule.width, rule.height);
        let perimeter = 2.0 * (w + h);
        let steps = ((perimeter / spacing).ceil() as usize).clamp(4, 4096);
        for i in 0..steps {
            let t = i as f32 / steps as f32 * perimeter;
            let p = if t < w {
                [rule.x + t, rule.y]
            } else if t < w + h {
                [rule.x + w, rule.y + (t - w)]
            } else if t < 2.0 * w + h {
                [rule.x + w - (t - w - h), rule.y + h]
            } else {
                [rule.x, rule.y + h - (t - 2.0 * w - h)]
            };
            out.push(p);
        }
    }
    out.truncate(MAX_POINTS);
    out
}

fn walk(face: &Face<'_>, upem: f32, glyph: GlyphId, size: f32, spacing: f32) -> Vec<[f32; 2]> {
    if upem <= 0.0 || !upem.is_finite() || size <= 0.0 || !size.is_finite() {
        return Vec::new();
    }
    let mut walker = Walker {
        scale: size / upem,
        spacing: spacing.max(0.25),
        points: Vec::new(),
        start: (0.0, 0.0),
        current: (0.0, 0.0),
        carry: 0.0,
    };
    if face.outline_glyph(glyph, &mut walker).is_none() {
        return Vec::new();
    }
    walker.close_contour();
    walker.points
}

/// Drops a point every `spacing` pixels, carrying leftover distance across edges.
struct Walker {
    scale: f32,
    spacing: f32,
    points: Vec<[f32; 2]>,
    start: (f32, f32),
    current: (f32, f32),
    carry: f32,
}

impl Walker {
    /// Font design units, y-up, to pixels from the pen origin, y-down.
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.scale, -y * self.scale)
    }

    fn edge(&mut self, to: (f32, f32)) {
        let (x0, y0) = self.current;
        let (x1, y1) = to;
        let length = (x1 - x0).hypot(y1 - y0);
        let mut travelled = self.spacing - self.carry;
        while travelled <= length && self.points.len() < MAX_POINTS {
            let t = if length > 0.0 {
                travelled / length
            } else {
                0.0
            };
            self.points.push([x0 + (x1 - x0) * t, y0 + (y1 - y0) * t]);
            travelled += self.spacing;
        }
        self.carry = (self.carry + length) % self.spacing;
        self.current = to;
    }

    fn close_contour(&mut self) {
        let (sx, sy) = self.start;
        let (cx, cy) = self.current;
        if (cx - sx).abs() > f32::EPSILON || (cy - sy).abs() > f32::EPSILON {
            self.edge(self.start);
        }
    }
}

impl OutlineBuilder for Walker {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close_contour();
        let p = self.point(x, y);
        self.start = p;
        self.current = p;
        // Every contour starts with a point, so one shorter than the spacing still gets one.
        if self.points.len() < MAX_POINTS {
            self.points.push([p.0, p.1]);
        }
        self.carry = 0.0;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.point(x, y);
        self.edge(p);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let a = self.current;
        let c = self.point(x1, y1);
        let p = self.point(x, y);
        flatten_quad(a, c, p, &mut |q| self.edge(q));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let a = self.current;
        let c1 = self.point(x1, y1);
        let c2 = self.point(x2, y2);
        let p = self.point(x, y);
        flatten_cubic(a, c1, c2, p, &mut |q| self.edge(q));
    }

    fn close(&mut self) {
        self.close_contour();
    }
}
