//! Rows and columns: `matrix`, `aligned`, `gather`, `cases`, `array`, and the rules across them.

use crate::ast::{Alphabet, ColumnAlign, Delim, EnvKind, Expr};
use crate::class::{AtomClass, spacing_mu};
use crate::error::Error;
#[cfg(feature = "formatting")]
use crate::scene::Rule;
use crate::style::MathStyle;

use super::delim::delim_frag;
use super::{Ctx, Frag, expr_frag, mu_to_px};

/// Floor on row leading, in ems; `mathLeading` is zero in many fonts.
const ROW_LEADING_MIN_EM: f32 = 0.25;

/// Row leading of a `\substack`, in ems of its script size.
const SUBSTACK_LEADING_EM: f32 = 0.12;

const COLUMN_GAP_MU: f32 = 12.0;

/// `\arraycolsep`: the padding either side of an `array` column, in ems.
const ARRAY_COLSEP_EM: f32 = 0.5;

/// An `array` row's strut: 0.7 and 0.3 of a 1.2em baseline skip.
const ARRAY_STRUT_ASCENT_EM: f32 = 0.84;
const ARRAY_STRUT_DESCENT_EM: f32 = 0.36;

#[cfg(feature = "formatting")]
/// `\doublerulesep`: the gap between two `\hline`s in a row, in ems.
const DOUBLE_RULE_SEP_EM: f32 = 0.2;

/// A dashed rule's dash and gap, in ems.
#[cfg(feature = "formatting")]
const DASH_EM: f32 = 0.3;
#[cfg(feature = "formatting")]
const DASH_GAP_EM: f32 = 0.2;

pub(crate) fn layout(
    ctx: &mut Ctx<'_, '_>,
    kind: EnvKind,
    rows: &[Vec<Expr>],
    style: MathStyle,
    alphabet: Alphabet,
) -> Result<Frag, Error> {
    let cell_style = match kind {
        EnvKind::Matrix { .. }
        | EnvKind::AlignedMatrix { .. }
        | EnvKind::Cases
        | EnvKind::RCases => MathStyle::Text,
        EnvKind::Array(spec) if spec.display => MathStyle::Display,
        EnvKind::Array(_) => MathStyle::Text,
        EnvKind::DCases | EnvKind::DRCases => MathStyle::Display,
        EnvKind::SubStack | EnvKind::SubArray(_) => MathStyle::Script,
        // A new environment kind lays out like `aligned` until it says otherwise.
        _ => style,
    };

    let mut cells: Vec<Vec<Frag>> = Vec::with_capacity(rows.len());
    for row in rows {
        let mut laid = Vec::with_capacity(row.len());
        for cell in row {
            laid.push(expr_frag(ctx, cell, cell_style, alphabet)?);
        }
        cells.push(laid);
    }

    let size = ctx.size(style);
    let columns = cells.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return Ok(Frag::empty(AtomClass::Inner));
    }

    let mut column_width = vec![0.0f32; columns];
    for row in &cells {
        for (i, cell) in row.iter().enumerate() {
            if let Some(w) = column_width.get_mut(i) {
                *w = w.max(cell.width);
            }
        }
    }

    // Rows holding nothing but `\hline`s take no room: they only carry the rules.
    let rule_only: Vec<bool> = rows.iter().map(|row| is_rule_only(row)).collect();
    let is_array = matches!(kind, EnvKind::Array(_));
    let (strut_ascent, strut_descent) = if is_array {
        (ARRAY_STRUT_ASCENT_EM * size, ARRAY_STRUT_DESCENT_EM * size)
    } else {
        (0.0, 0.0)
    };
    let row_ascent: Vec<f32> = cells
        .iter()
        .zip(&rule_only)
        .map(|(r, only)| match only {
            true => 0.0,
            false => r.iter().map(|c| c.ascent).fold(strut_ascent, f32::max),
        })
        .collect();
    let row_descent: Vec<f32> = cells
        .iter()
        .zip(&rule_only)
        .map(|(r, only)| match only {
            true => 0.0,
            false => r.iter().map(|c| c.descent).fold(strut_descent, f32::max),
        })
        .collect();

    let fonts = ctx.fonts;
    let font = &fonts.math;
    let leading = if matches!(kind, EnvKind::SubStack | EnvKind::SubArray(_)) {
        SUBSTACK_LEADING_EM * size
    } else if is_array {
        // The struts already space the rows.
        0.0
    } else {
        font.k(font.constants().math_leading(), size)
            .max(ROW_LEADING_MIN_EM * size)
    };
    // Baselines, top to bottom, relative to the first row's baseline.
    let mut baselines = Vec::with_capacity(cells.len());
    let mut y = 0.0f32;
    for i in 0..cells.len() {
        if i > 0 {
            let gap = if rule_only.get(i).copied().unwrap_or(false) {
                0.0
            } else {
                leading
            };
            y += row_descent.get(i - 1).copied().unwrap_or(0.0)
                + gap
                + row_ascent.get(i).copied().unwrap_or(0.0);
        }
        baselines.push(y);
    }

    let gaps = column_gaps(kind, rows, columns, size, style);
    let rule_width = font.default_rule_thickness(size);
    let mut column_x = Vec::with_capacity(columns);
    // Where each vertical rule goes, and whether it is dashed.
    let mut vertical: Vec<(f32, bool)> = Vec::new();
    let mut x = 0.0f32;
    let total_width = if let EnvKind::Array(spec) = kind {
        let pad = ARRAY_COLSEP_EM * size;
        for (i, width) in column_width.iter().enumerate() {
            if let Some(dashed) = spec.rule(i) {
                vertical.push((x, dashed));
                x += rule_width;
            }
            x += pad;
            column_x.push(x);
            x += width + pad;
        }
        if let Some(dashed) = spec.rule(columns) {
            vertical.push((x, dashed));
            x += rule_width;
        }
        x
    } else {
        for (i, width) in column_width.iter().enumerate() {
            column_x.push(x);
            x += width + gaps.get(i).copied().unwrap_or(0.0);
        }
        (x - gaps.last().copied().unwrap_or(0.0)).max(0.0)
    };

    let mut body = Frag::empty(AtomClass::Inner);
    for (r, row) in cells.into_iter().enumerate() {
        let baseline = baselines.get(r).copied().unwrap_or(0.0);
        for (c, cell) in row.into_iter().enumerate() {
            let column = column_x.get(c).copied().unwrap_or(0.0);
            let width = column_width.get(c).copied().unwrap_or(0.0);
            let dx = column + align_offset(kind, c, width, cell.width);
            body.place(&mut ctx.items, cell, dx, baseline);
        }
    }
    body.width = total_width;

    #[cfg(feature = "formatting")]
    {
        // y-down tops and bottoms of each row.
        let top_of = |r: usize| {
            baselines.get(r).copied().unwrap_or(0.0) - row_ascent.get(r).copied().unwrap_or(0.0)
        };
        let bottom_of = |r: usize| {
            baselines.get(r).copied().unwrap_or(0.0) + row_descent.get(r).copied().unwrap_or(0.0)
        };
        let table_top = top_of(0);
        let table_bottom = (0..rows.len())
            .filter(|r| !rule_only.get(*r).copied().unwrap_or(false))
            .map(bottom_of)
            .fold(table_top, f32::max);

        for (r, row) in rows.iter().enumerate() {
            let lines = row.first().map(leading_hlines).unwrap_or_default();
            if lines.is_empty() {
                continue;
            }
            // Midway between this row and the one above, or the edge of the table.
            let boundary = if r == 0 {
                table_top
            } else if rule_only.get(r).copied().unwrap_or(false) {
                bottom_of(r - 1)
            } else {
                (bottom_of(r - 1) + top_of(r)) / 2.0
            };
            let step = rule_width + DOUBLE_RULE_SEP_EM * size;
            let first = boundary - step * (lines.len() - 1) as f32 / 2.0 - rule_width / 2.0;
            for (i, dashed) in lines.into_iter().enumerate() {
                let top = first + step * i as f32;
                for (from, length) in dashes(total_width, dashed, size) {
                    body.add_rule(&mut ctx.items, Rule::new(from, top, length, rule_width));
                }
            }
        }
        for (left, dashed) in vertical {
            for (from, length) in dashes(table_bottom - table_top, dashed, size) {
                body.add_rule(
                    &mut ctx.items,
                    Rule::new(left, table_top + from, rule_width, length),
                );
            }
        }
    }

    body.centre_on(&mut ctx.items, font.axis_height(size));

    match delimiters(kind) {
        None => Ok(body),
        Some((left, right)) => wrap(ctx, body, left, right, size),
    }
}

#[cfg(feature = "formatting")]
/// The `\hline`s at the front of a row's first cell; `true` for a dashed one.
fn leading_hlines(cell: &Expr) -> Vec<bool> {
    let mut out = Vec::new();
    collect_hlines(cell, &mut out);
    out
}

/// Whether to keep looking past `expr` for more rules.
fn collect_hlines(expr: &Expr, out: &mut Vec<bool>) -> bool {
    match expr {
        Expr::HLine { dashed } => {
            out.push(*dashed);
            true
        }
        Expr::Marked { inner, .. } => collect_hlines(inner, out),
        Expr::Seq(items) => items.iter().all(|item| collect_hlines(item, out)),
        _ => false,
    }
}

/// Whether a row is only `\hline`s: a trailing `\\ \hline`.
fn is_rule_only(row: &[Expr]) -> bool {
    let Some((first, rest)) = row.split_first() else {
        return false;
    };
    let mut lines = Vec::new();
    collect_hlines(first, &mut lines) && !lines.is_empty() && rest.iter().all(Expr::is_blank)
}

#[cfg(feature = "formatting")]
/// The pieces of a rule `length` long: the whole of it, or dashes.
fn dashes(length: f32, dashed: bool, size: f32) -> Vec<(f32, f32)> {
    if !dashed || length <= 0.0 {
        return vec![(0.0, length.max(0.0))];
    }
    let (dash, gap) = (DASH_EM * size, DASH_GAP_EM * size);
    let mut out = Vec::new();
    let mut at = 0.0f32;
    while at < length {
        out.push((at, dash.min(length - at)));
        at += dash + gap;
    }
    out
}

/// The gap after each column: the widest any row asks for.
fn column_gaps(
    kind: EnvKind,
    rows: &[Vec<Expr>],
    columns: usize,
    size: f32,
    style: MathStyle,
) -> Vec<f32> {
    if !matches!(kind, EnvKind::Aligned) {
        return vec![mu_to_px(COLUMN_GAP_MU, size); columns];
    }
    let mut gaps = vec![0.0f32; columns];
    for row in rows {
        for i in 0..columns.saturating_sub(1) {
            let (Some(left), Some(right)) = (row.get(i), row.get(i + 1)) else {
                continue;
            };
            let mu = spacing_mu(
                left.trailing_class(),
                right.leading_class(),
                style.is_script(),
            );
            if let Some(gap) = gaps.get_mut(i) {
                *gap = gap.max(mu_to_px(mu, size));
            }
        }
    }
    gaps
}

fn align_offset(kind: EnvKind, column: usize, column_width: f32, cell_width: f32) -> f32 {
    let align = match kind {
        // Odd columns right, even columns left.
        EnvKind::Aligned => {
            if column % 2 == 0 {
                ColumnAlign::Right
            } else {
                ColumnAlign::Left
            }
        }
        EnvKind::Cases | EnvKind::DCases | EnvKind::RCases | EnvKind::DRCases => ColumnAlign::Left,
        EnvKind::Array(spec) => spec.align(column),
        EnvKind::AlignedMatrix { align, .. } | EnvKind::SubArray(align) => align,
        _ => ColumnAlign::Centre,
    };
    match align {
        ColumnAlign::Left => 0.0,
        ColumnAlign::Centre => (column_width - cell_width) / 2.0,
        ColumnAlign::Right => column_width - cell_width,
    }
}

fn delimiters(kind: EnvKind) -> Option<(char, char)> {
    match kind {
        EnvKind::Matrix { delims } | EnvKind::AlignedMatrix { delims, .. } => delims,
        EnvKind::Cases | EnvKind::DCases => Some(('{', '\0')),
        EnvKind::RCases | EnvKind::DRCases => Some(('\0', '}')),
        _ => None,
    }
}

fn wrap(
    ctx: &mut Ctx<'_, '_>,
    body: Frag,
    left: char,
    right: char,
    size: f32,
) -> Result<Frag, Error> {
    let axis = ctx.fonts.math.axis_height(size);
    let target = Some(2.0 * (body.ascent - axis).max(body.descent + axis));
    // `\0` means no delimiter on that side.
    let delim = |ch| {
        if ch == '\0' {
            Delim::None
        } else {
            Delim::Char(ch)
        }
    };
    let left = delim_frag(ctx, delim(left), target, size, AtomClass::Open)?;
    let right = delim_frag(ctx, delim(right), target, size, AtomClass::Close)?;

    let mut out = Frag::empty(AtomClass::Inner);
    let mut x = 0.0f32;
    for frag in [left, body, right] {
        let width = frag.width;
        out.place(&mut ctx.items, frag, x, 0.0);
        x += width;
    }
    out.width = x;
    Ok(out)
}
