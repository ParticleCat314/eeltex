//! Sub- and superscripts.

use crate::ast::{Alphabet, Expr};
use crate::error::Error;
use crate::style::MathStyle;

use super::{Ctx, Frag, expr_frag};

pub(crate) fn layout(
    ctx: &mut Ctx<'_, '_>,
    base: &Expr,
    sub: Option<&Expr>,
    sup: Option<&Expr>,
    style: MathStyle,
    alphabet: Alphabet,
) -> Result<Frag, Error> {
    let base = expr_frag(ctx, base, style, alphabet)?;
    let sub = sub
        .map(|e| expr_frag(ctx, e, style.subscript(), alphabet))
        .transpose()?;
    let sup = sup
        .map(|e| expr_frag(ctx, e, style.superscript(), alphabet))
        .transpose()?;
    Ok(attach(ctx, base, sub, sup, style))
}

pub(crate) fn attach(
    ctx: &mut Ctx<'_, '_>,
    base: Frag,
    sub: Option<Frag>,
    sup: Option<Frag>,
    style: MathStyle,
) -> Frag {
    if sub.is_none() && sup.is_none() {
        return base;
    }

    let fonts = ctx.fonts;
    let font = &fonts.math;
    let c = font.constants();
    // Constants are read at the size of the base, not of the script.
    let size = ctx.size(style);

    // y-up, as the MATH spec writes it; converted at the `place` calls.
    let mut shift_up = if style.is_cramped() {
        font.k(c.superscript_shift_up_cramped(), size)
    } else {
        font.k(c.superscript_shift_up(), size)
    };
    let mut shift_down = font.k(c.subscript_shift_down(), size);

    if let Some(sup) = &sup {
        shift_up = shift_up
            .max(base.ascent - font.k(c.superscript_baseline_drop_max(), size))
            .max(font.k(c.superscript_bottom_min(), size) + sup.descent);
    }
    if let Some(sub) = &sub {
        shift_down = shift_down
            .max(base.descent + font.k(c.subscript_baseline_drop_min(), size))
            .max(sub.ascent - font.k(c.subscript_top_max(), size));
    }

    if let (Some(sub), Some(sup)) = (&sub, &sup) {
        let gap = (shift_up - sup.descent) - (sub.ascent - shift_down);
        let needed = font.k(c.sub_superscript_gap_min(), size) - gap;
        if needed > 0.0 {
            // The superscript rises only as far as the font allows; the subscript takes the rest.
            let headroom = (font.k(c.superscript_bottom_max_with_subscript(), size)
                - (shift_up - sup.descent))
                .max(0.0);
            let raise = needed.min(headroom);
            shift_up += raise;
            shift_down += needed - raise;
        }
    }

    let base_width = base.width;
    // TeX: the italic correction of the base shifts the superscript only.
    let italic = base.italic;
    let sup_width = sup.as_ref().map_or(0.0, |f| f.width);
    let sub_width = sub.as_ref().map_or(0.0, |f| f.width);

    // An accent over `x_a` belongs over the `x`, not over the whole box.
    let accent_attach = base
        .accent_attach
        .unwrap_or(base_width / 2.0 + italic / 2.0);

    let mut out = Frag::empty(base.class);
    out.place(&mut ctx.items, base, 0.0, 0.0);
    if let Some(sup) = sup {
        out.place(&mut ctx.items, sup, base_width + italic, -shift_up);
    }
    if let Some(sub) = sub {
        out.place(&mut ctx.items, sub, base_width, shift_down);
    }

    out.width =
        base_width + (italic + sup_width).max(sub_width) + font.k(c.space_after_script(), size);
    out.italic = 0.0;
    out.single = None;
    out.accent_attach = Some(accent_attach);
    out
}
