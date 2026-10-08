//! Stretchy delimiters: pick a size variant from the font or assemble one from parts.

use ttf_parser::GlyphId;
use ttf_parser::math::{GlyphAssembly, GlyphConstruction, GlyphPart};

use crate::ast::{Alphabet, Delim, Expr};
use crate::class::AtomClass;
use crate::error::Error;
use crate::font::MathFont;
use crate::scene::{FontKind, GlyphInstance};
use crate::style::MathStyle;

use crate::attrs::NodeId;

use super::{Ctx, Frag, ItemRange, Spacer, expr_frag, glyph_frag, ink_extents, resolve_glyph};

/// Cap on extender repetitions, against degenerate font data.
const MAX_REPETITIONS: u16 = 128;

/// A `\middle` laid out at no width, waiting for its pair to know how tall to grow.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MiddleMark {
    /// A zero-width spacer that moves with the body, so it ends up where the delimiter goes.
    spacer: usize,
    delim: Delim,
    /// Pixels per em where it sits.
    size: f32,
}

pub(crate) fn layout(
    ctx: &mut Ctx<'_, '_>,
    left: Delim,
    body: &Expr,
    right: Delim,
    style: MathStyle,
    alphabet: Alphabet,
) -> Result<Frag, Error> {
    let saved = ctx.middles.replace(Vec::new());
    let laid = expr_frag(ctx, body, style, alphabet);
    let marks = core::mem::replace(&mut ctx.middles, saved).unwrap_or_default();
    let body = laid?;
    let size = ctx.size(style);
    let target = target_height(ctx, &body, style);

    let left = delim_frag(ctx, left, target, size, AtomClass::Open)?;
    let mut out = Frag::empty(AtomClass::Inner);
    let left_width = left.width;
    out.place(&mut ctx.items, left, 0.0, 0.0);
    let body_width = body.width;
    let body_ranges = (body.glyphs, body.rules, body.spacers);
    out.place(&mut ctx.items, body, left_width, 0.0);
    let grown = insert_middles(ctx, &mut out, body_ranges, &marks, target)?;
    let x = left_width + body_width + grown;
    let right = delim_frag(ctx, right, target, size, AtomClass::Close)?;
    let right_width = right.width;
    out.place(&mut ctx.items, right, x, 0.0);
    out.width = x + right_width;
    Ok(out)
}

/// Draw each `\middle` at the pair's size where its marker ended up, pushing what follows
/// it right to make room. Returns the width added.
fn insert_middles(
    ctx: &mut Ctx<'_, '_>,
    out: &mut Frag,
    (glyphs, rules, spacers): (ItemRange, ItemRange, ItemRange),
    marks: &[MiddleMark],
    target: Option<f32>,
) -> Result<f32, Error> {
    // Where each marker landed, left to right.
    let mut at: Vec<(f32, MiddleMark)> = marks
        .iter()
        .filter_map(|mark| Some((ctx.items.spacers.get(mark.spacer)?.x, *mark)))
        .collect();
    at.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut grown = 0.0f32;
    let mut placed = Vec::with_capacity(at.len());
    for &(x, mark) in &at {
        let frag = delim_frag(ctx, mark.delim, target, mark.size, AtomClass::Ord)?;
        let width = frag.width;
        placed.push((x, grown, frag));
        grown += width;
    }
    if placed.is_empty() {
        return Ok(0.0);
    }
    // Everything at or past a marker moves right by the widths of the middles before it.
    let shift = |x: f32| {
        placed
            .iter()
            .filter(|(at, _, _)| x >= *at - 1e-3)
            .map(|(_, _, f)| f.width)
            .sum::<f32>()
    };
    let items = &mut ctx.items;
    for g in items
        .glyphs
        .get_mut(glyphs.start..glyphs.end)
        .unwrap_or_default()
    {
        g.x += shift(g.x);
    }
    for r in items
        .rules
        .get_mut(rules.start..rules.end)
        .unwrap_or_default()
    {
        r.x += shift(r.x);
    }
    for s in items
        .spacers
        .get_mut(spacers.start..spacers.end)
        .unwrap_or_default()
    {
        s.x += shift(s.x);
    }
    for ((x, before, mut frag), mark) in placed.into_iter().zip(at.iter().map(|(_, m)| *m)) {
        // The delimiter's ink is the `\middle` node's, which its marker was given.
        #[cfg(feature = "attrs")]
        if let Some(node) = ctx.items.spacers.get(mark.spacer).map(|s| s.node) {
            for g in ctx.items.glyphs_mut(frag.glyphs) {
                g.attrs.node = node;
            }
        }
        #[cfg(not(feature = "attrs"))]
        let _ = mark;
        frag.class = AtomClass::Ord;
        out.place(&mut ctx.items, frag, x + before, 0.0);
    }
    Ok(grown)
}

/// How tall a pair around `body` grows; `None` keeps them at natural size.
fn target_height(ctx: &Ctx<'_, '_>, body: &Frag, style: MathStyle) -> Option<f32> {
    let size = ctx.size(style);
    let font = &ctx.fonts.math;
    let axis = font.axis_height(size);
    // A threshold, not a floor: shorter contents keep natural-size delimiters.
    let minimum = font.px(
        f32::from(font.constants().delimited_sub_formula_min_height()),
        size,
    );
    if body.height() < minimum {
        None
    } else {
        Some(2.0 * (body.ascent - axis).max(body.descent + axis))
    }
}

/// `\middle`: a marker inside a pair, which grows it later; natural size outside one.
pub(crate) fn middle(ctx: &mut Ctx<'_, '_>, delim: Delim, style: MathStyle) -> Result<Frag, Error> {
    let size = ctx.size(style);
    if ctx.middles.is_none() {
        return delim_frag(ctx, delim, None, size, AtomClass::Ord);
    }
    let mut frag = Frag::empty(AtomClass::Ord);
    frag.spacers = ctx.items.push_spacer(Spacer {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        node: NodeId::NONE,
    });
    if let Some(marks) = ctx.middles.as_mut() {
        marks.push(MiddleMark {
            spacer: frag.spacers.start,
            delim,
            size,
        });
    }
    Ok(frag)
}

/// One delimiter; a `target` of `None` draws it at natural size on the baseline.
pub(super) fn delim_frag(
    ctx: &mut Ctx<'_, '_>,
    delim: Delim,
    target: Option<f32>,
    size: f32,
    class: AtomClass,
) -> Result<Frag, Error> {
    let Delim::Char(ch) = delim else {
        return Ok(Frag::empty(class));
    };
    let Some(glyph) = resolve_glyph(ctx, ch, ch)? else {
        return Ok(Frag::empty(class));
    };
    let fonts = ctx.fonts;
    let font = &fonts.math;
    let Some(target) = target else {
        return Ok(glyph_frag(font, &mut ctx.items, glyph, class, size));
    };
    let mut frag = stretch_vertical(font, &mut ctx.items, glyph, target, size, class);
    frag.centre_on(&mut ctx.items, font.axis_height(size));
    Ok(frag)
}

/// A `\big`-family delimiter: a fixed multiple of the text size, centred on the maths axis.
pub(crate) fn sized(
    ctx: &mut Ctx<'_, '_>,
    ch: char,
    class: AtomClass,
    size: crate::ast::DelimSize,
    style: MathStyle,
) -> Result<Frag, Error> {
    let px = ctx.size(style);
    delim_frag(ctx, Delim::Char(ch), Some(size.em() * px), px, class)
}

/// The tallest-enough form of `glyph`, with its pen on the baseline.
pub(crate) fn stretch_vertical(
    font: &MathFont<'_>,
    buf: &mut super::ItemBuf,
    glyph: GlyphId,
    target: f32,
    size: f32,
    class: AtomClass,
) -> Frag {
    stretch_along(font, buf, glyph, target, size, class, Axis::Vertical)
}

/// The widest-enough form of `glyph`: `\widehat`, `\overbrace`, `\xrightarrow`.
pub(crate) fn stretch_horizontal(
    font: &MathFont<'_>,
    buf: &mut super::ItemBuf,
    glyph: GlyphId,
    target: f32,
    size: f32,
    class: AtomClass,
) -> Frag {
    stretch_along(font, buf, glyph, target, size, class, Axis::Horizontal)
}

/// First big-enough variant, else an assembly, else the largest variant, else the glyph.
fn stretch_along(
    font: &MathFont<'_>,
    buf: &mut super::ItemBuf,
    glyph: GlyphId,
    target: f32,
    size: f32,
    class: AtomClass,
    axis: Axis,
) -> Frag {
    let Some(variants) = font.variants() else {
        return glyph_frag(font, buf, glyph, class, size);
    };
    let constructions = match axis {
        Axis::Vertical => variants.vertical_constructions,
        Axis::Horizontal => variants.horizontal_constructions,
    };
    let Some(construction) = constructions.get(glyph) else {
        return glyph_frag(font, buf, glyph, class, size);
    };

    let (largest, reached) = scan_variants(font, &construction, glyph, target, size);
    if reached {
        return glyph_frag(font, buf, largest, class, size);
    }

    if let Some(assembly) = construction.assembly {
        if let Some(frag) = assemble_along(
            font,
            buf,
            &assembly,
            f32::from(variants.min_connector_overlap),
            target,
            size,
            class,
            axis,
        ) {
            return frag;
        }
    }
    glyph_frag(font, buf, largest, class, size)
}

/// The first variant whose advance reaches `target`, else the largest, and whether it reached.
pub(crate) fn scan_variants(
    font: &MathFont<'_>,
    construction: &GlyphConstruction<'_>,
    glyph: GlyphId,
    target: f32,
    size: f32,
) -> (GlyphId, bool) {
    let mut largest = glyph;
    for variant in construction.variants {
        largest = variant.variant_glyph;
        if font.px(f32::from(variant.advance_measurement), size) >= target {
            return (variant.variant_glyph, true);
        }
    }
    (largest, false)
}

/// Minimum overlap between assembly parts, in ems.
const ASSEMBLY_MIN_OVERLAP_EM: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Vertical,
    Horizontal,
}

/// Lay the parts of an assembly end to end along `axis` to reach `target`.
#[allow(clippy::too_many_arguments)]
fn assemble_along(
    font: &MathFont<'_>,
    buf: &mut super::ItemBuf,
    assembly: &GlyphAssembly<'_>,
    min_overlap_du: f32,
    target: f32,
    size: f32,
    class: AtomClass,
    axis: Axis,
) -> Option<Frag> {
    let advance = |p: &GlyphPart| font.px(f32::from(p.full_advance), size);

    let mut fixed_count = 0usize;
    let mut extender_count = 0usize;
    let mut sum_fixed = 0.0f32;
    let mut sum_ext = 0.0f32;
    let mut smallest = f32::INFINITY;
    for part in assembly.parts {
        let advance = advance(&part);
        smallest = smallest.min(advance);
        if part.part_flags.extender() {
            extender_count += 1;
            sum_ext += advance;
        } else {
            fixed_count += 1;
            sum_fixed += advance;
        }
    }
    if fixed_count + extender_count == 0 {
        return None;
    }

    // Font minimums are often butt joints, which show as seams in a distance field.
    let overlap_min = font
        .px(min_overlap_du, size)
        .max(ASSEMBLY_MIN_OVERLAP_EM * size)
        .min(smallest.max(0.0) * 0.45);

    let reach = |r: u16| -> (usize, f32) {
        let n = fixed_count + extender_count * usize::from(r);
        let height =
            sum_fixed + sum_ext * f32::from(r) - overlap_min * (n.saturating_sub(1) as f32);
        (n, height)
    };

    let mut repetitions = if extender_count == 0 { 0 } else { 1 };
    if extender_count > 0 {
        while repetitions < MAX_REPETITIONS && reach(repetitions).1 < target {
            repetitions += 1;
        }
    }
    let (count, height) = reach(repetitions);

    // Spread the excess evenly over the joints; leftover slack stays as extra length.
    let joints = count.saturating_sub(1);
    let slack = if joints > 0 {
        ((height - target) / joints as f32).max(0.0)
    } else {
        0.0
    };

    let mut out = Frag::empty(class);
    let mut width = 0.0f32;
    // Along `axis`; y-up when vertical, negated at the push below.
    let mut pen = 0.0f32;
    let mut previous: Option<GlyphPart> = None;
    for part in assembly.parts {
        let repeats = if part.part_flags.extender() {
            repetitions
        } else {
            1
        };
        for _ in 0..repeats {
            if let Some(previous) = previous {
                let room = font
                    .px(f32::from(previous.end_connector_length), size)
                    .min(font.px(f32::from(part.start_connector_length), size));
                let overlap = (overlap_min + slack).min(room).max(0.0);
                pen += advance(&previous) - overlap;
            }
            let (ascent, descent) = ink_extents(font, part.glyph_id, size);
            let (x, y) = match axis {
                Axis::Vertical => (0.0, -pen),
                Axis::Horizontal => (pen, 0.0),
            };
            let glyph = GlyphInstance::new(part.glyph_id, FontKind::Math, x, y, size);
            let range = buf.push_glyph(glyph);
            match axis {
                Axis::Vertical => {
                    width = width.max(font.px(font.advance(part.glyph_id), size));
                    out.add_glyph_range(range, pen + ascent, descent - pen);
                }
                Axis::Horizontal => {
                    width = pen + advance(&part);
                    out.add_glyph_range(range, ascent, descent);
                }
            }
            previous = Some(part);
        }
    }

    out.width = width;
    Some(out)
}
