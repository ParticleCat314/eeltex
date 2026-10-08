//! Radicals: a stretched surd with a rule over a cramped radicand.

use crate::ast::{Alphabet, Expr};
use crate::class::AtomClass;
use crate::error::Error;
use crate::scene::Rule;
use crate::style::MathStyle;

use super::delim::stretch_vertical;
use super::{Ctx, Frag, expr_frag};

const SURD: char = '\u{221A}';

pub(crate) fn layout(
    ctx: &mut Ctx<'_, '_>,
    degree: Option<&Expr>,
    radicand: &Expr,
    style: MathStyle,
    alphabet: Alphabet,
) -> Result<Frag, Error> {
    let rad = expr_frag(ctx, radicand, style.cramped(), alphabet)?;
    // TeX sets the index in scriptscript whatever the enclosing style.
    let degree = degree
        .map(|e| expr_frag(ctx, e, MathStyle::ScriptScript, alphabet))
        .transpose()?;

    let fonts = ctx.fonts;
    let font = &fonts.math;
    let c = font.constants();
    let size = ctx.size(style);
    let thickness = font.default_rule_thickness(size);
    let gap = font.k(
        if style.is_display() {
            c.radical_display_style_vertical_gap()
        } else {
            c.radical_vertical_gap()
        },
        size,
    );
    let extra = font.k(c.radical_extra_ascender(), size);

    let target = rad.height() + gap + thickness;
    let Some(surd_glyph) = font.glyph_for(SURD) else {
        return Ok(bare(&mut ctx.items, rad, thickness, gap, extra));
    };
    let surd = stretch_vertical(
        font,
        &mut ctx.items,
        surd_glyph,
        target,
        size,
        AtomClass::Ord,
    );

    // TeX `make_radical`: half the excess surd height becomes clearance above the radicand.
    let gap = gap + (surd.height() - target).max(0.0) / 2.0;

    // y-up, with the radicand's baseline at zero.
    let rule_bottom = rad.ascent + gap;
    let rule_top = rule_bottom + thickness;
    let surd_raise = rule_top - surd.ascent;

    let mut out = Frag::empty(AtomClass::Ord);
    let mut x = 0.0;

    if let Some(degree) = degree {
        let raise_percent = f32::from(c.radical_degree_bottom_raise_percent()) / 100.0;
        let surd_ink_bottom = surd_raise - surd.descent;
        let degree_bottom = surd_ink_bottom + raise_percent * surd.height();
        let before = font.k(c.radical_kern_before_degree(), size);
        // Usually negative: the index tucks back under the surd.
        let after = font.k(c.radical_kern_after_degree(), size);
        let degree_width = degree.width;
        let degree_descent = degree.descent;
        out.place(
            &mut ctx.items,
            degree,
            before,
            -(degree_bottom + degree_descent),
        );
        x = (before + degree_width + after).max(0.0);
    }

    let surd_width = surd.width;
    out.place(&mut ctx.items, surd, x, -surd_raise);
    x += surd_width;

    let rad_width = rad.width;
    out.place(&mut ctx.items, rad, x, 0.0);
    // `Rule::y` is the top edge, y-down.
    out.add_rule(
        &mut ctx.items,
        Rule::new(x, -rule_top, rad_width, thickness),
    );

    out.width = x + rad_width;
    out.ascent = out.ascent.max(rule_top + extra);
    out.single = None;
    Ok(out)
}

/// A radical drawn with a rule but no surd, for a font that has no U+221A.
fn bare(buf: &mut super::ItemBuf, rad: Frag, thickness: f32, gap: f32, extra: f32) -> Frag {
    let rule_top = rad.ascent + gap + thickness;
    let width = rad.width;
    let mut out = Frag::empty(AtomClass::Ord);
    out.place(buf, rad, 0.0, 0.0);
    out.add_rule(buf, Rule::new(0.0, -rule_top, width, thickness));
    out.width = width;
    out.ascent = out.ascent.max(rule_top + extra);
    out
}
