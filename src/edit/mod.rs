//! Interactive editing: a formula tree that can be changed, and a caret.
//!
//! ```
//! # #[cfg(feature = "embed-math-font")] {
//! use eeltex::edit::{Command, Editor};
//!
//! let mut editor = Editor::new();
//! editor.apply(Command::Insert('x'));
//! editor.apply(Command::Insert('/'));
//! editor.apply(Command::Insert('2'));
//! assert_eq!(editor.tex(), r"\frac{x}2");
//! # }
//! ```

pub mod cursor;
pub mod geom;
pub mod lower;
pub mod ops;
pub mod tree;

pub use crate::edit::cursor::{Cursor, Motion, Step};
pub use crate::edit::geom::Caret;
pub use crate::edit::lower::{Lowering, PLACEHOLDER, PLACEHOLDER_PAINT, UNKNOWN_PAINT};
pub use crate::edit::ops::{Changed, Command, Selection};
pub use crate::edit::tree::{EditId, IdGen, Item, ItemKind, Slot, SlotName};

use crate::edit::lower::{lower, raise};
use crate::{Decor, Error, Expr, Fonts, Options, Scene};

/// Ids the half-typed command is drawn under: clear of [`IdGen`], and below `u32::MAX` so
/// the span `id..id + 1` is not empty.
const PENDING_ITEM: EditId = EditId(u32::MAX - 4);
const PENDING_SLOT: EditId = EditId(u32::MAX - 5);
const PENDING_TEXT: EditId = EditId(u32::MAX - 6);

fn pending_item(name: &str) -> Item {
    let text = Item::new(PENDING_TEXT, ItemKind::Text(format!("\\{name}")));
    Item::new(
        PENDING_ITEM,
        ItemKind::Styled {
            decor: Decor::colour(PLACEHOLDER_PAINT),
            body: Slot::of(PENDING_SLOT, vec![text]),
        },
    )
}

/// What the spans on an edited scene mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum SpanMode {
    /// Spans carry [`EditId`]s, which the caret needs.
    #[default]
    Ids,
    /// Spans carry byte ranges into [`Editor::tex`]; the caret does not work in this mode.
    Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct EditOptions {
    pub spans: SpanMode,
}

impl EditOptions {
    #[must_use]
    pub fn with_spans(mut self, spans: SpanMode) -> EditOptions {
        self.spans = spans;
        self
    }
}

#[derive(Debug, Clone)]
pub struct Editor {
    pub(crate) root: Slot,
    pub(crate) ids: IdGen,
    pub(crate) cursor: Cursor,
    /// The fixed end of a range selection, if one is being made.
    pub(crate) anchor: Option<Cursor>,
    /// A raw `\command` part-way through being typed.
    pub(crate) pending: Option<String>,
    /// The red command `pending` was reopened from, which Escape puts back.
    pub(crate) reopened: Option<String>,
    pub(crate) undo: Vec<(Slot, Cursor)>,
    pub(crate) redo: Vec<(Slot, Cursor)>,
    /// Whether the next keystroke joins the last undo entry.
    pub(crate) coalescing: bool,
    pub(crate) opts: EditOptions,
}

impl Default for Editor {
    fn default() -> Editor {
        Editor::new()
    }
}

impl Editor {
    /// An empty formula.
    #[must_use]
    pub fn new() -> Editor {
        Editor::with_options(EditOptions::default())
    }

    #[must_use]
    pub fn with_options(opts: EditOptions) -> Editor {
        let mut ids = IdGen::default();
        let root = Slot::empty(ids.fresh());
        Editor {
            root,
            ids,
            cursor: Cursor::start(),
            anchor: None,
            pending: None,
            reopened: None,
            undo: Vec::new(),
            redo: Vec::new(),
            coalescing: false,
            opts,
        }
    }

    /// Read a formula in, with the caret at its end.
    ///
    /// # Errors
    ///
    /// Whatever [`crate::parse`] reports.
    pub fn from_tex(src: &str) -> Result<Editor, Error> {
        let mut editor = Editor::new();
        editor.set_tex(src)?;
        Ok(editor)
    }

    /// Replace the formula, discarding the caret and the undo history.
    ///
    /// # Errors
    ///
    /// As [`Editor::from_tex`].
    pub fn set_tex(&mut self, src: &str) -> Result<(), Error> {
        let expr = crate::parse(src)?;
        self.root = raise(&expr, &mut self.ids);
        self.cursor = Cursor::at(Vec::new(), self.root.len());
        self.anchor = None;
        self.pending = None;
        self.reopened = None;
        self.undo.clear();
        self.redo.clear();
        self.coalescing = false;
        Ok(())
    }

    /// The formula as LaTeX, with an empty slot written `{}`.
    #[must_use]
    pub fn tex(&self) -> String {
        crate::to_tex(&lower(&self.root, Lowering::TEX))
    }

    /// The formula as an expression, ready for [`crate::layout`].
    #[must_use]
    pub fn expr(&self) -> Expr {
        match self.opts.spans {
            SpanMode::Ids => lower(&self.drawn().0, Lowering::SCENE),
            SpanMode::Bytes => crate::parse_with(
                &self.tex(),
                &crate::ParseOptions::default().with_spans(true),
            )
            .unwrap_or_else(|_| lower(&self.root, Lowering::SCENE)),
        }
    }

    /// The tree as drawn, with any half-typed `\name` added, and the caret's place in it.
    pub(crate) fn drawn(&self) -> (Slot, Cursor) {
        let mut root = self.root.clone();
        let mut cursor = self.cursor.clone();
        let slot = cursor::resolve_mut(&mut root, &cursor.path);
        if let Some((name, slot)) = self.pending.as_deref().zip(slot) {
            let at = cursor.index.min(slot.items.len());
            slot.items.insert(at, pending_item(name));
            cursor.index = at + 1;
        }
        (root, cursor)
    }

    /// # Errors
    ///
    /// Whatever [`crate::layout`] reports.
    pub fn layout(&self, fonts: &Fonts<'_>, opts: &Options) -> Result<Scene, Error> {
        crate::layout(&self.expr(), fonts, opts)
    }

    #[must_use]
    pub fn ids_in_spans(&self) -> bool {
        self.opts.spans == SpanMode::Ids
    }

    #[must_use]
    pub fn root(&self) -> &Slot {
        &self.root
    }

    #[must_use]
    pub fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    /// Put the caret somewhere, trimming the path if it no longer resolves. A half-typed
    /// `\command` is abandoned; landing inside a red unknown command reopens it.
    pub fn set_cursor(&mut self, cursor: Cursor) {
        self.abandon_pending();
        self.cursor = cursor::clamp(&self.root, &cursor);
        self.anchor = None;
        self.reopen_if_inside();
    }

    /// Move the caret, keeping the selection anchor (or anchoring at the old position).
    pub fn extend_to(&mut self, cursor: Cursor) {
        self.abandon_pending();
        if self.anchor.is_none() {
            self.anchor = Some(self.cursor.clone());
        }
        self.cursor = cursor::clamp(&self.root, &cursor);
        self.step_out_of_unknown(true);
    }

    pub fn clear_selection(&mut self) {
        self.anchor = None;
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
