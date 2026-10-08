//! Turning an [`Expr`] into positioned glyphs.
//! Fragment coordinates are pixels, y-down, origin at the left edge on the baseline.

mod accent;
mod arena;
mod bigop;
mod delim;
#[cfg(feature = "formatting")]
mod enclose;
mod frac;
mod grid;
mod radical;
mod script;
mod stack;

use crate::alphabet;
use crate::ast::{Alphabet, Expr, StyleSwitch};
use crate::attrs::{NodeId, NodeKind, Span};
use crate::class::{AtomClass, demote_binaries, spacing_mu};
use crate::error::Error;
use crate::font::{Fonts, MathFont};
use crate::scene::{FontKind, GlyphInstance, Rule, Scene, Shape, stroke_of};
use crate::style::MathStyle;
use crate::{MissingGlyph, Options};

use ttf_parser::GlyphId;

/// A half-open range into one of [`ItemBuf`]'s vectors.
/// `EMPTY` unions correctly by min/max with no special case.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ItemRange {
    pub start: usize,
    pub end: usize,
}

impl ItemRange {
    pub(crate) const EMPTY: ItemRange = ItemRange {
        start: usize::MAX,
        end: 0,
    };

    fn union(self, other: ItemRange) -> ItemRange {
        ItemRange {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Where an inkless item such as `\quad` sits: a node box for something with no ink.
/// Moved with its fragment like a glyph, but never part of the scene.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(not(feature = "attrs"), allow(dead_code))]
pub(crate) struct Spacer {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub node: NodeId,
}

#[derive(Debug, Default)]
pub(crate) struct ItemBuf {
    pub glyphs: Vec<GlyphInstance>,
    pub rules: Vec<Rule>,
    pub spacers: Vec<Spacer>,
}

impl ItemBuf {
    fn push_glyph(&mut self, glyph: GlyphInstance) -> ItemRange {
        self.glyphs.push(glyph);
        ItemRange {
            start: self.glyphs.len() - 1,
            end: self.glyphs.len(),
        }
    }

    fn push_rule(&mut self, rule: Rule) -> ItemRange {
        self.rules.push(rule);
        ItemRange {
            start: self.rules.len() - 1,
            end: self.rules.len(),
        }
    }

    fn push_spacer(&mut self, spacer: Spacer) -> ItemRange {
        self.spacers.push(spacer);
        ItemRange {
            start: self.spacers.len() - 1,
            end: self.spacers.len(),
        }
    }

    /// The current tails, for [`ItemBuf::truncate`].
    pub(crate) fn marks(&self) -> (usize, usize, usize) {
        (self.glyphs.len(), self.rules.len(), self.spacers.len())
    }

    /// Drop everything pushed since `marks`.
    /// Valid only when the abandoned items are the buffer tail.
    pub(crate) fn truncate(&mut self, marks: (usize, usize, usize)) {
        self.glyphs.truncate(marks.0);
        self.rules.truncate(marks.1);
        self.spacers.truncate(marks.2);
    }

    fn glyphs_mut(&mut self, range: ItemRange) -> &mut [GlyphInstance] {
        self.glyphs
            .get_mut(range.start..range.end)
            .unwrap_or_default()
    }

    fn rules_mut(&mut self, range: ItemRange) -> &mut [Rule] {
        self.rules
            .get_mut(range.start..range.end)
            .unwrap_or_default()
    }

    fn spacers_mut(&mut self, range: ItemRange) -> &mut [Spacer] {
        self.spacers
            .get_mut(range.start..range.end)
            .unwrap_or_default()
    }
}

/// A partially laid-out piece of a formula.
/// Not `Clone`: its items live in the [`ItemBuf`], and [`Frag::place`] consumes each child once.
#[derive(Debug)]
pub(crate) struct Frag {
    pub glyphs: ItemRange,
    pub rules: ItemRange,
    pub spacers: ItemRange,
    pub width: f32,
    /// Distance above the baseline. Never negative.
    pub ascent: f32,
    /// Distance below the baseline. Never negative.
    pub descent: f32,
    pub class: AtomClass,
    /// Overhang of a slanted glyph past its advance; non-zero only for a single glyph.
    pub italic: f32,
    /// Horizontal accent attachment from the left edge; `None` means the centre.
    pub accent_attach: Option<f32>,
    /// Set when the fragment is exactly one glyph.
    pub single: Option<GlyphId>,
    /// The node this fragment is, or [`NodeId::NONE`] if unclaimed.
    pub node: NodeId,
}

impl Frag {
    pub(crate) fn empty(class: AtomClass) -> Frag {
        Frag {
            glyphs: ItemRange::EMPTY,
            rules: ItemRange::EMPTY,
            spacers: ItemRange::EMPTY,
            width: 0.0,
            ascent: 0.0,
            descent: 0.0,
            class,
            italic: 0.0,
            accent_attach: None,
            single: None,
            node: NodeId::NONE,
        }
    }

    pub(crate) fn height(&self) -> f32 {
        self.ascent + self.descent
    }

    /// Move every item, leaving the extents alone. Call only on a fully assembled fragment:
    /// a range still under construction can cover unplaced siblings.
    fn translate(&self, buf: &mut ItemBuf, dx: f32, dy: f32) {
        for g in buf.glyphs_mut(self.glyphs) {
            g.x += dx;
            g.y += dy;
        }
        for r in buf.rules_mut(self.rules) {
            r.x += dx;
            r.y += dy;
        }
        for s in buf.spacers_mut(self.spacers) {
            s.x += dx;
            s.y += dy;
        }
    }

    /// Put `child` at `(dx, dy)` (y-down) inside `self`, growing `self`'s box to cover it.
    fn place(&mut self, buf: &mut ItemBuf, child: Frag, dx: f32, dy: f32) {
        child.translate(buf, dx, dy);
        self.glyphs = self.glyphs.union(child.glyphs);
        self.rules = self.rules.union(child.rules);
        self.spacers = self.spacers.union(child.spacers);
        self.width = self.width.max(dx + child.width);
        self.ascent = self.ascent.max(child.ascent - dy);
        self.descent = self.descent.max(child.descent + dy);
    }

    /// Shift vertically so the box is centred on `axis` (y-up from the baseline).
    fn centre_on(&mut self, buf: &mut ItemBuf, axis: f32) {
        let shift = axis - (self.ascent - self.descent) / 2.0;
        self.translate(buf, 0.0, -shift);
        self.ascent += shift;
        self.descent -= shift;
    }

    fn add_rule(&mut self, buf: &mut ItemBuf, rule: Rule) {
        self.width = self.width.max(rule.x + rule.width);
        self.ascent = self.ascent.max(-rule.y);
        self.descent = self.descent.max(rule.y + rule.height);
        self.rules = self.rules.union(buf.push_rule(rule));
    }

    /// Absorb a glyph just pushed to `buf`, growing the box to its extents.
    fn add_glyph_range(&mut self, range: ItemRange, ascent: f32, descent: f32) {
        self.glyphs = self.glyphs.union(range);
        self.ascent = self.ascent.max(ascent);
        self.descent = self.descent.max(descent);
    }
}

/// What layout needs to know that is not in the tree.
pub(crate) struct Ctx<'f, 'a> {
    pub fonts: &'f Fonts<'a>,
    pub items: ItemBuf,
    pub font_size: f32,
    /// The size the caller asked for; `\large` and friends scale from this, not from each other.
    pub base_font_size: f32,
    /// The `\middle`s of the innermost `\left…\right` being laid out; `None` outside one.
    pub middles: Option<Vec<delim::MiddleMark>>,
    pub on_missing: MissingGlyph,
    /// Characters that had no glyph, for the caller's diagnostics.
    pub missing: Vec<char>,
    /// The tree being walked, recorded for the consumer. See [`arena`].
    pub arena: arena::Arena,
    /// Interned colours; index zero is the inherit entry.
    #[cfg(feature = "attrs")]
    pub paints: Vec<crate::attrs::Paint>,
}

impl<'a> Ctx<'_, 'a> {
    pub(crate) fn math(&self) -> &MathFont<'a> {
        &self.fonts.math
    }

    /// Pixels per em in `style`.
    pub(crate) fn size(&self, style: MathStyle) -> f32 {
        self.font_size * style.scale(self.math())
    }

    #[cfg(feature = "attrs")]
    fn intern_paint(&mut self, paint: crate::attrs::Paint) -> crate::attrs::PaintId {
        crate::model::attrs::intern(&mut self.paints, paint)
    }
}

/// Lay a parsed expression out against a font.
///
/// # Errors
///
/// [`Error::MissingGlyph`] if a character has no glyph and the caller asked
/// for that to be fatal; [`Error::BadMetrics`] if the font's numbers produce
/// non-finite geometry.
pub(crate) fn layout(expr: &Expr, fonts: &Fonts<'_>, opts: &Options) -> Result<Scene, Error> {
    let mut ctx = Ctx {
        fonts,
        items: ItemBuf {
            glyphs: Vec::with_capacity(64),
            rules: Vec::with_capacity(8),
            spacers: Vec::new(),
        },
        font_size: opts.font_size,
        base_font_size: opts.font_size,
        middles: None,
        on_missing: opts.on_missing_glyph,
        missing: Vec::new(),
        arena: arena::Arena::new(),
        #[cfg(feature = "attrs")]
        paints: vec![crate::attrs::Paint::rgba(0, 0, 0, 0)],
    };
    let frag = expr_frag(&mut ctx, expr, opts.style, Alphabet::Auto)?;
    // Everything built was placed into the root or truncated, so the buffers are the scene.
    #[cfg(feature = "attrs")]
    let nodes = {
        let math = &ctx.fonts.math;
        let text = ctx.fonts.text.as_ref();
        let arena = core::mem::take(&mut ctx.arena);
        arena.finish(
            &ctx.items.glyphs,
            &ctx.items.rules,
            &ctx.items.spacers,
            |g| item_box(math, text, g),
        )
    };
    // Rules that layout drew as fills or lines go to their own lists.
    let mut rules = Vec::with_capacity(ctx.items.rules.len());
    let mut backgrounds = Vec::new();
    let mut strokes = Vec::new();
    for rule in ctx.items.rules {
        match rule.shape {
            Shape::Rect => rules.push(rule),
            Shape::Background => backgrounds.push(rule.with_shape(Shape::Rect)),
            Shape::Rising(thickness) => strokes.push(stroke_of(&rule, thickness, true)),
            Shape::Falling(thickness) => strokes.push(stroke_of(&rule, thickness, false)),
        }
    }
    let scene = Scene {
        glyphs: ctx.items.glyphs,
        rules,
        backgrounds,
        strokes,
        width: frag.width,
        ascent: frag.ascent,
        descent: frag.descent,
        font_size: opts.font_size,
        missing: ctx.missing,
        #[cfg(feature = "attrs")]
        nodes,
        #[cfg(feature = "attrs")]
        paints: ctx.paints,
    };
    // Absurd font metrics become an error here rather than NaN geometry downstream.
    if !scene.is_finite() {
        return Err(Error::BadMetrics);
    }
    Ok(scene)
}

/// One glyph's box in scene space; the width is the advance, so hit-testing has no gaps.
#[cfg(feature = "attrs")]
fn item_box(
    math: &MathFont<'_>,
    text: Option<&crate::font::TextFont<'_>>,
    g: &GlyphInstance,
) -> arena::Box2 {
    // Text-face and maths-face glyphs differ only in units per em.
    let (advance, ascent, descent) = match (g.font, text) {
        (FontKind::Text, Some(face)) => {
            let (top, bottom) = face
                .bbox(g.glyph)
                .map_or((0.0, 0.0), |b| (f32::from(b.y_max), f32::from(-b.y_min)));
            (
                face.px(face.advance(g.glyph), g.size),
                face.px(top, g.size).max(0.0),
                face.px(bottom, g.size).max(0.0),
            )
        }
        _ => {
            let (ascent, descent) = ink_extents(math, g.glyph, g.size);
            (math.px(math.advance(g.glyph), g.size), ascent, descent)
        }
    };
    (g.x, g.y - ascent, advance, ascent + descent)
}

/// Colour a fragment's still-uncoloured items; an inner wrapper's colour wins. A no-op without `attrs`.
#[allow(unused_variables)]
fn paint_frag(ctx: &mut Ctx<'_, '_>, frag: &mut Frag, decor: &crate::ast::Decor) {
    #[cfg(feature = "attrs")]
    {
        use crate::attrs::PaintId;
        let Some(colour) = decor.colour else {
            return;
        };
        let id = ctx.intern_paint(colour);
        for glyph in ctx.items.glyphs_mut(frag.glyphs) {
            if glyph.attrs.paint == PaintId::INHERIT {
                glyph.attrs.paint = id;
            }
        }
        for rule in ctx.items.rules_mut(frag.rules) {
            if rule.attrs.paint == PaintId::INHERIT {
                rule.attrs.paint = id;
            }
        }
    }
}

fn node_kind(expr: &Expr) -> NodeKind {
    match expr {
        Expr::Sym { .. } | Expr::SizedDelim { .. } | Expr::Middle(_) => NodeKind::Atom,
        Expr::Rule { .. } => NodeKind::Rule,
        Expr::Enclose { .. } => NodeKind::Enclose,
        Expr::Seq(_) => NodeKind::Run,
        Expr::Group(_) | Expr::Marked { .. } => NodeKind::Group,
        Expr::Adjust { .. }
        | Expr::Alphabet(..)
        | Expr::Styled { .. }
        | Expr::Class(..)
        | Expr::Style { .. } => NodeKind::Style,
        Expr::Phantom { .. } => NodeKind::Phantom,
        Expr::Stack { .. } | Expr::XArrow { .. } => NodeKind::Stack,
        Expr::HLine { .. } | Expr::Space(_) => NodeKind::Kern,
        Expr::Scripts { .. } => NodeKind::Scripts,
        Expr::Frac { .. } => NodeKind::Frac,
        Expr::Sqrt { .. } => NodeKind::Radical,
        Expr::Delimited { .. } => NodeKind::Delimited,
        Expr::Func(_) => NodeKind::Func,
        Expr::BigOp { .. } => NodeKind::BigOp,
        Expr::Accent { .. } => NodeKind::Accent,
        Expr::Text(_) => NodeKind::Text,
        Expr::Env(..) => NodeKind::Env,
    }
}

/// Lay an expression out, recording it as a node.
/// `alphabet` is inherited from an enclosing `\mathbf{…}`; a `Sym` or nested wrapper naming its own wins.
pub(crate) fn expr_frag(
    ctx: &mut Ctx<'_, '_>,
    expr: &Expr,
    style: MathStyle,
    alphabet: Alphabet,
) -> Result<Frag, Error> {
    // `Marked` only attaches a source span; it is not a node of its own.
    let (span, expr) = match expr {
        Expr::Marked { span, inner } => (*span, &**inner),
        other => (Span::UNKNOWN, other),
    };
    let node = ctx.arena.open(node_kind(expr), span);
    // Only a `\middle` at the pair's own level grows with it; one in a fraction or box does not.
    let level = matches!(
        expr,
        Expr::Seq(_)
            | Expr::Middle(_)
            | Expr::Group(_)
            | Expr::Styled { .. }
            | Expr::Style { .. }
            | Expr::Alphabet(..)
            | Expr::Class(..)
    );
    let saved = if level { None } else { ctx.middles.take() };
    let frag = expr_frag_inner(ctx, expr, style, alphabet);
    if !level {
        ctx.middles = saved;
    }
    let mut frag = frag?;
    ctx.arena.close(node, frag.class);
    adopt(ctx, &frag, node);
    frag.node = node;
    Ok(frag)
}

/// Children claimed their items on close; what is still unclaimed in the range is `node`'s.
fn adopt(ctx: &mut Ctx<'_, '_>, frag: &Frag, node: NodeId) {
    let Ctx { arena, items, .. } = &mut *ctx;
    arena.adopt(
        items
            .glyphs
            .get_mut(frag.glyphs.start..frag.glyphs.end)
            .unwrap_or_default(),
        items
            .rules
            .get_mut(frag.rules.start..frag.rules.end)
            .unwrap_or_default(),
        items
            .spacers
            .get_mut(frag.spacers.start..frag.spacers.end)
            .unwrap_or_default(),
        node,
    );
}

fn expr_frag_inner(
    ctx: &mut Ctx<'_, '_>,
    expr: &Expr,
    style: MathStyle,
    alphabet: Alphabet,
) -> Result<Frag, Error> {
    match expr {
        Expr::Sym {
            ch,
            class,
            alphabet: own,
        } => {
            let effective = if *own == Alphabet::Auto {
                alphabet
            } else {
                *own
            };
            sym(ctx, *ch, *class, effective, style)
        }
        Expr::Seq(items) => seq(ctx, items, style, alphabet),
        Expr::Group(inner) => {
            let mut frag = expr_frag(ctx, inner, style, alphabet)?;
            // `{+}` spaces as an ordinary.
            frag.class = AtomClass::Ord;
            Ok(frag)
        }
        Expr::Alphabet(a, inner) => match peel_marked(inner) {
            // Prose in a styled alphabet takes the maths font's letters in that style.
            Expr::Text(text) if !matches!(a, Alphabet::Auto | Alphabet::Upright) => {
                let node = ctx.arena.open(NodeKind::Text, marked_span(inner));
                let frag = glyph_run(ctx, text, *a, style)?;
                ctx.arena.close(node, AtomClass::Ord);
                adopt(ctx, &frag, node);
                Ok(frag)
            }
            _ => expr_frag(ctx, inner, style, *a),
        },
        Expr::Class(class, inner) => {
            let mut frag = expr_frag(ctx, inner, style, alphabet)?;
            frag.class = *class;
            Ok(frag)
        }
        Expr::Phantom {
            body,
            width,
            height,
        } => {
            // Laid out in full for its size, then emptied; the node stays for hit-testing.
            let marks = ctx.items.marks();
            let mut frag = expr_frag(ctx, body, style, alphabet)?;
            // The body is the newest subtree, so it is exactly the buffer tail.
            ctx.items.truncate(marks);
            frag.glyphs = ItemRange::EMPTY;
            frag.rules = ItemRange::EMPTY;
            frag.spacers = ItemRange::EMPTY;
            frag.single = None;
            frag.italic = 0.0;
            if !*width {
                frag.width = 0.0;
            }
            if !*height {
                frag.ascent = 0.0;
                frag.descent = 0.0;
            }
            Ok(frag)
        }
        Expr::XArrow { ch, over, under } => {
            stack::xarrow(ctx, *ch, over.as_deref(), under.as_deref(), style, alphabet)
        }
        Expr::Stack {
            over,
            base,
            under,
            class,
        } => stack::layout(
            ctx,
            over.as_deref(),
            base,
            under.as_deref(),
            *class,
            style,
            alphabet,
        ),
        Expr::Styled { decor, body } => {
            let mut frag = expr_frag(ctx, body, style, alphabet)?;
            paint_frag(ctx, &mut frag, decor);
            Ok(frag)
        }
        Expr::Space(mu) => {
            let mut frag = Frag::empty(AtomClass::Ord);
            frag.width = mu_to_px(f32::from(*mu), ctx.size(style));
            // No ink, so nothing else would say where the space is: the caret needs to know.
            frag.spacers = ctx.items.push_spacer(Spacer {
                x: 0.0,
                y: 0.0,
                width: frag.width,
                node: NodeId::NONE,
            });
            Ok(frag)
        }
        Expr::Scripts { base, sub, sup } => {
            script::layout(ctx, base, sub.as_deref(), sup.as_deref(), style, alphabet)
        }
        Expr::Frac {
            num,
            den,
            style: frac_style,
            bar,
        } => frac::layout(ctx, num, den, *frac_style, *bar, style, alphabet),
        Expr::Sqrt { degree, radicand } => {
            radical::layout(ctx, degree.as_deref(), radicand, style, alphabet)
        }
        Expr::Delimited { left, body, right } => {
            delim::layout(ctx, *left, body, *right, style, alphabet)
        }
        Expr::Func(name) => {
            let mut frag = glyph_run(ctx, name, Alphabet::Upright, style)?;
            frag.class = AtomClass::Op;
            Ok(frag)
        }
        Expr::BigOp {
            kind,
            lower,
            upper,
            limits,
        } => bigop::layout(
            ctx,
            *kind,
            lower.as_deref(),
            upper.as_deref(),
            *limits,
            style,
            alphabet,
        ),
        Expr::Accent { kind, base } => accent::layout(ctx, *kind, base, style, alphabet),
        Expr::Text(s) => text_run(ctx, s, style),
        Expr::Env(kind, rows) => grid::layout(ctx, *kind, rows, style, alphabet),
        Expr::Style {
            style: StyleSwitch::Size(switch),
            body,
        } => {
            let saved = ctx.font_size;
            ctx.font_size = ctx.base_font_size * switch.factor();
            let frag = expr_frag(ctx, body, style, alphabet);
            ctx.font_size = saved;
            frag
        }
        // A switch names a style; crampedness carries across.
        Expr::Style {
            style: switch,
            body,
        } => {
            let switched = match switch {
                StyleSwitch::Display => MathStyle::Display,
                StyleSwitch::Text => MathStyle::Text,
                StyleSwitch::Script => MathStyle::Script,
                StyleSwitch::ScriptScript | StyleSwitch::Size(_) => MathStyle::ScriptScript,
            };
            let switched = if style.is_cramped() {
                switched.cramped()
            } else {
                switched
            };
            expr_frag(ctx, body, switched, alphabet)
        }
        Expr::SizedDelim { ch, class, size } => delim::sized(ctx, *ch, *class, *size, style),
        Expr::Middle(delim) => delim::middle(ctx, *delim, style),
        #[cfg(feature = "formatting")]
        Expr::Rule {
            width,
            height,
            raise,
        } => Ok(enclose::rule(ctx, *width, *height, *raise, style)),
        #[cfg(feature = "formatting")]
        Expr::Adjust { adjust, body } => enclose::adjust(ctx, *adjust, body, style, alphabet),
        #[cfg(feature = "formatting")]
        Expr::Enclose { kind, body } => enclose::enclose(ctx, *kind, body, style, alphabet),
        #[cfg(feature = "formatting")]
        Expr::HLine { .. } => Ok(enclose::hline(ctx)),
        // Without `formatting` these are never parsed; one built by hand draws its body plainly.
        #[cfg(not(feature = "formatting"))]
        Expr::Adjust { body, .. } | Expr::Enclose { body, .. } => {
            expr_frag(ctx, body, style, alphabet)
        }
        #[cfg(not(feature = "formatting"))]
        Expr::Rule { .. } | Expr::HLine { .. } => Ok(Frag::empty(AtomClass::Ord)),
        // Stripped by `expr_frag` before this is called.
        Expr::Marked { inner, .. } => expr_frag_inner(ctx, inner, style, alphabet),
    }
}

fn peel_marked(expr: &Expr) -> &Expr {
    match expr {
        Expr::Marked { inner, .. } => peel_marked(inner),
        other => other,
    }
}

fn marked_span(expr: &Expr) -> Span {
    match expr {
        Expr::Marked { span, .. } => *span,
        _ => Span::UNKNOWN,
    }
}

/// Math units to pixels. 1 mu is 1/18 em of the current style's size.
pub(crate) fn mu_to_px(mu: f32, size: f32) -> f32 {
    mu * size / 18.0
}

/// A run of atoms, with TeX's inter-atom spacing between them.
fn seq(
    ctx: &mut Ctx<'_, '_>,
    items: &[Expr],
    style: MathStyle,
    alphabet: Alphabet,
) -> Result<Frag, Error> {
    let size = ctx.size(style);
    if items.is_empty() {
        return Ok(Frag::empty(AtomClass::Ord));
    }

    // Explicit kerns are transparent to the spacing table; the flag marks them.
    let mut frags: Vec<(Frag, bool)> = Vec::with_capacity(items.len());
    for item in items {
        let kern = item.is_transparent_kern();
        frags.push((expr_frag(ctx, item, style, alphabet)?, kern));
    }

    // Demote over the non-kern classes only, then write them back.
    {
        let mut visible: Vec<AtomClass> = frags
            .iter()
            .filter(|(_, kern)| !*kern)
            .map(|(frag, _)| frag.class)
            .collect();
        demote_binaries(&mut visible);
        let mut demoted = visible.iter();
        for (frag, kern) in &mut frags {
            if !*kern {
                if let Some(class) = demoted.next() {
                    frag.class = *class;
                }
            }
        }
    }

    let single_class = frags.first().map_or(AtomClass::Ord, |(frag, _)| frag.class);

    let mut out = Frag::empty(AtomClass::Inner);
    let mut x = 0.0f32;
    // Class of the last non-kern atom.
    let mut previous: Option<AtomClass> = None;
    let mut last_italic = 0.0f32;

    for (frag, kern) in frags {
        let class = frag.class;
        if !kern {
            if let Some(left) = previous {
                x += mu_to_px(spacing_mu(left, class, style.is_script()), size);
            }
        }
        let width = frag.width;
        let italic = frag.italic;
        out.place(&mut ctx.items, frag, x, 0.0);
        x += width;
        if kern {
            last_italic = 0.0;
        } else {
            previous = Some(class);
            last_italic = italic;
        }
    }

    out.width = x;
    // The run overhangs by as much as its last glyph.
    out.italic = last_italic;
    if items.len() == 1 {
        out.class = single_class;
    }
    Ok(out)
}

pub(crate) fn sym(
    ctx: &mut Ctx<'_, '_>,
    ch: char,
    class: AtomClass,
    alphabet: Alphabet,
    style: MathStyle,
) -> Result<Frag, Error> {
    let size = ctx.size(style);
    let requested = alphabet::remap(ch, alphabet);
    let Some(glyph) = resolve_glyph(ctx, ch, requested)? else {
        return Ok(Frag::empty(class));
    };
    Ok(glyph_frag(
        &ctx.fonts.math,
        &mut ctx.items,
        glyph,
        class,
        size,
    ))
}

/// Find a glyph for a styled character, falling back rather than giving up.
fn resolve_glyph(
    ctx: &mut Ctx<'_, '_>,
    original: char,
    requested: char,
) -> Result<Option<GlyphId>, Error> {
    let font = ctx.math();
    let found = font
        .glyph_for(requested)
        .or_else(|| font.glyph_for(alphabet::remap(original, Alphabet::Auto)))
        .or_else(|| font.glyph_for(original));
    if let Some(glyph) = found {
        return Ok(Some(glyph));
    }
    ctx.missing.push(requested);
    match ctx.on_missing {
        // A visible box, so a live preview keeps drawing.
        MissingGlyph::Notdef => Ok(Some(GlyphId(0))),
        MissingGlyph::Skip => Ok(None),
        MissingGlyph::Error => Err(Error::MissingGlyph(requested)),
    }
}

/// A fragment holding one glyph from the maths font, positioned at the origin.
pub(crate) fn glyph_frag(
    font: &MathFont<'_>,
    buf: &mut ItemBuf,
    glyph: GlyphId,
    class: AtomClass,
    size: f32,
) -> Frag {
    let (ascent, descent) = ink_extents(font, glyph, size);
    Frag {
        glyphs: buf.push_glyph(GlyphInstance::new(glyph, FontKind::Math, 0.0, 0.0, size)),
        rules: ItemRange::EMPTY,
        spacers: ItemRange::EMPTY,
        width: font.px(font.advance(glyph), size),
        ascent,
        descent,
        class,
        italic: font.px(font.italic_correction(glyph), size),
        accent_attach: font
            .top_accent_attachment(glyph)
            .map(|du| font.px(du, size)),
        single: Some(glyph),
        node: NodeId::NONE,
    }
}

/// A glyph's ink extents in pixels, y-up, clamped at zero.
/// Ink, not the face ascender: `MATH` vertical constraints are defined against ink.
pub(crate) fn ink_extents(font: &MathFont<'_>, glyph: GlyphId, size: f32) -> (f32, f32) {
    match font.bbox(glyph) {
        Some(b) => (
            font.px(f32::from(b.y_max), size).max(0.0),
            font.px(f32::from(-b.y_min), size).max(0.0),
        ),
        None => (0.0, 0.0),
    }
}

/// The lowest point of a glyph's ink, in pixels y-up, **not** clamped.
/// A combining accent sits wholly above the baseline, so its descent would misplace it.
pub(crate) fn ink_bottom(font: &MathFont<'_>, glyph: GlyphId, size: f32) -> f32 {
    font.bbox(glyph)
        .map_or(0.0, |b| font.px(f32::from(b.y_min), size))
}

/// A run of maths-font characters in one alphabet with no inter-atom spacing (`\sin` etc.).
fn glyph_run(
    ctx: &mut Ctx<'_, '_>,
    text: &str,
    alphabet: Alphabet,
    style: MathStyle,
) -> Result<Frag, Error> {
    let mut out = Frag::empty(AtomClass::Ord);
    let mut x = 0.0f32;
    let mut italic = 0.0f32;
    for ch in text.chars() {
        let frag = sym(ctx, ch, AtomClass::Ord, alphabet, style)?;
        let width = frag.width;
        italic = frag.italic;
        out.place(&mut ctx.items, frag, x, 0.0);
        x += width;
    }
    out.width = x;
    out.italic = italic;
    Ok(out)
}

/// `\text{…}`: prose, upright, with the source's own spaces.
fn text_run(ctx: &mut Ctx<'_, '_>, text: &str, style: MathStyle) -> Result<Frag, Error> {
    let fonts = ctx.fonts;
    let Some(font) = fonts.text.as_ref() else {
        // No text face: use the maths font's upright Latin.
        return glyph_run(ctx, text, Alphabet::Upright, style);
    };

    let size = ctx.size(style);
    let mut out = Frag::empty(AtomClass::Ord);
    let mut x = 0.0f32;
    for ch in text.chars() {
        let glyph = match font.glyph_for(ch) {
            Some(glyph) => glyph,
            None => {
                ctx.missing.push(ch);
                match ctx.on_missing {
                    // Same visible box as `resolve_glyph`.
                    MissingGlyph::Notdef => GlyphId(0),
                    MissingGlyph::Skip => continue,
                    MissingGlyph::Error => return Err(Error::MissingGlyph(ch)),
                }
            }
        };
        let advance = font.px(font.advance(glyph), size);
        let (ascent, descent) = match font.bbox(glyph) {
            Some(b) => (
                font.px(f32::from(b.y_max), size).max(0.0),
                font.px(f32::from(-b.y_min), size).max(0.0),
            ),
            None => (0.0, 0.0),
        };
        let range = ctx
            .items
            .push_glyph(GlyphInstance::new(glyph, FontKind::Text, x, 0.0, size));
        out.add_glyph_range(range, ascent, descent);
        x += advance;
    }
    out.width = x;
    Ok(out)
}
