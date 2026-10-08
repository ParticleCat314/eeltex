//! Identity and per-item attributes: nodes, spans, paint.

use crate::class::AtomClass;

/// A byte range in the source that produced something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    /// The span of something that did not record one.
    pub const UNKNOWN: Span = Span { start: 0, end: 0 };

    #[must_use]
    pub fn new(start: u32, end: u32) -> Span {
        Span { start, end }
    }

    #[must_use]
    pub fn is_known(self) -> bool {
        self.end > self.start
    }

    #[must_use]
    pub fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        !self.is_known()
    }

    #[must_use]
    pub fn contains(self, offset: u32) -> bool {
        offset >= self.start && offset < self.end
    }
}

/// An index into [`Scene::nodes`](crate::Scene).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const ROOT: NodeId = NodeId(0);
    /// No node. Used while layout is still deciding whose glyph this is.
    pub const NONE: NodeId = NodeId(u32::MAX);

    #[must_use]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl Default for NodeId {
    fn default() -> NodeId {
        NodeId::NONE
    }
}

/// What part of a formula a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NodeKind {
    /// One symbol.
    Atom,
    /// A run of atoms with inter-atom spacing between them.
    Run,
    /// A braced group, which is a run that forced itself ordinary.
    Group,
    /// Explicit spacing.
    Kern,
    /// A base with scripts hanging off it.
    Scripts,
    Frac,
    Radical,
    /// A `\left…\right` pair and everything between.
    Delimited,
    /// `\sin` and the other upright names.
    Func,
    /// `\sum`, `\int`, `\lim` — the operator with its limits.
    BigOp,
    /// The limit itself: `\sum`'s lower and upper.
    Limit,
    Accent,
    /// `\text{…}`.
    Text,
    /// An `aligned` / `matrix` / `cases` environment.
    Env,
    /// A wrapper that only changes how its contents are drawn, e.g. `\color`.
    Style,
    /// `\phantom{…}`: laid out, measured, not drawn.
    Phantom,
    /// One thing set over or under another, e.g. `\overset`.
    Stack,
    /// A frame, fill or strike around or across its contents, e.g. `\boxed`, `\cancel`.
    Enclose,
    /// `\rule`: a filled box.
    Rule,
}

/// One node of the tree layout walked.
///
/// The box is in scene pixels, y down, origin on the baseline at the left edge.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Node {
    /// `None` only for the root.
    pub parent: Option<NodeId>,
    pub kind: NodeKind,
    /// The class this node contributed to its neighbours' spacing.
    pub class: AtomClass,
    pub span: Span,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// The node's own baseline, y down; not derivable from the box.
    pub baseline: f32,
}

impl Node {
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    #[must_use]
    pub fn area(&self) -> f32 {
        self.width * self.height
    }

    #[must_use]
    pub fn has_ink(&self) -> bool {
        self.width > 0.0 && self.height > 0.0
    }
}

/// A colour, straight 8-bit RGBA, **not** premultiplied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Paint {
    pub rgba: [u8; 4],
}

impl Paint {
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Paint {
        Paint {
            rgba: [r, g, b, 255],
        }
    }

    #[must_use]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Paint {
        Paint { rgba: [r, g, b, a] }
    }
}

/// An index into [`Scene::paints`](crate::Scene).
///
/// [`PaintId::INHERIT`] (index zero) means the consumer's current colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PaintId(pub u16);

impl PaintId {
    pub const INHERIT: PaintId = PaintId(0);

    #[must_use]
    pub fn index(self) -> usize {
        self.0 as usize
    }

    #[must_use]
    pub fn is_inherit(self) -> bool {
        self == PaintId::INHERIT
    }
}

impl Default for PaintId {
    fn default() -> PaintId {
        PaintId::INHERIT
    }
}

/// Interns `paint`; falls back to [`PaintId::INHERIT`] once a `u16` of indices is exhausted.
#[cfg(feature = "attrs")]
pub(crate) fn intern(paints: &mut Vec<Paint>, paint: Paint) -> PaintId {
    if let Some(index) = paints.iter().position(|p| *p == paint) {
        return PaintId(index as u16);
    }
    let Ok(index) = u16::try_from(paints.len()) else {
        return PaintId::INHERIT;
    };
    paints.push(paint);
    PaintId(index)
}

/// Everything a scene item carries beyond where it goes.
#[cfg(feature = "attrs")]
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct ItemAttrs {
    /// The innermost node that produced this item.
    pub node: NodeId,
    pub paint: PaintId,
    /// Opacity in `0.0..=1.0`; always 1 out of layout.
    pub alpha: f32,
}

#[cfg(feature = "attrs")]
impl Default for ItemAttrs {
    fn default() -> ItemAttrs {
        ItemAttrs {
            node: NodeId::NONE,
            paint: PaintId::INHERIT,
            alpha: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_span_is_not_a_zero_length_one_at_zero() {
        assert!(!Span::UNKNOWN.is_known());
        assert!(Span::new(0, 1).is_known());
        assert!(Span::new(2, 5).contains(2));
        assert!(!Span::new(2, 5).contains(5));
    }

    #[test]
    fn the_sentinel_is_not_a_real_node() {
        assert_ne!(NodeId::NONE, NodeId::ROOT);
        assert_eq!(NodeId::default(), NodeId::NONE);
    }

    #[test]
    fn inherit_is_the_default_paint() {
        assert!(PaintId::default().is_inherit());
        assert_eq!(PaintId::INHERIT.index(), 0);
    }
}
