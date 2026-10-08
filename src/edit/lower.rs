//! Conversion between the edit tree and [`Expr`]: [`raise`] reads one in, [`lower`] writes one out.

use crate::Alphabet;
use crate::ast::{Decor, Expr};
use crate::attrs::{Paint, Span};
use crate::class::AtomClass;
use crate::edit::tree::{EditId, IdGen, Item, ItemKind, Slot};

/// The character drawn where a slot is empty: U+25A1, real ink so the slot can be clicked into.
pub const PLACEHOLDER: char = '\u{25A1}';

pub const PLACEHOLDER_PAINT: Paint = Paint::rgba(0x80, 0x80, 0x80, 0x80);

/// The colour a typed `\name` the parser does not know is kept in, as red text.
pub const UNKNOWN_PAINT: Paint = Paint::rgb(0xd0, 0x20, 0x20);

#[must_use]
pub fn raise(expr: &Expr, ids: &mut IdGen) -> Slot {
    let id = ids.fresh();
    let mut items = Vec::new();
    push_items(expr, ids, &mut items);
    Slot::of(id, items)
}

fn push_items(expr: &Expr, ids: &mut IdGen, out: &mut Vec<Item>) {
    match expr {
        Expr::Seq(items) => {
            for item in items {
                push_items(item, ids, out);
            }
        }
        Expr::Marked { inner, .. } => push_items(inner, ids, out),
        other => {
            if let Some(item) = raise_item(other, ids) {
                out.push(item);
            }
        }
    }
}

/// `None` for a `Seq` or `Marked`, which are not items.
fn raise_item(expr: &Expr, ids: &mut IdGen) -> Option<Item> {
    let id = ids.fresh();
    let kind = match expr {
        Expr::Seq(_) | Expr::Marked { .. } => return None,
        Expr::Sym {
            ch,
            class,
            alphabet,
        } => ItemKind::Sym {
            ch: *ch,
            class: *class,
            alphabet: *alphabet,
        },
        Expr::Text(text) => ItemKind::Text(text.clone()),
        Expr::Func(name) => ItemKind::Func(name),
        Expr::Space(mu) => ItemKind::Space(*mu),
        Expr::Group(inner) => ItemKind::Group(raise(inner, ids)),
        Expr::Frac {
            num,
            den,
            style,
            bar,
        } => ItemKind::Frac {
            num: raise(num, ids),
            den: raise(den, ids),
            style: *style,
            bar: *bar,
        },
        Expr::Sqrt { degree, radicand } => ItemKind::Sqrt {
            degree: degree.as_ref().map(|d| raise(d, ids)),
            radicand: raise(radicand, ids),
        },
        Expr::Scripts { base, sub, sup } => ItemKind::Scripts {
            base: raise(base, ids),
            sub: sub.as_ref().map(|s| raise(s, ids)),
            sup: sup.as_ref().map(|s| raise(s, ids)),
        },
        Expr::BigOp {
            kind,
            lower,
            upper,
            limits,
        } => ItemKind::BigOp {
            kind: *kind,
            lower: lower.as_ref().map(|l| raise(l, ids)),
            upper: upper.as_ref().map(|u| raise(u, ids)),
            limits: *limits,
        },
        Expr::Delimited { left, body, right } => ItemKind::Delimited {
            left: *left,
            body: raise(body, ids),
            right: *right,
        },
        Expr::Alphabet(alphabet, inner) => ItemKind::Alphabet(*alphabet, raise(inner, ids)),
        Expr::Class(class, inner) => ItemKind::Class(*class, raise(inner, ids)),
        Expr::Styled { decor, body } => ItemKind::Styled {
            decor: *decor,
            body: raise(body, ids),
        },
        Expr::Phantom {
            body,
            width,
            height,
        } => ItemKind::Phantom {
            body: raise(body, ids),
            width: *width,
            height: *height,
        },
        Expr::Accent { kind, base } => ItemKind::Accent {
            kind: *kind,
            base: raise(base, ids),
        },
        Expr::Stack {
            over,
            base,
            under,
            class,
        } => ItemKind::Stack {
            over: over.as_ref().map(|o| raise(o, ids)),
            base: raise(base, ids),
            under: under.as_ref().map(|u| raise(u, ids)),
            class: *class,
        },
        Expr::XArrow { ch, over, under } => ItemKind::XArrow {
            ch: *ch,
            over: over.as_ref().map(|o| raise(o, ids)),
            under: under.as_ref().map(|u| raise(u, ids)),
        },
        Expr::Env(kind, rows) => ItemKind::Env {
            kind: *kind,
            rows: rows
                .iter()
                .map(|row| row.iter().map(|cell| raise(cell, ids)).collect())
                .collect(),
        },
        Expr::Style { style, body } => ItemKind::Style {
            style: *style,
            body: raise(body, ids),
        },
        Expr::SizedDelim { ch, class, size } => ItemKind::SizedDelim {
            ch: *ch,
            class: *class,
            size: *size,
        },
        Expr::Middle(delim) => ItemKind::Middle(*delim),
        Expr::Rule {
            width,
            height,
            raise,
        } => ItemKind::Rule {
            width: *width,
            height: *height,
            raise: *raise,
        },
        Expr::Adjust { adjust, body } => ItemKind::Adjust {
            adjust: *adjust,
            body: raise(body, ids),
        },
        Expr::Enclose { kind, body } => ItemKind::Enclose {
            kind: *kind,
            body: raise(body, ids),
        },
        Expr::HLine { dashed } => ItemKind::HLine { dashed: *dashed },
    };
    Some(Item::new(id, kind))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Lowering {
    /// Wrap every slot and item in an [`Expr::Marked`] carrying its [`EditId`].
    pub ids: bool,
    /// Draw [`PLACEHOLDER`] where a slot is empty.
    pub placeholders: bool,
}

impl Lowering {
    /// For layout: identities, and a visible box in every empty slot.
    pub const SCENE: Lowering = Lowering {
        ids: true,
        placeholders: true,
    };
    /// For [`crate::to_tex`]: plain LaTeX, with an empty slot left empty.
    pub const TEX: Lowering = Lowering {
        ids: false,
        placeholders: false,
    };
}

#[must_use]
pub fn lower(slot: &Slot, how: Lowering) -> Expr {
    if slot.is_empty() && how.placeholders {
        return mark(slot.id, placeholder(), how.ids);
    }
    let items = slot
        .items
        .iter()
        .map(|item| mark(item.id, lower_item(item, how), how.ids))
        .collect();
    // A `Seq` even for one item, or the slot's and the item's `Marked` collapse into one node.
    mark(slot.id, Expr::Seq(items), how.ids)
}

fn mark(id: EditId, expr: Expr, ids: bool) -> Expr {
    if !ids {
        return expr;
    }
    Expr::Marked {
        // One byte long, so `Scene::node_at_offset(id)` finds exactly this node.
        span: Span::new(id.0, id.0.saturating_add(1)),
        inner: Box::new(expr),
    }
}

fn placeholder() -> Expr {
    Expr::Styled {
        decor: Decor::colour(PLACEHOLDER_PAINT),
        body: Box::new(Expr::Sym {
            ch: PLACEHOLDER,
            class: AtomClass::Ord,
            alphabet: Alphabet::Upright,
        }),
    }
}

/// One item as an expression, with no wrapper of its own.
pub(crate) fn lower_item(item: &Item, how: Lowering) -> Expr {
    match &item.kind {
        ItemKind::Sym {
            ch,
            class,
            alphabet,
        } => Expr::Sym {
            ch: *ch,
            class: *class,
            alphabet: *alphabet,
        },
        ItemKind::Text(text) => Expr::Text(text.clone()),
        ItemKind::Func(name) => Expr::Func(name),
        ItemKind::Space(mu) => Expr::Space(*mu),
        ItemKind::Group(inner) => Expr::Group(Box::new(lower(inner, how))),
        ItemKind::Frac {
            num,
            den,
            style,
            bar,
        } => Expr::Frac {
            num: Box::new(lower(num, how)),
            den: Box::new(lower(den, how)),
            style: *style,
            bar: *bar,
        },
        ItemKind::Sqrt { degree, radicand } => Expr::Sqrt {
            degree: degree.as_ref().map(|d| Box::new(lower(d, how))),
            radicand: Box::new(lower(radicand, how)),
        },
        ItemKind::Scripts { base, sub, sup } => Expr::Scripts {
            base: Box::new(lower(base, how)),
            sub: sub.as_ref().map(|s| Box::new(lower(s, how))),
            sup: sup.as_ref().map(|s| Box::new(lower(s, how))),
        },
        ItemKind::BigOp {
            kind,
            lower: low,
            upper,
            limits,
        } => Expr::BigOp {
            kind: *kind,
            lower: low.as_ref().map(|l| Box::new(lower(l, how))),
            upper: upper.as_ref().map(|u| Box::new(lower(u, how))),
            limits: *limits,
        },
        ItemKind::Delimited { left, body, right } => Expr::Delimited {
            left: *left,
            body: Box::new(lower(body, how)),
            right: *right,
        },
        ItemKind::Alphabet(alphabet, inner) => {
            Expr::Alphabet(*alphabet, Box::new(lower(inner, how)))
        }
        ItemKind::Class(class, inner) => Expr::Class(*class, Box::new(lower(inner, how))),
        ItemKind::Styled { decor, body } => Expr::Styled {
            decor: *decor,
            body: Box::new(lower(body, how)),
        },
        ItemKind::Phantom {
            body,
            width,
            height,
        } => Expr::Phantom {
            body: Box::new(lower(body, how)),
            width: *width,
            height: *height,
        },
        ItemKind::Accent { kind, base } => Expr::Accent {
            kind: *kind,
            base: Box::new(lower(base, how)),
        },
        ItemKind::Stack {
            over,
            base,
            under,
            class,
        } => Expr::Stack {
            over: over.as_ref().map(|o| Box::new(lower(o, how))),
            base: Box::new(lower(base, how)),
            under: under.as_ref().map(|u| Box::new(lower(u, how))),
            class: *class,
        },
        ItemKind::XArrow { ch, over, under } => Expr::XArrow {
            ch: *ch,
            over: over.as_ref().map(|o| Box::new(lower(o, how))),
            under: under.as_ref().map(|u| Box::new(lower(u, how))),
        },
        ItemKind::Env { kind, rows } => Expr::Env(
            *kind,
            rows.iter()
                .map(|row| row.iter().map(|cell| lower(cell, how)).collect())
                .collect(),
        ),
        ItemKind::Style { style, body } => Expr::Style {
            style: *style,
            body: Box::new(lower(body, how)),
        },
        ItemKind::SizedDelim { ch, class, size } => Expr::SizedDelim {
            ch: *ch,
            class: *class,
            size: *size,
        },
        ItemKind::Middle(delim) => Expr::Middle(*delim),
        ItemKind::Rule {
            width,
            height,
            raise,
        } => Expr::Rule {
            width: *width,
            height: *height,
            raise: *raise,
        },
        ItemKind::Adjust { adjust, body } => Expr::Adjust {
            adjust: *adjust,
            body: Box::new(lower(body, how)),
        },
        ItemKind::Enclose { kind, body } => Expr::Enclose {
            kind: *kind,
            body: Box::new(lower(body, how)),
        },
        ItemKind::HLine { dashed } => Expr::HLine { dashed: *dashed },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::tree::SlotName;

    fn round(src: &str) -> Slot {
        let mut ids = IdGen::default();
        let expr = crate::parse(src).expect("parses");
        raise(&expr, &mut ids)
    }

    #[test]
    fn raising_and_lowering_is_the_identity_on_the_expression() {
        for src in [
            r"\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}",
            r"\sum_{n=1}^{\infty} \frac{1}{n^2}",
            r"\begin{pmatrix} 1 & 2 \\ 3 & 4 \end{pmatrix}",
            r"\left( \frac{a}{b} \right)_y",
            r"\binom{n}{k}",
            r"\underbrace{x+y}_{n}",
            r"\text{ if } x",
            "x",
        ] {
            let expr = crate::parse(src).expect("parses");
            let slot = round(src);
            assert_eq!(
                crate::to_tex(&lower(&slot, Lowering::TEX)),
                crate::to_tex(&expr),
                "{src}"
            );
            let again =
                crate::parse(&crate::to_tex(&lower(&slot, Lowering::TEX))).expect("re-parses");
            assert_eq!(expr, again, "{src}");
        }
    }

    #[test]
    fn a_run_dissolves_into_the_slot() {
        let slot = round("abc");
        assert_eq!(slot.len(), 3);
        assert!(
            slot.items
                .iter()
                .all(|i| matches!(i.kind, ItemKind::Sym { .. }))
        );
    }

    #[test]
    fn ids_are_unique_across_the_tree() {
        let slot = round(r"\frac{a+b}{c^2}");
        let mut seen = Vec::new();
        collect(&slot, &mut seen);
        let mut sorted = seen.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), seen.len(), "an id was used twice");
        assert!(seen.iter().all(|id| id.is_some()));
    }

    fn collect(slot: &Slot, out: &mut Vec<EditId>) {
        out.push(slot.id);
        for item in &slot.items {
            out.push(item.id);
            for name in item.slots_in_order() {
                if let Some(child) = item.slot(name) {
                    collect(child, out);
                }
            }
        }
    }

    #[test]
    fn an_empty_slot_lowers_to_visible_ink() {
        let slot = Slot::empty(EditId(7));
        let expr = lower(&slot, Lowering::SCENE);
        let Expr::Marked { span, inner } = &expr else {
            panic!("the slot lost its id: {expr:?}")
        };
        assert_eq!(*span, Span::new(7, 8));
        assert!(matches!(**inner, Expr::Styled { .. }));
        // `\Box` is the table's first spelling of U+25A1; `\square` is its alias.
        assert!(
            crate::to_tex(&expr).contains(r"\Box"),
            "{}",
            crate::to_tex(&expr)
        );
    }

    #[test]
    fn a_slot_and_its_only_item_keep_separate_ids() {
        let mut ids = IdGen::default();
        let slot = raise(&crate::parse("x").expect("parses"), &mut ids);
        let item_id = slot.items.first().expect("one item").id;
        let expr = lower(&slot, Lowering::SCENE);
        let Expr::Marked { span, inner } = &expr else {
            panic!("no slot span")
        };
        assert_eq!(span.start, slot.id.0);
        let Expr::Seq(items) = &**inner else {
            panic!("the items were not wrapped in a Seq: {inner:?}")
        };
        let Some(Expr::Marked { span, .. }) = items.first() else {
            panic!("no item span")
        };
        assert_eq!(span.start, item_id.0);
    }

    #[test]
    fn an_empty_fraction_has_two_enterable_slots() {
        let slot = round(r"\frac{}{}");
        let item = slot.items.first().expect("one item");
        assert_eq!(item.slots_in_order(), vec![SlotName::Num, SlotName::Den]);
        for name in item.slots_in_order() {
            assert!(item.slot(name).is_some_and(Slot::is_empty));
        }
    }
}
