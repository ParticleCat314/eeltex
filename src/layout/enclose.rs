//! Boxes drawn around or across things, and boxes measured differently from their ink:
//! `\boxed`, `\colorbox`, `\cancel`, `\rule`, `\smash`, `\rlap`, `\raisebox`, `\hline`.

use crate::ast::{Adjust, Alphabet, Enclose, Expr, Lap};
use crate::attrs::NodeId;
use crate::class::AtomClass;
use crate::error::Error;
use crate::scene::{Rule, Shape};
use crate::style::MathStyle;

use super::{Ctx, Frag, Spacer, expr_frag};

/// `\fboxsep`: the gap between a frame and what it holds, in ems.
const FRAME_PAD_EM: f32 = 0.3;

/// `\fboxrule`, in ems.
const FRAME_RULE_EM: f32 = 0.04;

/// How far a cancelling stroke reaches past the body, top and bottom, in ems.
const CANCEL_PAD_EM: f32 = 0.1;

/// The length of each barb of `\cancelto`'s arrowhead, in ems.
const ARROWHEAD_EM: f32 = 0.25;

/// `\rule[raise]{width}{height}`.
pub(crate) fn rule(
    ctx: &mut Ctx<'_, '_>,
    width: f32,
    height: f32,
    raise: f32,
    style: MathStyle,
) -> Frag {
    let size = ctx.size(style);
    let (width, height, raise) = (width * size, height * size, raise * size);
    let mut frag = Frag::empty(AtomClass::Ord);
    if width > 0.0 && height > 0.0 {
        frag.add_rule(
            &mut ctx.items,
            Rule::new(0.0, -(raise + height), width, height),
        );
    } else {
        // Nothing to see, as in `\strut`; a spacer still says where it is.
        frag.spacers = ctx.items.push_spacer(Spacer {
            x: 0.0,
            y: 0.0,
            width: width.max(0.0),
            node: NodeId::NONE,
        });
    }
    frag.width = width.max(0.0);
    frag.ascent = (raise + height).max(0.0);
    frag.descent = (-raise).max(0.0);
    frag
}

/// `\hline`: no room of its own; the environment draws the line.
pub(crate) fn hline(ctx: &mut Ctx<'_, '_>) -> Frag {
    let mut frag = Frag::empty(AtomClass::Ord);
    frag.spacers = ctx.items.push_spacer(Spacer {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        node: NodeId::NONE,
    });
    frag
}

pub(crate) fn adjust(
    ctx: &mut Ctx<'_, '_>,
    adjust: Adjust,
    body: &Expr,
    style: MathStyle,
    alphabet: Alphabet,
) -> Result<Frag, Error> {
    let mut frag = expr_frag(ctx, body, style, alphabet)?;
    match adjust {
        Adjust::Smash { top, bottom } => {
            if top {
                frag.ascent = 0.0;
            }
            if bottom {
                frag.descent = 0.0;
            }
        }
        Adjust::Lap(lap) => {
            let shift = match lap {
                Lap::Left => -frag.width,
                Lap::Right => 0.0,
                Lap::Centre => -frag.width / 2.0,
            };
            frag.translate(&mut ctx.items, shift, 0.0);
            frag.width = 0.0;
            frag.italic = 0.0;
        }
        Adjust::Raise(em) => {
            let shift = em * ctx.size(style);
            frag.translate(&mut ctx.items, 0.0, -shift);
            frag.ascent = (frag.ascent + shift).max(0.0);
            frag.descent = (frag.descent - shift).max(0.0);
        }
    }
    frag.single = None;
    Ok(frag)
}

pub(crate) fn enclose(
    ctx: &mut Ctx<'_, '_>,
    kind: Enclose,
    body: &Expr,
    style: MathStyle,
    alphabet: Alphabet,
) -> Result<Frag, Error> {
    let body = expr_frag(ctx, body, style, alphabet)?;
    let size = ctx.size(style);
    Ok(match kind {
        Enclose::Frame => framed(ctx, body, size, None, None),
        Enclose::Fill(fill) => framed(ctx, body, size, None, Some(fill)),
        Enclose::FramedFill { frame, fill } => {
            framed(ctx, body, size, Some(Some(frame)), Some(fill))
        }
        Enclose::Cancel | Enclose::BackCancel | Enclose::CrossCancel | Enclose::CancelTo => {
            cancel(ctx, body, kind, size)
        }
        Enclose::StrikeOut => strike_out(ctx, body, size),
    })
}

/// A padded box with a fill behind, a frame around, or both.
/// `frame` is `None` for no frame, `Some(None)` for one in the ink, `Some(Some(_))` for a colour.
fn framed(
    ctx: &mut Ctx<'_, '_>,
    body: Frag,
    size: f32,
    frame: Option<Option<crate::attrs::Paint>>,
    fill: Option<crate::attrs::Paint>,
) -> Frag {
    // `\boxed` and `\fbox` frame in the ink; `\colorbox` has no frame.
    let frame = match (frame, fill) {
        (None, None) => Some(None),
        (frame, _) => frame,
    };
    let pad = FRAME_PAD_EM * size;
    let line = if frame.is_some() {
        FRAME_RULE_EM * size
    } else {
        0.0
    };
    let inset = pad + line;
    let (width, ascent, descent) = (
        body.width + 2.0 * inset,
        body.ascent + inset,
        body.descent + inset,
    );

    let mut out = Frag::empty(AtomClass::Ord);
    if let Some(fill) = fill {
        paint_background(
            ctx,
            &mut out,
            Rule::new(0.0, -ascent, width, ascent + descent),
            fill,
        );
    }
    out.place(&mut ctx.items, body, inset, 0.0);
    if let Some(colour) = frame {
        let height = ascent + descent;
        for edge in [
            Rule::new(0.0, -ascent, width, line),
            Rule::new(0.0, descent - line, width, line),
            Rule::new(0.0, -ascent, line, height),
            Rule::new(width - line, -ascent, line, height),
        ] {
            let edge = with_paint(ctx, edge, colour);
            out.add_rule(&mut ctx.items, edge);
        }
    }
    out.width = width;
    out.ascent = out.ascent.max(ascent);
    out.descent = out.descent.max(descent);
    out
}

/// A background fill, which needs a colour to be anything but a blot over the body.
#[allow(unused_variables)]
fn paint_background(ctx: &mut Ctx<'_, '_>, out: &mut Frag, rule: Rule, fill: crate::attrs::Paint) {
    #[cfg(feature = "attrs")]
    {
        let rule = with_paint(ctx, rule, Some(fill)).with_shape(Shape::Background);
        out.add_rule(&mut ctx.items, rule);
    }
}

#[allow(unused_variables, unused_mut)]
fn with_paint(ctx: &mut Ctx<'_, '_>, mut rule: Rule, paint: Option<crate::attrs::Paint>) -> Rule {
    #[cfg(feature = "attrs")]
    if let Some(paint) = paint {
        rule.attrs.paint = ctx.intern_paint(paint);
    }
    rule
}

/// A line from `(x0, y0)` to `(x1, y1)`, y-down, as a rule whose diagonal it is.
fn segment(x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32) -> Rule {
    let rising = (x1 - x0) * (y1 - y0) < 0.0;
    Rule::new(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs()).with_shape(if rising {
        Shape::Rising(thickness)
    } else {
        Shape::Falling(thickness)
    })
}

fn cancel(ctx: &mut Ctx<'_, '_>, body: Frag, kind: Enclose, size: f32) -> Frag {
    let thickness = ctx.fonts.math.default_rule_thickness(size);
    let pad = CANCEL_PAD_EM * size;
    let (width, top, bottom) = (body.width, -(body.ascent + pad), body.descent + pad);
    let mut out = Frag::empty(body.class);
    out.place(&mut ctx.items, body, 0.0, 0.0);

    let rising = matches!(
        kind,
        Enclose::Cancel | Enclose::CrossCancel | Enclose::CancelTo
    );
    let falling = matches!(kind, Enclose::BackCancel | Enclose::CrossCancel);
    if rising {
        out.add_rule(&mut ctx.items, segment(0.0, bottom, width, top, thickness));
    }
    if falling {
        out.add_rule(&mut ctx.items, segment(0.0, top, width, bottom, thickness));
    }
    if kind == Enclose::CancelTo {
        // Two barbs back from the tip, either side of the shaft.
        let (dx, dy) = (width, top - bottom);
        let length = (dx * dx + dy * dy).sqrt();
        if length > 0.0 {
            let (ux, uy) = (dx / length, dy / length);
            let barb = ARROWHEAD_EM * size;
            let (sin, cos) = 0.45f32.sin_cos();
            for side in [1.0f32, -1.0] {
                let (bx, by) = (ux * cos - uy * sin * side, uy * cos + ux * sin * side);
                let rule = segment(width - bx * barb, top - by * barb, width, top, thickness);
                out.add_rule(&mut ctx.items, rule);
            }
        }
    }
    out.width = width;
    out
}

fn strike_out(ctx: &mut Ctx<'_, '_>, body: Frag, size: f32) -> Frag {
    let font = &ctx.fonts.math;
    let thickness = font.default_rule_thickness(size);
    let axis = font.axis_height(size);
    let width = body.width;
    let mut out = Frag::empty(body.class);
    out.place(&mut ctx.items, body, 0.0, 0.0);
    out.add_rule(
        &mut ctx.items,
        Rule::new(0.0, -axis - thickness / 2.0, width, thickness),
    );
    out.width = width;
    out
}
