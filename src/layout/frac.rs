//! Fractions, including barless ones (`\binom`).

use crate::ast::{Alphabet, Expr, FracStyle};
use crate::class::AtomClass;
use crate::error::Error;
use crate::scene::Rule;
use crate::style::MathStyle;

use super::{Ctx, Frag, expr_frag};

pub(crate) fn layout(
    ctx: &mut Ctx<'_, '_>,
    num: &Expr,
    den: &Expr,
    frac_style: FracStyle,
    bar: bool,
    style: MathStyle,
    alphabet: Alphabet,
) -> Result<Frag, Error> {
    let style = match (frac_style, style.is_cramped()) {
        (FracStyle::Auto, _) => style,
        (FracStyle::Display | FracStyle::Continued, true) => MathStyle::DisplayCramped,
        (FracStyle::Display | FracStyle::Continued, false) => MathStyle::Display,
        (FracStyle::Text, true) => MathStyle::TextCramped,
        (FracStyle::Text, false) => MathStyle::Text,
    };

    let mut num = expr_frag(ctx, num, style.numerator(), alphabet)?;
    if frac_style == FracStyle::Continued {
        // amsmath's `\strut`: 0.7 and 0.3 of a 1.2em baseline skip.
        let size = ctx.size(style.numerator());
        num.ascent = num.ascent.max(0.84 * size);
        num.descent = num.descent.max(0.36 * size);
    }
    let den = expr_frag(ctx, den, style.denominator(), alphabet)?;

    let fonts = ctx.fonts;
    let font = &fonts.math;
    let c = font.constants();
    let size = ctx.size(style);
    let display = style.is_display();
    let k = |in_display, in_text| font.k(if display { in_display } else { in_text }, size);
    let axis = font.axis_height(size);
    // A barless fraction (`\binom`) uses the `stack*` constants.
    let thickness = if bar {
        font.fraction_rule_thickness(size)
    } else {
        0.0
    };

    // y-up throughout; converted once at the `place` calls below.
    let (mut shift_up, mut shift_down, gap_num_min, gap_den_min) = if thickness > 0.0 {
        (
            k(
                c.fraction_numerator_display_style_shift_up(),
                c.fraction_numerator_shift_up(),
            ),
            k(
                c.fraction_denominator_display_style_shift_down(),
                c.fraction_denominator_shift_down(),
            ),
            k(
                c.fraction_num_display_style_gap_min(),
                c.fraction_numerator_gap_min(),
            ),
            k(
                c.fraction_denom_display_style_gap_min(),
                c.fraction_denominator_gap_min(),
            ),
        )
    } else {
        let gap = k(c.stack_display_style_gap_min(), c.stack_gap_min());
        (
            k(c.stack_top_display_style_shift_up(), c.stack_top_shift_up()),
            k(
                c.stack_bottom_display_style_shift_down(),
                c.stack_bottom_shift_down(),
            ),
            gap / 2.0,
            gap / 2.0,
        )
    };

    let bar_top = axis + thickness / 2.0;
    let bar_bottom = axis - thickness / 2.0;

    let gap = (shift_up - num.descent) - bar_top;
    if gap < gap_num_min {
        shift_up += gap_num_min - gap;
    }
    let gap = bar_bottom - (den.ascent - shift_down);
    if gap < gap_den_min {
        shift_down += gap_den_min - gap;
    }

    let width = num.width.max(den.width);
    let num_dx = (width - num.width) / 2.0;
    let den_dx = (width - den.width) / 2.0;

    let mut out = Frag::empty(AtomClass::Inner);
    out.place(&mut ctx.items, num, num_dx, -shift_up);
    out.place(&mut ctx.items, den, den_dx, shift_down);
    if thickness > 0.0 {
        // `Rule::y` is the top edge, y-down.
        out.add_rule(&mut ctx.items, Rule::new(0.0, -bar_top, width, thickness));
    }
    out.width = width;
    out.single = None;
    Ok(out)
}
