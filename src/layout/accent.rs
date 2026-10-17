//! Accent layout: glyph accents, stretchy wide accents, and the `\overline`/`\underline` rules.

use crate::ast::{Accent, Alphabet, Expr};
use crate::class::AtomClass;
use crate::error::Error;
use crate::scene::Rule;
use crate::style::MathStyle;

use super::{Ctx, Frag, expr_frag, glyph_frag};

pub(crate) fn layout(
    ctx: &mut Ctx<'_, '_>,
    kind: Accent,
    base: &Expr,
    style: MathStyle,
    alphabet: Alphabet,
) -> Result<Frag, Error> {
    // Cramped: there is now something directly above the base.
    let base = expr_frag(ctx, base, style.cramped(), alphabet)?;
    Ok(match kind {
        Accent::Overline => overline(ctx, base, style),
        Accent::Underline => underline(ctx, base, style),
        Accent::OverLineSegment => line_segment(ctx, base, style, false),
        Accent::UnderLineSegment => line_segment(ctx, base, style, true),
        Accent::Not => strike(ctx, base, style),
        _ if kind.is_under() => under_accent(ctx, kind, base, style),
        _ => glyph_accent(ctx, kind, base, style),
    })
}

/// Returns the accent fragment and the glyph actually drawn (stretching may pick a variant).
fn accent_frag(
    ctx: &mut Ctx<'_, '_>,
    kind: Accent,
    base_width: f32,
    style: MathStyle,
) -> Option<(Frag, ttf_parser::GlyphId)> {
    let size = ctx.size(style);
    let fonts = ctx.fonts;
    let font = &fonts.math;
    let ch = kind.char()?;
    // Prefer the combining form, then the spacing one.
    let glyph = font
        .glyph_for(ch)
        .or_else(|| font.glyph_for(spacing_form(kind)))?;
    let frag = if kind.is_wide() {
        super::delim::stretch_horizontal(
            font,
            &mut ctx.items,
            glyph,
            base_width,
            size,
            AtomClass::Ord,
        )
    } else {
        glyph_frag(font, &mut ctx.items, glyph, AtomClass::Ord, size)
    };
    let drawn = frag.single.unwrap_or(glyph);
    Some((frag, drawn))
}

fn under_accent(ctx: &mut Ctx<'_, '_>, kind: Accent, base: Frag, style: MathStyle) -> Frag {
    let size = ctx.size(style);
    // Copy the reference so the font borrow is independent of `ctx`.
    let fonts = ctx.fonts;
    let font = &fonts.math;
    let c = font.constants();
    let Some((accent, drawn)) = accent_frag(ctx, kind, base.width, style) else {
        return base;
    };

    let gap = font.k(c.underbar_vertical_gap(), size);
    let extra = font.k(c.underbar_extra_descender(), size);
    // Ink top, y-up from the accent baseline; usually negative for a bottom brace.
    let (ink_top, _) = super::ink_extents(font, drawn, size);
    let dy = base.descent + gap + ink_top;

    let base_width = base.width;
    let base_italic = base.italic;
    let dx = (base_width - accent.width) / 2.0;
    let mut out = Frag::empty(AtomClass::Ord);
    out.place(&mut ctx.items, base, 0.0, 0.0);
    out.place(&mut ctx.items, accent, dx.max(0.0), dy);
    out.width = base_width;
    out.italic = base_italic;
    out.descent += extra;
    out
}

/// `\not`: a solidus through the base, keeping the base class so `\not=` spaces as a relation.
fn strike(ctx: &mut Ctx<'_, '_>, base: Frag, style: MathStyle) -> Frag {
    let Some((accent, _)) = accent_frag(ctx, Accent::Not, base.width, style) else {
        return base;
    };
    // A combining overlay is centred on the base box, not placed after it.
    let dx = (base.width - accent.width) / 2.0;
    let base_width = base.width;
    let base_italic = base.italic;
    let base_attach = base.accent_attach;
    let mut out = Frag::empty(base.class);
    out.place(&mut ctx.items, base, 0.0, 0.0);
    out.place(&mut ctx.items, accent, dx, 0.0);
    out.width = base_width;
    out.italic = base_italic;
    out.accent_attach = base_attach;
    out
}

fn glyph_accent(ctx: &mut Ctx<'_, '_>, kind: Accent, base: Frag, style: MathStyle) -> Frag {
    let size = ctx.size(style);
    let fonts = ctx.fonts;
    let font = &fonts.math;
    let Some((accent, drawn)) = accent_frag(ctx, kind, base.width, style) else {
        return base;
    };

    // Align attachment points; half the base overhang stands in when the font gives none.
    let base_attach = base
        .accent_attach
        .unwrap_or(base.width / 2.0 + base.italic / 2.0);
    let accent_attach = accent.accent_attach.unwrap_or(accent.width / 2.0);
    let dx = base_attach - accent_attach;

    let c = font.constants();
    let base_height = font.k(c.accent_base_height(), size);
    // A combining mark is drawn to sit over a base `accentBaseHeight` tall, clearance
    // included; it rises with a taller base and is never lowered onto a shorter one.
    let lift = (base.ascent - base_height).max(0.0);
    let gap = if kind.clears_base() {
        font.k(c.overbar_vertical_gap(), size)
    } else {
        0.0
    };
    let dy = match accent.single {
        Some(_) => {
            // Ink bottom after the lift, y-up; arrows and braces also keep the overbar gap.
            let bottom = super::ink_bottom(font, drawn, size) + lift;
            -(lift + (base.ascent + gap - bottom).max(0.0))
        }
        // An assembled accent has no single glyph to measure, so its fragment box is used.
        None => -(base.ascent.max(base_height) + gap) - accent.descent,
    };

    let mut out = Frag::empty(AtomClass::Ord);
    let base_width = base.width;
    let base_italic = base.italic;
    out.place(&mut ctx.items, base, 0.0, 0.0);
    out.place(&mut ctx.items, accent, dx.max(0.0), dy);
    // The accent must not widen the atom.
    out.width = base_width;
    out.italic = base_italic;
    out
}

/// The free-standing form of an accent, for a font without the combining one.
fn spacing_form(kind: Accent) -> char {
    match kind {
        Accent::Hat | Accent::WideHat => '\u{02C6}',
        Accent::Tilde | Accent::WideTilde => '\u{02DC}',
        Accent::Bar => '\u{00AF}',
        Accent::Vec => '\u{2192}',
        Accent::Dot => '\u{02D9}',
        Accent::DDot => '\u{00A8}',
        // No spacing form for three or four dots; fall back on two.
        Accent::DDDot | Accent::DDDDot => '\u{00A8}',
        Accent::Acute => '\u{00B4}',
        Accent::Grave => '\u{0060}',
        Accent::Breve => '\u{02D8}',
        Accent::Check | Accent::WideCheck => '\u{02C7}',
        Accent::Ring => '\u{02DA}',
        Accent::DoubleAcute => '\u{02DD}',
        Accent::UnderTilde => '\u{02DC}',
        Accent::UnderLeftRightArrow => '\u{2194}',
        Accent::OverLeftHarpoon => '\u{21BC}',
        Accent::OverRightHarpoon => '\u{21C0}',
        Accent::OverRightDoubleArrow => '\u{21D2}',
        Accent::OverGroup => '\u{23DE}',
        Accent::UnderGroup => '\u{23DF}',
        // Substitutes for fonts without the horizontal accent glyphs.
        Accent::Not => '\u{002F}',
        Accent::OverBrace => '\u{FE37}',
        Accent::UnderBrace => '\u{FE38}',
        Accent::OverBracket | Accent::UnderBracket => '\u{005F}',
        Accent::OverRightArrow | Accent::UnderRightArrow => '\u{2192}',
        Accent::OverLeftArrow | Accent::UnderLeftArrow => '\u{2190}',
        Accent::OverLeftRightArrow => '\u{2194}',
        Accent::Overline
        | Accent::Underline
        | Accent::OverLineSegment
        | Accent::UnderLineSegment => '\u{0020}',
    }
}

fn overline(ctx: &mut Ctx<'_, '_>, base: Frag, style: MathStyle) -> Frag {
    let size = ctx.size(style);
    let fonts = ctx.fonts;
    let font = &fonts.math;
    let c = font.constants();
    let thickness = nonzero(font.k(c.overbar_rule_thickness(), size), font, size);
    let gap = font.k(c.overbar_vertical_gap(), size);
    let extra = font.k(c.overbar_extra_ascender(), size);

    let width = base.width;
    let top = base.ascent + gap + thickness;
    let mut out = Frag::empty(AtomClass::Ord);
    out.place(&mut ctx.items, base, 0.0, 0.0);
    out.add_rule(&mut ctx.items, Rule::new(0.0, -top, width, thickness));
    out.width = width;
    out.ascent = out.ascent.max(top + extra);
    out
}

fn underline(ctx: &mut Ctx<'_, '_>, base: Frag, style: MathStyle) -> Frag {
    let size = ctx.size(style);
    let fonts = ctx.fonts;
    let font = &fonts.math;
    let c = font.constants();
    let thickness = nonzero(font.k(c.underbar_rule_thickness(), size), font, size);
    let gap = font.k(c.underbar_vertical_gap(), size);
    let extra = font.k(c.underbar_extra_descender(), size);

    let width = base.width;
    let top = base.descent + gap;
    let mut out = Frag::empty(AtomClass::Ord);
    out.place(&mut ctx.items, base, 0.0, 0.0);
    out.add_rule(&mut ctx.items, Rule::new(0.0, top, width, thickness));
    out.width = width;
    out.descent = out.descent.max(top + thickness + extra);
    out
}

/// `\overlinesegment` and `\underlinesegment`: a bar with a tick at each end, pointing at the base.
fn line_segment(ctx: &mut Ctx<'_, '_>, base: Frag, style: MathStyle, under: bool) -> Frag {
    let size = ctx.size(style);
    let fonts = ctx.fonts;
    let font = &fonts.math;
    let c = font.constants();
    let thickness = nonzero(font.k(c.overbar_rule_thickness(), size), font, size);
    let gap = font.k(c.overbar_vertical_gap(), size);
    let extra = font.k(c.overbar_extra_ascender(), size);
    let tick = 0.15 * size;

    let width = base.width;
    let (ascent, descent) = (base.ascent, base.descent);
    let mut out = Frag::empty(AtomClass::Ord);
    out.place(&mut ctx.items, base, 0.0, 0.0);
    // y-down tops of the bar and the ticks.
    let (bar, ticks) = if under {
        let top = descent + gap + tick - thickness;
        (top, top + thickness - tick)
    } else {
        let top = -(ascent + gap + tick);
        (top, top)
    };
    out.add_rule(&mut ctx.items, Rule::new(0.0, bar, width, thickness));
    for x in [0.0, width - thickness] {
        out.add_rule(&mut ctx.items, Rule::new(x, ticks, thickness, tick));
    }
    out.width = width;
    if under {
        out.descent = out.descent.max(descent + gap + tick + extra);
    } else {
        out.ascent = out.ascent.max(ascent + gap + tick + extra);
    }
    out
}

/// Guards against a zero rule thickness, which would draw nothing.
fn nonzero(value: f32, font: &crate::font::MathFont<'_>, size: f32) -> f32 {
    if value > 0.0 {
        value
    } else {
        font.default_rule_thickness(size)
    }
}
