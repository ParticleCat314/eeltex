//! One thing set over or under another: `\overset`, `\underset`, `\stackrel`, brace and arrow labels.

use crate::ast::{Alphabet, Expr};
use crate::class::AtomClass;
use crate::error::Error;
use crate::style::MathStyle;

use super::{Ctx, Frag, expr_frag};

pub(crate) fn layout(
    ctx: &mut Ctx<'_, '_>,
    over: Option<&Expr>,
    base: &Expr,
    under: Option<&Expr>,
    class: Option<AtomClass>,
    style: MathStyle,
    alphabet: Alphabet,
) -> Result<Frag, Error> {
    let base_style = if over.is_some() {
        style.cramped()
    } else {
        style
    };
    let base = expr_frag(ctx, base, base_style, alphabet)?;
    let over = over
        .map(|e| expr_frag(ctx, e, style.superscript(), alphabet))
        .transpose()?;
    let under = under
        .map(|e| expr_frag(ctx, e, style.subscript(), alphabet))
        .transpose()?;

    let class = class.unwrap_or(base.class);
    Ok(limits(ctx, base, over, under, class, 0.0, style))
}

/// Centres `over` and `under` on `base`; `lean` shifts `over` right and `under` left.
pub(super) fn limits(
    ctx: &mut Ctx<'_, '_>,
    base: Frag,
    over: Option<Frag>,
    under: Option<Frag>,
    class: AtomClass,
    lean: f32,
    style: MathStyle,
) -> Frag {
    let fonts = ctx.fonts;
    let font = &fonts.math;
    let c = font.constants();
    let size = ctx.size(style);

    let (base_width, ascent, descent) = (base.width, base.ascent, base.descent);
    let over_width = over.as_ref().map_or(0.0, |f| f.width);
    let under_width = under.as_ref().map_or(0.0, |f| f.width);
    let width = base_width
        .max(over_width + lean.abs())
        .max(under_width + lean.abs());

    let mut out = Frag::empty(class);
    out.place(&mut ctx.items, base, (width - base_width) / 2.0, 0.0);
    if let Some(over) = over {
        let baseline = (ascent + font.k(c.upper_limit_gap_min(), size) + over.descent)
            .max(font.k(c.upper_limit_baseline_rise_min(), size) + ascent);
        let x = (width - over_width) / 2.0 + lean;
        out.place(&mut ctx.items, over, x, -baseline);
    }
    if let Some(under) = under {
        let baseline = (descent + font.k(c.lower_limit_gap_min(), size) + under.ascent)
            .max(font.k(c.lower_limit_baseline_drop_min(), size) + descent);
        let x = (width - under_width) / 2.0 - lean;
        out.place(&mut ctx.items, under, x, baseline);
    }

    out.width = width;
    out.italic = 0.0;
    out.single = None;
    out
}

const XARROW_PAD_MU: f32 = 6.0;

/// `\xrightarrow[under]{over}`: an arrow stretched to fit its labels.
pub(crate) fn xarrow(
    ctx: &mut Ctx<'_, '_>,
    ch: char,
    over: Option<&Expr>,
    under: Option<&Expr>,
    style: MathStyle,
    alphabet: Alphabet,
) -> Result<Frag, Error> {
    // Labels are the buffer tail, so a bail-out can truncate their ink back out.
    let marks = ctx.items.marks();
    let over = over
        .map(|e| expr_frag(ctx, e, style.superscript(), alphabet))
        .transpose()?;
    let under = under
        .map(|e| expr_frag(ctx, e, style.subscript(), alphabet))
        .transpose()?;

    let size = ctx.size(style);
    let fonts = ctx.fonts;
    let font = &fonts.math;
    let Some(glyph) = font.glyph_for(ch) else {
        ctx.items.truncate(marks);
        return super::sym(ctx, ch, AtomClass::Rel, alphabet, style);
    };

    let label_width = over
        .as_ref()
        .map_or(0.0, |f| f.width)
        .max(under.as_ref().map_or(0.0, |f| f.width));
    let target = label_width + 2.0 * super::mu_to_px(XARROW_PAD_MU, size);
    let arrow = super::delim::stretch_horizontal(
        font,
        &mut ctx.items,
        glyph,
        target.max(font.px(font.advance(glyph), size)),
        size,
        AtomClass::Rel,
    );
    Ok(limits(ctx, arrow, over, under, AtomClass::Rel, 0.0, style))
}
