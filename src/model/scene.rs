//! The output of layout: where every glyph and rule goes.

use ttf_parser::GlyphId;

#[cfg(feature = "attrs")]
use crate::attrs::{ItemAttrs, Node, Paint};
use crate::attrs::{NodeId, PaintId};

/// Which of the supplied faces a glyph belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontKind {
    Math,
    /// The face supplied for `\text{…}`, if there was one.
    Text,
}

macro_rules! attr_accessors {
    () => {
        /// The node this item belongs to, or [`NodeId::NONE`] without `attrs`.
        #[must_use]
        pub fn node(&self) -> NodeId {
            #[cfg(feature = "attrs")]
            {
                self.attrs.node
            }
            #[cfg(not(feature = "attrs"))]
            {
                NodeId::NONE
            }
        }

        /// The paint this item takes, or [`PaintId::INHERIT`] without `attrs`.
        #[must_use]
        pub fn paint(&self) -> PaintId {
            #[cfg(feature = "attrs")]
            {
                self.attrs.paint
            }
            #[cfg(not(feature = "attrs"))]
            {
                PaintId::INHERIT
            }
        }

        /// Opacity, or 1 without `attrs`.
        #[must_use]
        pub fn alpha(&self) -> f32 {
            #[cfg(feature = "attrs")]
            {
                self.attrs.alpha
            }
            #[cfg(not(feature = "attrs"))]
            {
                1.0
            }
        }
    };
}

/// One glyph, positioned at its pen origin.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct GlyphInstance {
    pub glyph: GlyphId,
    pub font: FontKind,
    /// Pen origin, y-down; outline point `(ux, uy)` (y-up) lands at `(x + ux * s, y - uy * s)`.
    pub x: f32,
    pub y: f32,
    /// Pixels per em for *this* glyph.
    pub size: f32,
    #[cfg(feature = "attrs")]
    pub attrs: ItemAttrs,
}

impl GlyphInstance {
    pub(crate) fn new(glyph: GlyphId, font: FontKind, x: f32, y: f32, size: f32) -> GlyphInstance {
        GlyphInstance {
            glyph,
            font,
            x,
            y,
            size,
            #[cfg(feature = "attrs")]
            attrs: ItemAttrs::default(),
        }
    }

    attr_accessors!();
}

/// A filled axis-aligned rectangle: fraction bars, radical rules, over/underlines.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Rule {
    pub x: f32,
    /// The **top** edge, y-down.
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[cfg(feature = "attrs")]
    pub attrs: ItemAttrs,
    /// What layout means this box to be; always [`Shape::Rect`] once in a [`Scene`].
    pub(crate) shape: Shape,
}

/// How layout uses a [`Rule`] while it is building: [`Scene`] sorts them apart at the end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Shape {
    Rect,
    /// A fill behind everything else: `\colorbox`.
    #[cfg_attr(not(all(feature = "attrs", feature = "formatting")), allow(dead_code))]
    Background,
    /// A line along the box's diagonal, bottom left to top right, this thick.
    #[cfg_attr(not(feature = "formatting"), allow(dead_code))]
    Rising(f32),
    /// A line along the box's other diagonal, top left to bottom right.
    #[cfg_attr(not(feature = "formatting"), allow(dead_code))]
    Falling(f32),
}

impl Rule {
    pub(crate) fn new(x: f32, y: f32, width: f32, height: f32) -> Rule {
        Rule {
            x,
            y,
            width,
            height,
            #[cfg(feature = "attrs")]
            attrs: ItemAttrs::default(),
            shape: Shape::Rect,
        }
    }

    pub(crate) fn with_shape(mut self, shape: Shape) -> Rule {
        self.shape = shape;
        self
    }

    attr_accessors!();
}

/// A straight line with square ends, `thickness` across: the strokes of `\cancel`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Stroke {
    /// One end, y-down.
    pub x0: f32,
    pub y0: f32,
    /// The other end.
    pub x1: f32,
    pub y1: f32,
    pub thickness: f32,
    #[cfg(feature = "attrs")]
    pub attrs: ItemAttrs,
}

impl Stroke {
    /// The four corners of the filled quadrilateral, in winding order.
    #[must_use]
    pub fn corners(&self) -> [[f32; 2]; 4] {
        let (dx, dy) = (self.x1 - self.x0, self.y1 - self.y0);
        let length = (dx * dx + dy * dy).sqrt();
        if length <= 0.0 {
            return [[self.x0, self.y0]; 4];
        }
        let half = self.thickness / 2.0;
        let (nx, ny) = (-dy / length * half, dx / length * half);
        [
            [self.x0 + nx, self.y0 + ny],
            [self.x1 + nx, self.y1 + ny],
            [self.x1 - nx, self.y1 - ny],
            [self.x0 - nx, self.y0 - ny],
        ]
    }

    attr_accessors!();
}

/// A rule's diagonal as a stroke.
pub(crate) fn stroke_of(rule: &Rule, thickness: f32, rising: bool) -> Stroke {
    let (bottom, top) = (rule.y + rule.height, rule.y);
    let (y0, y1) = if rising { (bottom, top) } else { (top, bottom) };
    Stroke {
        x0: rule.x,
        y0,
        x1: rule.x + rule.width,
        y1,
        thickness,
        #[cfg(feature = "attrs")]
        attrs: rule.attrs,
    }
}

/// A laid-out formula.
///
/// Pixels, y down, origin at the left edge on the baseline; the bounding box is
/// `(0, -ascent)` to `(width, descent)`. Layout is linear in the font size.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Scene {
    pub glyphs: Vec<GlyphInstance>,
    pub rules: Vec<Rule>,
    /// Fills drawn before anything else, in their own paint: `\colorbox`. Empty without `attrs`.
    pub backgrounds: Vec<Rule>,
    /// Diagonal lines: `\cancel` and its kin.
    pub strokes: Vec<Stroke>,
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
    pub font_size: f32,
    /// Characters with no glyph in the supplied fonts.
    pub missing: Vec<char>,
    /// The tree layout walked, parents before children; index zero is the root.
    #[cfg(feature = "attrs")]
    pub nodes: Vec<Node>,
    /// The colours this scene uses; index zero is always the inherit entry.
    #[cfg(feature = "attrs")]
    pub paints: Vec<Paint>,
}

impl Scene {
    #[must_use]
    pub fn height(&self) -> f32 {
        self.ascent + self.descent
    }

    /// The scene at `k` times the size, equivalent to laying out at `font_size * k`.
    #[must_use]
    pub fn scaled(&self, k: f32) -> Scene {
        Scene {
            glyphs: self
                .glyphs
                .iter()
                .map(|g| GlyphInstance {
                    x: g.x * k,
                    y: g.y * k,
                    size: g.size * k,
                    ..*g
                })
                .collect(),
            rules: self.rules.iter().map(|r| scale_rule(r, k)).collect(),
            backgrounds: self.backgrounds.iter().map(|r| scale_rule(r, k)).collect(),
            strokes: self
                .strokes
                .iter()
                .map(|s| Stroke {
                    x0: s.x0 * k,
                    y0: s.y0 * k,
                    x1: s.x1 * k,
                    y1: s.y1 * k,
                    thickness: s.thickness * k,
                    ..*s
                })
                .collect(),
            width: self.width * k,
            ascent: self.ascent * k,
            descent: self.descent * k,
            font_size: self.font_size * k,
            missing: self.missing.clone(),
            // Node boxes scale too, so hit-testing survives a zoom.
            #[cfg(feature = "attrs")]
            nodes: self
                .nodes
                .iter()
                .map(|n| Node {
                    x: n.x * k,
                    y: n.y * k,
                    width: n.width * k,
                    height: n.height * k,
                    baseline: n.baseline * k,
                    ..*n
                })
                .collect(),
            #[cfg(feature = "attrs")]
            paints: self.paints.clone(),
        }
    }

    /// The scene shifted; `width`, `ascent` and `descent` are unchanged.
    #[must_use]
    pub fn translated(&self, dx: f32, dy: f32) -> Scene {
        Scene {
            glyphs: self
                .glyphs
                .iter()
                .map(|g| GlyphInstance {
                    x: g.x + dx,
                    y: g.y + dy,
                    ..*g
                })
                .collect(),
            rules: self.rules.iter().map(|r| move_rule(r, dx, dy)).collect(),
            backgrounds: self
                .backgrounds
                .iter()
                .map(|r| move_rule(r, dx, dy))
                .collect(),
            strokes: self
                .strokes
                .iter()
                .map(|s| Stroke {
                    x0: s.x0 + dx,
                    y0: s.y0 + dy,
                    x1: s.x1 + dx,
                    y1: s.y1 + dy,
                    ..*s
                })
                .collect(),
            #[cfg(feature = "attrs")]
            nodes: self
                .nodes
                .iter()
                .map(|n| Node {
                    x: n.x + dx,
                    y: n.y + dy,
                    baseline: n.baseline + dy,
                    ..*n
                })
                .collect(),
            ..self.clone()
        }
    }

    pub(crate) fn is_finite(&self) -> bool {
        self.width.is_finite()
            && self.ascent.is_finite()
            && self.descent.is_finite()
            && self
                .glyphs
                .iter()
                .all(|g| g.x.is_finite() && g.y.is_finite() && g.size.is_finite())
            && self.rules.iter().chain(&self.backgrounds).all(|r| {
                r.x.is_finite() && r.y.is_finite() && r.width.is_finite() && r.height.is_finite()
            })
            && self.strokes.iter().all(|s| {
                [s.x0, s.y0, s.x1, s.y1, s.thickness]
                    .iter()
                    .all(|v| v.is_finite())
            })
    }
}

fn scale_rule(r: &Rule, k: f32) -> Rule {
    Rule {
        x: r.x * k,
        y: r.y * k,
        width: r.width * k,
        height: r.height * k,
        ..*r
    }
}

fn move_rule(r: &Rule, dx: f32, dy: f32) -> Rule {
    Rule {
        x: r.x + dx,
        y: r.y + dy,
        ..*r
    }
}

/// Node identity queries; see [`crate::attrs`].
#[cfg(feature = "attrs")]
impl Scene {
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.index())
    }

    /// The node and everything above it, ending at the root.
    pub fn ancestors(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let mut next = self.node(id).map(|_| id);
        core::iter::from_fn(move || {
            let current = next?;
            next = self.node(current).and_then(|n| n.parent);
            Some(current)
        })
    }

    /// Whether `id` is `ancestor` or lies beneath it.
    #[must_use]
    pub fn is_within(&self, id: NodeId, ancestor: NodeId) -> bool {
        self.ancestors(id).any(|n| n == ancestor)
    }

    /// The innermost (smallest) ink-producing node whose box contains the point.
    #[must_use]
    pub fn hit(&self, x: f32, y: f32) -> Option<NodeId> {
        let mut best: Option<(f32, NodeId)> = None;
        for (index, node) in self.nodes.iter().enumerate() {
            if !node.has_ink() || !node.contains(x, y) {
                continue;
            }
            let area = node.area();
            // Ties go to the later node, which is deeper: the arena is built parents first.
            if best.is_none_or(|(smallest, _)| area <= smallest) {
                best = Some((area, NodeId(index as u32)));
            }
        }
        best.map(|(_, id)| id)
    }

    /// The innermost node covering a byte offset in the source; needs a parse with spans on.
    #[must_use]
    pub fn node_at_offset(&self, offset: u32) -> Option<NodeId> {
        let mut best: Option<(u32, NodeId)> = None;
        for (index, node) in self.nodes.iter().enumerate() {
            if !node.span.is_known() || !node.span.contains(offset) {
                continue;
            }
            let len = node.span.len();
            if best.is_none_or(|(shortest, _)| len <= shortest) {
                best = Some((len, NodeId(index as u32)));
            }
        }
        best.map(|(_, id)| id)
    }

    /// Every glyph produced by `id` or anything beneath it.
    pub fn glyphs_of(&self, id: NodeId) -> impl Iterator<Item = &GlyphInstance> + '_ {
        self.glyphs
            .iter()
            .filter(move |g| self.is_within(g.attrs.node, id))
    }

    /// Every rule produced by `id` or anything beneath it.
    pub fn rules_of(&self, id: NodeId) -> impl Iterator<Item = &Rule> + '_ {
        self.rules
            .iter()
            .filter(move |r| self.is_within(r.attrs.node, id))
    }

    /// Every stroke produced by `id` or anything beneath it.
    pub fn strokes_of(&self, id: NodeId) -> impl Iterator<Item = &Stroke> + '_ {
        self.strokes
            .iter()
            .filter(move |s| self.is_within(s.attrs.node, id))
    }

    /// Colour a node and everything beneath it, in place, overriding any `\color` in the source.
    /// Backgrounds keep their own fill.
    pub fn set_paint(&mut self, node: NodeId, paint: Paint) {
        let id = self.intern(paint);
        let within: Vec<bool> = (0..self.nodes.len())
            .map(|i| self.is_within(NodeId(i as u32), node))
            .collect();
        let covered = |n: NodeId| within.get(n.index()).copied().unwrap_or(false);
        for glyph in &mut self.glyphs {
            if covered(glyph.attrs.node) {
                glyph.attrs.paint = id;
            }
        }
        for rule in &mut self.rules {
            if covered(rule.attrs.node) {
                rule.attrs.paint = id;
            }
        }
        for stroke in &mut self.strokes {
            if covered(stroke.attrs.node) {
                stroke.attrs.paint = id;
            }
        }
    }

    /// Put every item back to [`PaintId::INHERIT`], including colours the source asked for.
    /// Backgrounds keep their fill: inheriting the ink would hide what is on them.
    pub fn clear_paints(&mut self) {
        for glyph in &mut self.glyphs {
            glyph.attrs.paint = PaintId::INHERIT;
        }
        for rule in &mut self.rules {
            rule.attrs.paint = PaintId::INHERIT;
        }
        for stroke in &mut self.strokes {
            stroke.attrs.paint = PaintId::INHERIT;
        }
    }

    /// Look a colour up in this scene's table, adding it if it is new.
    #[must_use]
    pub fn intern(&mut self, paint: Paint) -> PaintId {
        if self.paints.is_empty() {
            // Index zero must stay the inherit entry, even in a hand-built scene.
            self.paints.push(Paint::rgba(0, 0, 0, 0));
        }
        crate::model::attrs::intern(&mut self.paints, paint)
    }

    /// The colour an item takes, or `None` if it inherits the consumer's.
    #[must_use]
    pub fn resolve(&self, paint: PaintId) -> Option<Paint> {
        if paint.is_inherit() {
            return None;
        }
        self.paints.get(paint.index()).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> Scene {
        Scene {
            glyphs: vec![GlyphInstance::new(
                GlyphId(3),
                FontKind::Math,
                2.0,
                -1.0,
                16.0,
            )],
            rules: vec![Rule::new(0.0, -4.0, 10.0, 0.5)],
            width: 10.0,
            ascent: 8.0,
            descent: 2.0,
            font_size: 16.0,
            ..Scene::default()
        }
    }

    #[test]
    fn scaling_touches_every_dimension() {
        let s = scene().scaled(2.0);
        assert_eq!(s.width, 20.0);
        assert_eq!(s.ascent, 16.0);
        assert_eq!(s.font_size, 32.0);
        assert_eq!(
            s.glyphs.first().map(|g| (g.x, g.y, g.size)),
            Some((4.0, -2.0, 32.0))
        );
        assert_eq!(s.rules.first().map(|r| (r.y, r.height)), Some((-8.0, 1.0)));
    }

    #[test]
    fn translating_leaves_the_extent_alone() {
        let s = scene().translated(5.0, 3.0);
        assert_eq!(s.width, 10.0);
        assert_eq!(s.ascent, 8.0);
        assert_eq!(s.glyphs.first().map(|g| (g.x, g.y)), Some((7.0, 2.0)));
    }

    #[test]
    fn nan_geometry_is_detectable() {
        assert!(scene().is_finite());
        let mut bad = scene();
        bad.width = f32::NAN;
        assert!(!bad.is_finite());
        let mut bad = scene();
        if let Some(g) = bad.glyphs.first_mut() {
            g.y = f32::INFINITY;
        }
        assert!(!bad.is_finite());
    }
}
