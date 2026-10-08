//! The editable tree: [`Expr`](crate::Expr) with every child a [`Slot`] and an [`EditId`] on all.

use crate::ast::{
    Accent, Adjust, Alphabet, BigOpKind, Decor, Delim, DelimSize, Enclose, EnvKind, FracStyle,
    Limits, StyleSwitch,
};
use crate::class::AtomClass;

/// Stable identity for one slot or item. Zero is reserved, as a zero span means unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditId(pub u32);

impl EditId {
    /// The id nothing has.
    pub const NONE: EditId = EditId(0);

    #[must_use]
    pub fn is_some(self) -> bool {
        self.0 != 0
    }
}

/// A source of fresh [`EditId`]s that never reuses one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdGen {
    next: u32,
}

impl Default for IdGen {
    fn default() -> IdGen {
        IdGen { next: 1 }
    }
}

impl IdGen {
    #[must_use]
    pub fn fresh(&mut self) -> EditId {
        let id = EditId(self.next);
        // Saturating: a wrapped, duplicate id would silently merge two sub-expressions.
        self.next = self.next.saturating_add(1);
        id
    }
}

/// Maximum nesting depth, under the parser's so [`Editor::tex`](super::Editor::tex) re-parses.
pub const MAX_DEPTH: usize = 16;

/// A list of items the caret can sit between.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub id: EditId,
    pub items: Vec<Item>,
}

impl Slot {
    #[must_use]
    pub fn empty(id: EditId) -> Slot {
        Slot::of(id, Vec::new())
    }

    #[must_use]
    pub fn of(id: EditId, items: Vec<Item>) -> Slot {
        Slot { id, items }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// How many slots deep the nesting goes below this one.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.items.iter().map(Item::depth).max().unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: EditId,
    pub kind: ItemKind,
}

impl Item {
    #[must_use]
    pub fn new(id: EditId, kind: ItemKind) -> Item {
        Item { id, kind }
    }
}

/// What an item is: [`Expr`](crate::Expr) variant for variant, with every child a [`Slot`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ItemKind {
    /// One maths character.
    Sym {
        ch: char,
        class: AtomClass,
        alphabet: Alphabet,
    },
    /// `\text{…}`. A leaf: prose is edited as a string, not as a slot of atoms.
    Text(String),
    /// `\sin` and the other upright names.
    Func(&'static str),
    /// Explicit spacing, in mu.
    Space(i16),
    /// A braced group, kept because it forces [`AtomClass::Ord`].
    Group(Slot),
    Frac {
        num: Slot,
        den: Slot,
        style: FracStyle,
        /// Clear for `\binom`.
        bar: bool,
    },
    Sqrt {
        degree: Option<Slot>,
        radicand: Slot,
    },
    Scripts {
        base: Slot,
        sub: Option<Slot>,
        sup: Option<Slot>,
    },
    BigOp {
        kind: BigOpKind,
        lower: Option<Slot>,
        upper: Option<Slot>,
        limits: Limits,
    },
    Delimited {
        left: Delim,
        body: Slot,
        right: Delim,
    },
    Alphabet(Alphabet, Slot),
    Class(AtomClass, Slot),
    Styled {
        decor: Decor,
        body: Slot,
    },
    Phantom {
        body: Slot,
        width: bool,
        height: bool,
    },
    Accent {
        kind: Accent,
        base: Slot,
    },
    Stack {
        over: Option<Slot>,
        base: Slot,
        under: Option<Slot>,
        class: Option<AtomClass>,
    },
    XArrow {
        ch: char,
        over: Option<Slot>,
        under: Option<Slot>,
    },
    Env {
        kind: EnvKind,
        rows: Vec<Vec<Slot>>,
    },
    /// `\textstyle` and friends, holding the rest of the run it switched.
    Style {
        style: StyleSwitch,
        body: Slot,
    },
    /// A `\big`-family delimiter. A leaf, like a symbol.
    SizedDelim {
        ch: char,
        class: AtomClass,
        size: DelimSize,
    },
    /// `\middle|`. A leaf.
    Middle(Delim),
    /// `\rule`, in ems. A leaf.
    Rule {
        width: f32,
        height: f32,
        raise: f32,
    },
    /// `\smash`, `\rlap`, `\raisebox`.
    Adjust {
        adjust: Adjust,
        body: Slot,
    },
    /// `\boxed`, `\colorbox`, `\cancel`.
    Enclose {
        kind: Enclose,
        body: Slot,
    },
    /// `\hline`. A leaf.
    HLine {
        dashed: bool,
    },
}

/// Which child slot of an item a step descends into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SlotName {
    /// The only slot of a wrapper: group, delimited body, alphabet, phantom, colour.
    Body,
    Num,
    Den,
    Degree,
    Radicand,
    /// What scripts, accents and stacks hang off.
    Base,
    Sub,
    Sup,
    Lower,
    Upper,
    Over,
    Under,
    Cell {
        row: u16,
        col: u16,
    },
}

impl SlotName {
    /// Whether this slot sets its contents on a baseline of its own, as a numerator or script does.
    #[must_use]
    pub fn breaks_baseline(self) -> bool {
        match self {
            SlotName::Body | SlotName::Base | SlotName::Radicand => false,
            SlotName::Num
            | SlotName::Den
            | SlotName::Degree
            | SlotName::Sub
            | SlotName::Sup
            | SlotName::Lower
            | SlotName::Upper
            | SlotName::Over
            | SlotName::Under
            | SlotName::Cell { .. } => true,
        }
    }
}

impl Item {
    /// The named child slot, if this item has one.
    #[must_use]
    pub fn slot(&self, name: SlotName) -> Option<&Slot> {
        use ItemKind as K;
        Some(match (&self.kind, name) {
            (
                K::Group(s)
                | K::Alphabet(_, s)
                | K::Class(_, s)
                | K::Styled { body: s, .. }
                | K::Phantom { body: s, .. }
                | K::Style { body: s, .. }
                | K::Adjust { body: s, .. }
                | K::Enclose { body: s, .. }
                | K::Delimited { body: s, .. },
                SlotName::Body,
            ) => s,
            (K::Frac { num, .. }, SlotName::Num) => num,
            (K::Frac { den, .. }, SlotName::Den) => den,
            (K::Sqrt { radicand, .. }, SlotName::Radicand) => radicand,
            (K::Sqrt { degree, .. }, SlotName::Degree) => degree.as_ref()?,
            (
                K::Scripts { base, .. } | K::Accent { base, .. } | K::Stack { base, .. },
                SlotName::Base,
            ) => base,
            (K::Scripts { sub, .. }, SlotName::Sub) => sub.as_ref()?,
            (K::Scripts { sup, .. }, SlotName::Sup) => sup.as_ref()?,
            (K::BigOp { lower, .. }, SlotName::Lower) => lower.as_ref()?,
            (K::BigOp { upper, .. }, SlotName::Upper) => upper.as_ref()?,
            (K::Stack { over, .. }, SlotName::Over) => over.as_ref()?,
            (K::Stack { under, .. }, SlotName::Under) => under.as_ref()?,
            (K::XArrow { over, .. }, SlotName::Over) => over.as_ref()?,
            (K::XArrow { under, .. }, SlotName::Under) => under.as_ref()?,
            (K::Env { rows, .. }, SlotName::Cell { row, col }) => {
                rows.get(usize::from(row))?.get(usize::from(col))?
            }
            _ => return None,
        })
    }

    pub fn slot_mut(&mut self, name: SlotName) -> Option<&mut Slot> {
        use ItemKind as K;
        Some(match (&mut self.kind, name) {
            (
                K::Group(s)
                | K::Alphabet(_, s)
                | K::Class(_, s)
                | K::Styled { body: s, .. }
                | K::Phantom { body: s, .. }
                | K::Style { body: s, .. }
                | K::Adjust { body: s, .. }
                | K::Enclose { body: s, .. }
                | K::Delimited { body: s, .. },
                SlotName::Body,
            ) => s,
            (K::Frac { num, .. }, SlotName::Num) => num,
            (K::Frac { den, .. }, SlotName::Den) => den,
            (K::Sqrt { radicand, .. }, SlotName::Radicand) => radicand,
            (K::Sqrt { degree, .. }, SlotName::Degree) => degree.as_mut()?,
            (
                K::Scripts { base, .. } | K::Accent { base, .. } | K::Stack { base, .. },
                SlotName::Base,
            ) => base,
            (K::Scripts { sub, .. }, SlotName::Sub) => sub.as_mut()?,
            (K::Scripts { sup, .. }, SlotName::Sup) => sup.as_mut()?,
            (K::BigOp { lower, .. }, SlotName::Lower) => lower.as_mut()?,
            (K::BigOp { upper, .. }, SlotName::Upper) => upper.as_mut()?,
            (K::Stack { over, .. }, SlotName::Over) => over.as_mut()?,
            (K::Stack { under, .. }, SlotName::Under) => under.as_mut()?,
            (K::XArrow { over, .. }, SlotName::Over) => over.as_mut()?,
            (K::XArrow { under, .. }, SlotName::Under) => under.as_mut()?,
            (K::Env { rows, .. }, SlotName::Cell { row, col }) => {
                rows.get_mut(usize::from(row))?.get_mut(usize::from(col))?
            }
            _ => return None,
        })
    }

    /// This item's slots in caret order: reading order, but sub before sup and lower before upper.
    #[must_use]
    pub fn slots_in_order(&self) -> Vec<SlotName> {
        use ItemKind as K;
        let mut out = Vec::new();
        let mut push_if = |name: SlotName, present: bool| {
            if present {
                out.push(name);
            }
        };
        match &self.kind {
            K::Sym { .. }
            | K::Text(_)
            | K::Func(_)
            | K::Space(_)
            | K::SizedDelim { .. }
            | K::Middle(_)
            | K::Rule { .. }
            | K::HLine { .. } => {}
            K::Group(_)
            | K::Alphabet(..)
            | K::Class(..)
            | K::Styled { .. }
            | K::Phantom { .. }
            | K::Style { .. }
            | K::Adjust { .. }
            | K::Enclose { .. }
            | K::Delimited { .. } => push_if(SlotName::Body, true),
            K::Frac { .. } => {
                push_if(SlotName::Num, true);
                push_if(SlotName::Den, true);
            }
            K::Sqrt { degree, .. } => {
                push_if(SlotName::Degree, degree.is_some());
                push_if(SlotName::Radicand, true);
            }
            K::Scripts { sub, sup, .. } => {
                push_if(SlotName::Base, true);
                push_if(SlotName::Sub, sub.is_some());
                push_if(SlotName::Sup, sup.is_some());
            }
            K::BigOp { lower, upper, .. } => {
                push_if(SlotName::Lower, lower.is_some());
                push_if(SlotName::Upper, upper.is_some());
            }
            K::Accent { .. } => push_if(SlotName::Base, true),
            K::Stack { over, under, .. } => {
                push_if(SlotName::Over, over.is_some());
                push_if(SlotName::Base, true);
                push_if(SlotName::Under, under.is_some());
            }
            K::XArrow { over, under, .. } => {
                push_if(SlotName::Over, over.is_some());
                push_if(SlotName::Under, under.is_some());
            }
            K::Env { rows, .. } => {
                for (row, cells) in rows.iter().enumerate() {
                    for (col, _) in cells.iter().enumerate() {
                        // A `u16` name cannot address a row or column past 65535.
                        let (Ok(row), Ok(col)) = (u16::try_from(row), u16::try_from(col)) else {
                            continue;
                        };
                        out.push(SlotName::Cell { row, col });
                    }
                }
            }
        }
        out
    }

    /// Whether the caret can go inside this item.
    #[must_use]
    pub fn is_composite(&self) -> bool {
        !self.slots_in_order().is_empty()
    }

    /// How many slots deep this item nests. Zero for a leaf.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.slots_in_order()
            .into_iter()
            .filter_map(|name| self.slot(name))
            .map(|slot| 1 + slot.depth())
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_start_at_one_and_never_repeat() {
        let mut id = IdGen::default();
        let first = id.fresh();
        assert_eq!(first, EditId(1));
        assert!(first.is_some());
        assert!(!EditId::NONE.is_some());
        let mut seen = vec![first];
        for _ in 0..100 {
            let next = id.fresh();
            assert!(!seen.contains(&next));
            seen.push(next);
        }
    }

    #[test]
    fn an_empty_slot_is_not_an_absent_one() {
        let mut id = IdGen::default();
        let base = Slot::empty(id.fresh());
        let absent = Item::new(
            id.fresh(),
            ItemKind::Scripts {
                base: base.clone(),
                sub: None,
                sup: None,
            },
        );
        let empty = Item::new(
            id.fresh(),
            ItemKind::Scripts {
                base,
                sub: None,
                sup: Some(Slot::empty(id.fresh())),
            },
        );
        assert_eq!(absent.slots_in_order(), vec![SlotName::Base]);
        assert_eq!(empty.slots_in_order(), vec![SlotName::Base, SlotName::Sup]);
        assert!(absent.slot(SlotName::Sup).is_none());
        assert!(empty.slot(SlotName::Sup).is_some_and(Slot::is_empty));
    }

    #[test]
    fn every_named_slot_resolves() {
        let mut id = IdGen::default();
        let slot = |g: &mut IdGen| Slot::empty(g.fresh());
        let kinds = vec![
            ItemKind::Frac {
                num: slot(&mut id),
                den: slot(&mut id),
                style: FracStyle::Auto,
                bar: true,
            },
            ItemKind::Sqrt {
                degree: Some(slot(&mut id)),
                radicand: slot(&mut id),
            },
            ItemKind::Scripts {
                base: slot(&mut id),
                sub: Some(slot(&mut id)),
                sup: Some(slot(&mut id)),
            },
            ItemKind::BigOp {
                kind: BigOpKind::Sum,
                lower: Some(slot(&mut id)),
                upper: Some(slot(&mut id)),
                limits: Limits::Auto,
            },
            ItemKind::Stack {
                over: Some(slot(&mut id)),
                base: slot(&mut id),
                under: Some(slot(&mut id)),
                class: None,
            },
            ItemKind::XArrow {
                ch: '\u{2192}',
                over: Some(slot(&mut id)),
                under: Some(slot(&mut id)),
            },
            ItemKind::Env {
                kind: EnvKind::Matrix { delims: None },
                rows: vec![vec![slot(&mut id), slot(&mut id)], vec![slot(&mut id)]],
            },
            ItemKind::Group(slot(&mut id)),
            ItemKind::Delimited {
                left: Delim::Char('('),
                body: slot(&mut id),
                right: Delim::Char(')'),
            },
            ItemKind::Accent {
                kind: Accent::Hat,
                base: slot(&mut id),
            },
        ];
        for kind in kinds {
            let mut item = Item::new(id.fresh(), kind);
            let names = item.slots_in_order();
            assert!(!names.is_empty(), "{:?} reported no slots", item.kind);
            for name in names {
                assert!(
                    item.slot(name).is_some(),
                    "{name:?} of {:?} does not resolve",
                    item.kind
                );
                assert!(item.slot_mut(name).is_some(), "{name:?} is not mutable");
            }
        }
    }

    #[test]
    fn a_matrix_enumerates_its_cells_in_reading_order() {
        let mut id = IdGen::default();
        let slot = |g: &mut IdGen| Slot::empty(g.fresh());
        let item = Item::new(
            id.fresh(),
            ItemKind::Env {
                kind: EnvKind::Matrix { delims: None },
                rows: vec![
                    vec![slot(&mut id), slot(&mut id)],
                    vec![slot(&mut id), slot(&mut id)],
                ],
            },
        );
        assert_eq!(
            item.slots_in_order(),
            vec![
                SlotName::Cell { row: 0, col: 0 },
                SlotName::Cell { row: 0, col: 1 },
                SlotName::Cell { row: 1, col: 0 },
                SlotName::Cell { row: 1, col: 1 },
            ]
        );
    }
}
