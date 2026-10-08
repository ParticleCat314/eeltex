//! Big operators: display sizing and limit placement.

use crate::ast::{Accent, Alphabet, BigOpKind, Expr, Limits};
use crate::class::AtomClass;
use crate::error::Error;
use crate::style::MathStyle;

use super::{Ctx, Frag, expr_frag, glyph_frag, script, stack};

pub(crate) fn layout(
    ctx: &mut Ctx<'_, '_>,
    kind: BigOpKind,
    lower: Option<&Expr>,
    upper: Option<&Expr>,
    limits: Limits,
    style: MathStyle,
    alphabet: Alphabet,
) -> Result<Frag, Error> {
    let mut op = operator(ctx, kind, style, alphabet)?;
    let lower = lower
        .map(|e| expr_frag(ctx, e, style.subscript(), alphabet))
        .transpose()?;
    let upper = upper
        .map(|e| expr_frag(ctx, e, style.superscript(), alphabet))
        .transpose()?;

    let use_limits = match limits {
        Limits::Always => true,
        Limits::Never => false,
        Limits::Auto => !kind.scripts_by_default() && style.is_display(),
    };

    if !use_limits {
        // TeXbook rule 13a: with a subscript, the italic correction comes back out of the advance.
        if lower.is_some() {
            op.width -= op.italic;
        }
        let mut frag = script::attach(ctx, op, lower, upper, style);
        frag.class = AtomClass::Op;
        return Ok(frag);
    }

    // Half the italic correction leans the upper limit right and the lower left.
    let lean = op.italic / 2.0;
    Ok(stack::limits(
        ctx,
        op,
        upper,
        lower,
        AtomClass::Op,
        lean,
        style,
    ))
}

fn operator(
    ctx: &mut Ctx<'_, '_>,
    kind: BigOpKind,
    style: MathStyle,
    alphabet: Alphabet,
) -> Result<Frag, Error> {
    let size = ctx.size(style);
    let Some(ch) = kind.char() else {
        let (name, accent) = match kind {
            BigOpKind::Limit(name) => (name, None),
            BigOpKind::VarLimInf => ("lim", Some(Accent::Underline)),
            BigOpKind::VarLimSup => ("lim", Some(Accent::Overline)),
            BigOpKind::VarInjLim => ("lim", Some(Accent::UnderRightArrow)),
            BigOpKind::VarProjLim => ("lim", Some(Accent::UnderLeftArrow)),
            _ => ("lim", None),
        };
        let word = match accent {
            Some(kind) => Expr::Accent {
                kind,
                base: Box::new(Expr::Func(name)),
            },
            None => Expr::Func(name),
        };
        let mut frag = expr_frag(ctx, &word, style, alphabet)?;
        frag.class = AtomClass::Op;
        return Ok(frag);
    };

    let fonts = ctx.fonts;
    let font = &fonts.math;
    let Some(glyph) = font.glyph_for(ch) else {
        return super::sym(ctx, ch, AtomClass::Op, Alphabet::Upright, style);
    };

    let glyph = if style.is_display() {
        display_variant(ctx, glyph, size)
    } else {
        glyph
    };
    let mut frag = glyph_frag(font, &mut ctx.items, glyph, AtomClass::Op, size);
    let centre = (frag.ascent - frag.descent) / 2.0;
    let shift = font.axis_height(size) - centre;
    frag.translate(&mut ctx.items, 0.0, -shift);
    frag.ascent += shift;
    frag.descent -= shift;
    Ok(frag)
}

/// The smallest vertical variant that clears `displayOperatorMinHeight`.
fn display_variant(
    ctx: &Ctx<'_, '_>,
    glyph: ttf_parser::GlyphId,
    size: f32,
) -> ttf_parser::GlyphId {
    let fonts = ctx.fonts;
    let font = &fonts.math;
    let minimum = font.px(
        f32::from(font.constants().display_operator_min_height()),
        size,
    );
    let Some(variants) = font.variants() else {
        return glyph;
    };
    let Some(construction) = variants.vertical_constructions.get(glyph) else {
        return glyph;
    };
    super::delim::scan_variants(font, &construction, glyph, minimum, size).0
}
