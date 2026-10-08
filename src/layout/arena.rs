//! Records the expression tree layout walked, so scene items map back to nodes.
//! Nodes are stored in pre-order (parents before children).

#[cfg(feature = "attrs")]
use crate::attrs::Node;
use crate::attrs::{NodeId, NodeKind, Span};
use crate::class::AtomClass;
use crate::layout::Spacer;
use crate::scene::{GlyphInstance, Rule};

/// A glyph's box: left, top, width, height, in scene coordinates, y-down.
#[cfg(feature = "attrs")]
pub(crate) type Box2 = (f32, f32, f32, f32);

#[cfg(feature = "attrs")]
#[derive(Debug, Default)]
pub(crate) struct Arena {
    parent: Vec<Option<NodeId>>,
    kind: Vec<NodeKind>,
    class: Vec<AtomClass>,
    span: Vec<Span>,
    /// Currently open nodes, innermost last.
    stack: Vec<NodeId>,
}

#[cfg(feature = "attrs")]
impl Arena {
    pub(crate) fn new() -> Arena {
        Arena::default()
    }

    pub(crate) fn open(&mut self, kind: NodeKind, span: Span) -> NodeId {
        let id = NodeId(self.parent.len() as u32);
        self.parent.push(self.stack.last().copied());
        self.kind.push(kind);
        self.class.push(AtomClass::Ord);
        self.span.push(span);
        self.stack.push(id);
        id
    }

    pub(crate) fn close(&mut self, id: NodeId, class: AtomClass) {
        if let Some(slot) = self.class.get_mut(id.index()) {
            *slot = class;
        }
        // An early `?` in `expr_frag` can leave the stack deep; pop back to `id`.
        while let Some(top) = self.stack.pop() {
            if top == id {
                break;
            }
        }
    }

    /// Give every item that no deeper node claimed to `id`.
    pub(crate) fn adopt(
        &self,
        glyphs: &mut [GlyphInstance],
        rules: &mut [Rule],
        spacers: &mut [Spacer],
        id: NodeId,
    ) {
        for s in spacers {
            if s.node == NodeId::NONE {
                s.node = id;
            }
        }
        for g in glyphs {
            if g.attrs.node == NodeId::NONE {
                g.attrs.node = id;
            }
        }
        for r in rules {
            if r.attrs.node == NodeId::NONE {
                r.attrs.node = id;
            }
        }
    }

    pub(crate) fn finish(
        self,
        glyphs: &[GlyphInstance],
        rules: &[Rule],
        spacers: &[Spacer],
        glyph_box: impl Fn(&GlyphInstance) -> Box2,
    ) -> Vec<Node> {
        let count = self.parent.len();
        let mut min = vec![(f32::INFINITY, f32::INFINITY); count];
        let mut max = vec![(f32::NEG_INFINITY, f32::NEG_INFINITY); count];
        let mut baseline: Vec<Option<f32>> = vec![None; count];

        for g in glyphs {
            let index = g.attrs.node.index();
            let (x, y, w, h) = glyph_box(g);
            grow(&mut min, &mut max, index, x, y, w, h);
            if let Some(slot) = baseline.get_mut(index) {
                // All of a node's own glyphs share one baseline.
                slot.get_or_insert(g.y);
            }
        }
        for r in rules {
            let index = r.attrs.node.index();
            grow(&mut min, &mut max, index, r.x, r.y, r.width, r.height);
        }
        // Flat on the baseline: a position and a width, no height, so never hit by a click.
        for s in spacers {
            grow(&mut min, &mut max, s.node.index(), s.x, s.y, s.width, 0.0);
        }

        for index in (0..count).rev() {
            let Some(Some(parent)) = self.parent.get(index).copied() else {
                continue;
            };
            let (Some(lo), Some(hi)) = (min.get(index).copied(), max.get(index).copied()) else {
                continue;
            };
            if lo.0 > hi.0 {
                continue; // no ink beneath this node at all
            }
            grow(
                &mut min,
                &mut max,
                parent.index(),
                lo.0,
                lo.1,
                hi.0 - lo.0,
                hi.1 - lo.1,
            );
        }

        let mut nodes = Vec::with_capacity(count);
        for index in 0..count {
            let lo = min.get(index).copied().unwrap_or((0.0, 0.0));
            let hi = max.get(index).copied().unwrap_or((0.0, 0.0));
            let (x, y, width, height) = if lo.0 > hi.0 {
                (0.0, 0.0, 0.0, 0.0)
            } else {
                (lo.0, lo.1, hi.0 - lo.0, hi.1 - lo.1)
            };
            let parent = self.parent.get(index).copied().flatten();
            let own = baseline.get(index).copied().flatten();
            // Parents are finished before children, so this is already final.
            let inherited = parent
                .and_then(|p| nodes.get(p.index()))
                .map_or(0.0, |n: &Node| n.baseline);
            nodes.push(Node {
                parent,
                kind: self.kind.get(index).copied().unwrap_or(NodeKind::Atom),
                class: self.class.get(index).copied().unwrap_or(AtomClass::Ord),
                span: self.span.get(index).copied().unwrap_or(Span::UNKNOWN),
                x,
                y,
                width,
                height,
                baseline: own.unwrap_or(inherited),
            });
        }
        nodes
    }
}

#[cfg(feature = "attrs")]
fn grow(
    min: &mut [(f32, f32)],
    max: &mut [(f32, f32)],
    index: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    // A zero-extent item still establishes a position.
    if let Some(lo) = min.get_mut(index) {
        lo.0 = lo.0.min(x);
        lo.1 = lo.1.min(y);
    }
    if let Some(hi) = max.get_mut(index) {
        hi.0 = hi.0.max(x + w);
        hi.1 = hi.1.max(y + h);
    }
}

#[cfg(not(feature = "attrs"))]
#[derive(Debug, Default)]
pub(crate) struct Arena;

#[cfg(not(feature = "attrs"))]
impl Arena {
    pub(crate) fn new() -> Arena {
        Arena
    }

    pub(crate) fn open(&mut self, _kind: NodeKind, _span: Span) -> NodeId {
        NodeId::NONE
    }

    pub(crate) fn close(&mut self, _id: NodeId, _class: AtomClass) {}

    pub(crate) fn adopt(
        &self,
        _glyphs: &mut [GlyphInstance],
        _rules: &mut [Rule],
        _spacers: &mut [Spacer],
        _id: NodeId,
    ) {
    }
}
