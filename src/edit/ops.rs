//! Edit operations: [`Editor::apply`] is the single entry point for every frontend.

use crate::Alphabet;
use crate::ast::Decor;
use crate::class::{AtomClass, class_of_char, math_char};
use crate::edit::Editor;
use crate::edit::cursor::{self, Cursor, Motion, Step};
use crate::edit::lower::{Lowering, UNKNOWN_PAINT, lower_item, raise};
use crate::edit::tree::{Item, ItemKind, MAX_DEPTH, Slot, SlotName};

/// What a frontend asks the editor to do.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Command {
    /// One typed character, interpreted structurally (`/`, `^`, `(` ...).
    Insert(char),
    /// Text inserted without interpretation: a paste or an IME commit.
    InsertText(String),
    /// A construct by name, without the backslash: `"frac"`, `"sqrt"`, `"sum"`.
    Named(String),
    Backspace,
    Delete,
    Move(Motion),
    /// Move, keeping the other end of the selection where it is: shift-arrow.
    Extend(Motion),
    SelectAll,
    /// Add a row to the enclosing environment; a no-op outside one.
    NewRow,
    /// Abandon a half-typed `\name`, or drop the selection.
    Escape,
    /// Submit a half-typed `\name`: Enter. A no-op when none is being typed.
    Submit,
    Undo,
    Redo,
}

/// What an [`Editor::apply`] changed, so a frontend knows what to redo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Changed {
    /// The formula changed; worth an undo entry.
    pub tree: bool,
    /// The scene needs laying out again (also true while a `\name` is being typed).
    pub relayout: bool,
    /// The caret moved, so it needs drawing again.
    pub cursor: bool,
}

impl Changed {
    pub(crate) const NOTHING: Changed = Changed {
        tree: false,
        relayout: false,
        cursor: false,
    };
    pub(crate) const MOVED: Changed = Changed {
        tree: false,
        relayout: false,
        cursor: true,
    };
    /// A `\name` grew or went away: nothing in the formula, something on screen.
    pub(crate) const TYPING: Changed = Changed {
        tree: false,
        relayout: true,
        cursor: true,
    };
    pub(crate) const EDITED: Changed = Changed {
        tree: true,
        relayout: true,
        cursor: true,
    };

    #[must_use]
    pub fn is_anything(self) -> bool {
        self.tree || self.relayout || self.cursor
    }

    fn merge(&mut self, other: Changed) {
        self.tree |= other.tree;
        self.relayout |= other.relayout;
        self.cursor |= other.cursor;
    }
}

/// A range of whole items within one slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub path: Vec<Step>,
    pub start: usize,
    pub end: usize,
}

impl Selection {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

impl Editor {
    /// Do one thing, and say what it changed.
    pub fn apply(&mut self, command: Command) -> Changed {
        match command {
            Command::Insert(ch) => self.insert_char(ch),
            Command::InsertText(text) => self.snapshotted(|e| e.insert_text(&text)),
            Command::Named(name) => self.snapshotted(|e| e.insert_named(&name)),
            Command::Backspace if self.pending.is_some() => self.unwind_pending(),
            Command::Backspace => self.snapshotted(Editor::backspace),
            Command::Delete => self.snapshotted(Editor::delete_forwards),
            // The caret stays put while a raw command is open: Enter or Escape closes it.
            Command::Move(_) | Command::Extend(_) if self.pending.is_some() => Changed::NOTHING,
            Command::Move(motion) => self.move_caret(motion, false),
            Command::Extend(motion) => self.move_caret(motion, true),
            Command::SelectAll => self.select_all(),
            Command::NewRow => self.snapshotted(Editor::add_row),
            Command::Escape => self.escape(),
            Command::Submit => self.submit(),
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
        }
    }

    /// Insert a run of characters, each with its structural meaning.
    pub fn insert_str(&mut self, text: &str) -> Changed {
        let mut changed = Changed::NOTHING;
        for ch in text.chars() {
            changed.merge(self.insert_char(ch));
        }
        changed
    }

    /// The selection as LaTeX, for a copy or a cut.
    #[must_use]
    pub fn selected_tex(&self) -> Option<String> {
        let range = self.selection()?;
        let slot = cursor::resolve(&self.root, &range.path)?;
        let items = slot
            .items
            .get(range.start..range.end.min(slot.items.len()))?;
        let run = Slot::of(self.root.id, items.to_vec());
        Some(crate::to_tex(&crate::edit::lower::lower(
            &run,
            Lowering::TEX,
        )))
    }

    /// Every typed `\name` the parser did not know, kept in the formula as red text.
    #[must_use]
    pub fn unknown_commands(&self) -> Vec<String> {
        fn walk(slot: &Slot, out: &mut Vec<String>) {
            for item in &slot.items {
                if let Some(name) = unknown_name(item) {
                    out.push(format!("\\{name}"));
                    continue;
                }
                for name in item.slots_in_order() {
                    if let Some(child) = item.slot(name) {
                        walk(child, out);
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, &mut out);
        out
    }

    /// The `\name` being typed, if one is.
    #[must_use]
    pub fn pending(&self) -> Option<&str> {
        self.pending.as_deref()
    }

    /// The selected range, if the anchor and the caret are apart.
    #[must_use]
    pub fn selection(&self) -> Option<Selection> {
        let anchor = self.anchor.as_ref()?;
        let common = self
            .cursor
            .path
            .iter()
            .zip(anchor.path.iter())
            .take_while(|(a, b)| a == b)
            .count();
        let (a_lo, a_hi) = endpoint(anchor, common);
        let (b_lo, b_hi) = endpoint(&self.cursor, common);
        let start = a_lo.min(b_lo);
        let end = a_hi.max(b_hi);
        if end <= start {
            return None;
        }
        let path = self.cursor.path.get(..common)?.to_vec();
        Some(Selection { path, start, end })
    }

    fn select_all(&mut self) -> Changed {
        let len = self.root.len();
        if len == 0 {
            return Changed::NOTHING;
        }
        self.anchor = Some(Cursor::at(Vec::new(), 0));
        self.cursor = Cursor::at(Vec::new(), len);
        Changed::MOVED
    }

    fn escape(&mut self) -> Changed {
        if self.pending.is_some() {
            self.anchor = None;
            return self.abandon_pending();
        }
        match self.anchor.take() {
            Some(_) => Changed::MOVED,
            None => Changed::NOTHING,
        }
    }

    /// Drop a raw command being typed; one reopened from red text goes back as it was.
    pub(crate) fn abandon_pending(&mut self) -> Changed {
        if self.pending.take().is_none() {
            return Changed::NOTHING;
        }
        match self.reopened.take() {
            Some(original) => {
                let item = self.unknown_item(&original);
                let mut changed = self.insert_item(item, false);
                changed.relayout = true;
                changed
            }
            None => Changed::TYPING,
        }
    }

    /// If the caret has landed inside a red unknown command, take it out and type it again.
    pub(crate) fn reopen_if_inside(&mut self) -> Changed {
        self.reopen_inside(true)
    }

    /// [`Editor::reopen_if_inside`]; `record` is false inside an edit that already took a snapshot.
    fn reopen_inside(&mut self, record: bool) -> Changed {
        let Some((depth, child, name)) = self.enclosing_unknown() else {
            return Changed::NOTHING;
        };
        let name = name.to_owned();
        if record {
            self.snapshot();
        }
        let parent: Vec<Step> = self.cursor.path.iter().take(depth).cloned().collect();
        let Some(slot) = cursor::resolve_mut(&mut self.root, &parent) else {
            return Changed::NOTHING;
        };
        splice(slot, child, 1, Vec::new());
        self.cursor = Cursor::at(parent, child);
        self.anchor = None;
        self.pending = Some(name.clone());
        self.reopened = Some(name);
        Changed::EDITED
    }

    /// Keep a selection's moving end out of red unknown commands: they are selected whole.
    pub(crate) fn step_out_of_unknown(&mut self, after: bool) {
        if let Some((depth, child, _)) = self.enclosing_unknown() {
            let parent: Vec<Step> = self.cursor.path.iter().take(depth).cloned().collect();
            self.cursor = Cursor::at(parent, child + usize::from(after));
        }
    }

    /// The outermost red unknown command around the caret: its depth, index and name.
    fn enclosing_unknown(&self) -> Option<(usize, usize, &str)> {
        self.cursor
            .path
            .iter()
            .enumerate()
            .find_map(|(depth, step)| {
                let parent = self.cursor.path.get(..depth)?;
                let item = cursor::item_at(&self.root, parent, step.child)?;
                Some((depth, step.child, unknown_name(item)?))
            })
    }

    fn move_caret(&mut self, motion: Motion, extend: bool) -> Changed {
        // Moving the caret ends an undo coalescing run.
        self.coalescing = false;
        if extend {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor.clone());
            }
        } else {
            self.anchor = None;
        }
        match cursor::moved(&self.root, &self.cursor, motion) {
            Some(next) => {
                self.cursor = next;
                if extend {
                    self.step_out_of_unknown(!matches!(
                        motion,
                        Motion::Left | Motion::WordLeft | Motion::Home
                    ));
                    return Changed::MOVED;
                }
                let mut changed = Changed::MOVED;
                changed.merge(self.reopen_if_inside());
                changed
            }
            // Dropping the selection is still a change.
            None if !extend => Changed::MOVED,
            None => Changed::NOTHING,
        }
    }

    fn insert_char(&mut self, ch: char) -> Changed {
        // A raw `\command` takes every key, braces and spaces included, until it is submitted.
        if let Some(raw) = self.pending.as_mut() {
            raw.push(ch);
            return Changed::TYPING;
        }

        if ch == '\\' {
            self.pending = Some(String::new());
            return Changed::TYPING;
        }

        // A structural keystroke is its own undo step.
        match ch {
            '/' => self.snapshotted(Editor::make_fraction),
            '^' => self.snapshotted(|e| e.open_script(SlotName::Sup)),
            '_' => self.snapshotted(|e| e.open_script(SlotName::Sub)),
            '&' => self.snapshotted(Editor::add_cell),
            ' ' => self.snapshotted(Editor::insert_space),
            '(' | '[' => self.snapshotted(|e| e.open_delimiter(ch)),
            '{' => self.snapshotted(Editor::open_group),
            // Snapshots itself, and only if it types a character.
            ')' | ']' | '}' => self.close_delimiter(ch),
            _ => {
                self.snapshot_typing();
                let mut changed = self.insert_symbol(ch);
                if ch.is_ascii_alphabetic() {
                    changed.merge(self.expand_alias());
                }
                changed
            }
        }
    }

    /// Enter on a raw `\command`: parse it, keeping it as red text if the parser refuses.
    fn submit(&mut self) -> Changed {
        let Some(raw) = self.pending.take() else {
            return Changed::NOTHING;
        };
        // Reopening already recorded the formula as it was, so one undo undoes the whole edit.
        let reopened = self.reopened.take().is_some();
        if raw.is_empty() {
            return Changed::TYPING;
        }
        let mut changed = if reopened {
            self.insert_raw(&raw)
        } else {
            self.snapshotted(|e| e.insert_raw(&raw))
        };
        changed.relayout = true;
        changed
    }

    fn insert_raw(&mut self, raw: &str) -> Changed {
        self.delete_selection();
        // A bare name gets empty arguments to fill; anything longer is read as written.
        if raw.chars().all(|ch| ch.is_ascii_alphabetic()) {
            if let Some(item) = self.parse_named(raw) {
                return self.insert_item(item, true);
            }
        } else if let Ok(expr) = crate::parse(&format!("\\{raw}")) {
            let mut items = raise(&expr, &mut self.ids).items;
            if items.len() == 1 {
                if let Some(item) = items.pop() {
                    return self.insert_item(item, true);
                }
            }
            return self.splice_in(items);
        }
        let item = self.unknown_item(raw);
        self.insert_item(item, false)
    }

    /// A run of letters that spells an alias becomes its construct: `sqrt` is `\sqrt{}`.
    ///
    /// Only a whole run counts, so `point` stays five letters rather than ending in `\int`.
    fn expand_alias(&mut self) -> Changed {
        let Some(slot) = cursor::resolve(&self.root, &self.cursor.path) else {
            return Changed::NOTHING;
        };
        let end = self.cursor.index.min(slot.items.len());
        let letter = |item: &Item| match item.kind {
            ItemKind::Sym {
                ch,
                alphabet: Alphabet::Auto,
                ..
            } if ch.is_ascii_alphabetic() => Some(ch),
            _ => None,
        };
        let mut start = end;
        while let Some(before) = start.checked_sub(1) {
            if slot.items.get(before).and_then(letter).is_none() {
                break;
            }
            start = before;
        }
        let word: String = slot
            .items
            .get(start..end)
            .unwrap_or_default()
            .iter()
            .filter_map(letter)
            .collect();
        if !ALIASES.contains(&word.as_str()) {
            return Changed::NOTHING;
        }
        let Some(item) = self.parse_named(&word) else {
            return Changed::NOTHING;
        };
        let Some(slot) = cursor::resolve_mut(&mut self.root, &self.cursor.path) else {
            return Changed::NOTHING;
        };
        splice(slot, start, end - start, Vec::new());
        self.cursor.index = start;
        self.insert_item(item, true)
    }

    /// `&`: another cell in the current row; an ordinary character outside an environment.
    fn add_cell(&mut self) -> Changed {
        self.anchor = None;
        let Some((parent, step)) = self.cursor.parent() else {
            return self.insert_symbol('&');
        };
        let SlotName::Cell { row, col } = step.slot else {
            return self.insert_symbol('&');
        };
        let parent = parent.to_vec();
        let child = step.child;
        let fresh = self.ids.fresh();
        let Some(rows) = self.env_rows_mut(&parent, child) else {
            return Changed::NOTHING;
        };
        let Some(cells) = rows.get_mut(usize::from(row)) else {
            return Changed::NOTHING;
        };
        let at = usize::from(col).saturating_add(1).min(cells.len());
        cells.insert(at, Slot::empty(fresh));
        // Other rows are left ragged, which is legal LaTeX.
        let Ok(col) = u16::try_from(at) else {
            return Changed::NOTHING;
        };
        self.go_to_cell(&parent, child, row, col, 0);
        Changed::EDITED
    }

    /// The rows of the environment at `child` in the slot at `parent`.
    fn env_rows_mut(&mut self, parent: &[Step], child: usize) -> Option<&mut Vec<Vec<Slot>>> {
        let slot = cursor::resolve_mut(&mut self.root, parent)?;
        match &mut slot.items.get_mut(child)?.kind {
            ItemKind::Env { rows, .. } => Some(rows),
            _ => None,
        }
    }

    fn go_to_cell(&mut self, parent: &[Step], child: usize, row: u16, col: u16, index: usize) {
        let mut path = parent.to_vec();
        path.push(Step::new(child, SlotName::Cell { row, col }));
        self.cursor = Cursor::at(path, index);
    }

    /// Space: a `\space` kern, since math mode discards space characters.
    fn insert_space(&mut self) -> Changed {
        self.delete_selection();
        let item = Item::new(self.ids.fresh(), ItemKind::Space(SPACE_MU));
        self.insert_item(item, false)
    }

    /// Enter: another row in the environment the caret is in.
    fn add_row(&mut self) -> Changed {
        self.anchor = None;
        let Some((parent, step)) = self.cursor.parent() else {
            return Changed::NOTHING;
        };
        let SlotName::Cell { row, .. } = step.slot else {
            return Changed::NOTHING;
        };
        let parent = parent.to_vec();
        let child = step.child;
        let width = cursor::item_at(&self.root, &parent, child)
            .and_then(|item| match &item.kind {
                ItemKind::Env { rows, .. } => Some(rows.get(usize::from(row))?.len().max(1)),
                _ => None,
            })
            .unwrap_or(1);
        let fresh: Vec<Slot> = (0..width).map(|_| Slot::empty(self.ids.fresh())).collect();
        let Some(rows) = self.env_rows_mut(&parent, child) else {
            return Changed::NOTHING;
        };
        let at = usize::from(row).saturating_add(1).min(rows.len());
        rows.insert(at, fresh);
        let Ok(row) = u16::try_from(at) else {
            return Changed::NOTHING;
        };
        self.go_to_cell(&parent, child, row, 0, 0);
        Changed::EDITED
    }

    /// A paste or IME commit: parsed as LaTeX if possible, else inserted as characters.
    fn insert_text(&mut self, text: &str) -> Changed {
        if text.is_empty() {
            return Changed::NOTHING;
        }
        if text.contains('\\') {
            if let Ok(expr) = crate::parse(text) {
                let items = raise(&expr, &mut self.ids).items;
                return self.splice_in(items);
            }
        }
        self.delete_selection();
        let items: Vec<Item> = text.chars().map(|ch| self.symbol_item(ch)).collect();
        self.splice_in(items)
    }

    /// Whether `extra` more levels of nesting at the caret still fit under [`MAX_DEPTH`].
    fn has_room(&self, extra: usize) -> bool {
        self.cursor.path.len().saturating_add(extra) < MAX_DEPTH
    }

    /// Drop a run of items in at the caret, replacing the selection.
    fn splice_in(&mut self, items: Vec<Item>) -> Changed {
        let deepest = items.iter().map(Item::depth).max().unwrap_or(0);
        if !self.has_room(deepest) {
            return Changed::NOTHING;
        }
        self.delete_selection();
        let count = items.len();
        let Some(slot) = cursor::resolve_mut(&mut self.root, &self.cursor.path) else {
            return Changed::NOTHING;
        };
        let at = self.cursor.index.min(slot.items.len());
        splice(slot, at, 0, items);
        self.cursor.index = at + count;
        Changed::EDITED
    }

    fn symbol_item(&mut self, ch: char) -> Item {
        Item::new(
            self.ids.fresh(),
            ItemKind::Sym {
                ch: math_char(ch),
                class: class_of_char(ch),
                alphabet: Alphabet::Auto,
            },
        )
    }

    fn insert_symbol(&mut self, ch: char) -> Changed {
        self.delete_selection();
        let item = self.symbol_item(ch);
        self.insert_item(item, false)
    }

    /// Put an item at the caret; with `enter`, the caret lands in its first empty slot.
    fn insert_item(&mut self, item: Item, enter: bool) -> Changed {
        if !self.has_room(item.depth()) {
            return Changed::NOTHING;
        }
        let hole = enter.then(|| first_hole(&item)).flatten();
        let Some(slot) = cursor::resolve_mut(&mut self.root, &self.cursor.path) else {
            return Changed::NOTHING;
        };
        let at = self.cursor.index.min(slot.items.len());
        splice(slot, at, 0, vec![item]);
        match hole {
            Some(mut steps) => {
                if let Some(first) = steps.first_mut() {
                    first.child = at;
                }
                self.cursor.path.extend(steps);
                self.cursor.index = 0;
            }
            None => self.cursor.index = at + 1,
        }
        Changed::EDITED
    }

    /// A construct by name, built by asking the parser what it is.
    fn insert_named(&mut self, name: &str) -> Changed {
        if name.is_empty() {
            return Changed::NOTHING;
        }
        self.delete_selection();
        let Some(item) = self.parse_named(name) else {
            return Changed::NOTHING;
        };
        self.insert_item(item, true)
    }

    /// Backspace while a `\command` is typed: one character, or the backslash itself.
    fn unwind_pending(&mut self) -> Changed {
        if let Some(mut name) = self.pending.take() {
            if name.pop().is_some() {
                self.pending = Some(name);
            } else {
                // Backspaced away entirely: deliberate, so nothing is restored.
                self.reopened = None;
            }
        }
        Changed::TYPING
    }

    /// A `\command` the parser rejected, as red text.
    fn unknown_item(&mut self, raw: &str) -> Item {
        let text = Item::new(
            self.ids.fresh(),
            ItemKind::Text(format!("\\{}", balanced(raw))),
        );
        Item::new(
            self.ids.fresh(),
            ItemKind::Styled {
                decor: Decor::colour(UNKNOWN_PAINT),
                body: Slot::of(self.ids.fresh(), vec![text]),
            },
        )
    }

    /// `\name` with as few empty arguments as the parser will accept.
    fn parse_named(&mut self, name: &str) -> Option<Item> {
        const MAX_ARGUMENTS: usize = 3;
        // An environment is tried as `\begin{name}` before the `\name{}` spellings.
        let sources = core::iter::once(format!("\\begin{{{name}}}\\end{{{name}}}"))
            .chain((0..=MAX_ARGUMENTS).map(|arity| format!("\\{name}{}", "{}".repeat(arity))));
        for source in sources {
            let Ok(expr) = crate::parse(&source) else {
                continue;
            };
            let mut items = raise(&expr, &mut self.ids).items;
            if items.len() == 1 {
                return items.pop();
            }
        }
        None
    }

    /// `/`: the operand run before the caret becomes a numerator.
    fn make_fraction(&mut self) -> Changed {
        if !self.has_room(1) {
            return Changed::NOTHING;
        }
        let (start, count) = match self.selection() {
            Some(range) => {
                self.anchor = None;
                let (start, count) = (range.start, range.len());
                self.cursor = Cursor::at(range.path, start);
                (start, count)
            }
            None => {
                let Some(slot) = cursor::resolve(&self.root, &self.cursor.path) else {
                    return Changed::NOTHING;
                };
                let start = operand_start(slot, self.cursor.index);
                (start, self.cursor.index.saturating_sub(start))
            }
        };
        let Some(slot) = cursor::resolve_mut(&mut self.root, &self.cursor.path) else {
            return Changed::NOTHING;
        };
        let taken = splice(slot, start, count, Vec::new());

        let num = Slot::of(self.ids.fresh(), taken);
        let den = Slot::empty(self.ids.fresh());
        let item = Item::new(
            self.ids.fresh(),
            ItemKind::Frac {
                num,
                den,
                style: crate::FracStyle::Auto,
                bar: true,
            },
        );
        let Some(slot) = cursor::resolve_mut(&mut self.root, &self.cursor.path) else {
            return Changed::NOTHING;
        };
        splice(slot, start, 0, vec![item]);
        self.cursor.path.push(Step::new(start, SlotName::Den));
        self.cursor.index = 0;
        Changed::EDITED
    }

    /// `^` or `_`: attach a script to whatever is before the caret.
    fn open_script(&mut self, which: SlotName) -> Changed {
        self.delete_selection();
        // `\int_0^1`: the `^` belongs to the integral, so step out of the limit first.
        if let Some((parent, step)) = self.cursor.parent() {
            if is_sibling_script(step.slot, which) {
                self.cursor = Cursor::at(parent.to_vec(), step.child + 1);
            }
        }
        let Some(slot) = cursor::resolve(&self.root, &self.cursor.path) else {
            return Changed::NOTHING;
        };
        let before = self.cursor.index.checked_sub(1);
        let existing = before.and_then(|i| slot.items.get(i)).map(|i| &i.kind);

        let target = match (existing, which) {
            (Some(ItemKind::BigOp { .. }), SlotName::Sup) => SlotName::Upper,
            (Some(ItemKind::BigOp { .. }), SlotName::Sub) => SlotName::Lower,
            (_, name) => name,
        };
        let attachable = matches!(
            existing,
            Some(ItemKind::Scripts { .. } | ItemKind::BigOp { .. })
        );

        if let (Some(child), true) = (before, attachable) {
            let fresh = self.ids.fresh();
            let Some(slot) = cursor::resolve_mut(&mut self.root, &self.cursor.path) else {
                return Changed::NOTHING;
            };
            let Some(item) = slot.items.get_mut(child) else {
                return Changed::NOTHING;
            };
            let created = ensure_slot(item, target, fresh);
            let len = item.slot(target).map_or(0, Slot::len);
            self.cursor.path.push(Step::new(child, target));
            self.cursor.index = len;
            return if created {
                Changed::EDITED
            } else {
                Changed::MOVED
            };
        }

        if !self.has_room(1) {
            return Changed::NOTHING;
        }
        let base_id = self.ids.fresh();
        let script_id = self.ids.fresh();
        let item_id = self.ids.fresh();
        let Some(slot) = cursor::resolve_mut(&mut self.root, &self.cursor.path) else {
            return Changed::NOTHING;
        };
        let (at, base_items) = match before {
            Some(child) if child < slot.items.len() => (child, splice(slot, child, 1, Vec::new())),
            _ => (self.cursor.index.min(slot.items.len()), Vec::new()),
        };
        let script = Slot::empty(script_id);
        let kind = ItemKind::Scripts {
            base: Slot::of(base_id, base_items),
            sub: (target == SlotName::Sub).then(|| script.clone()),
            sup: (target == SlotName::Sup).then_some(script),
        };
        splice(slot, at, 0, vec![Item::new(item_id, kind)]);
        self.cursor.path.push(Step::new(at, target));
        self.cursor.index = 0;
        Changed::EDITED
    }

    /// `(` or `[`: a stretchy pair with the caret inside it.
    fn open_delimiter(&mut self, open: char) -> Changed {
        self.delete_selection();
        let close = match open {
            '(' => ')',
            '[' => ']',
            other => other,
        };
        let body = Slot::empty(self.ids.fresh());
        let item = Item::new(
            self.ids.fresh(),
            ItemKind::Delimited {
                left: crate::Delim::Char(open),
                body,
                right: crate::Delim::Char(close),
            },
        );
        self.insert_item(item, true)
    }

    fn open_group(&mut self) -> Changed {
        self.delete_selection();
        let body = Slot::empty(self.ids.fresh());
        let item = Item::new(self.ids.fresh(), ItemKind::Group(body));
        self.insert_item(item, true)
    }

    /// `)` or `]`: step out of the pair it closes.
    fn close_delimiter(&mut self, close: char) -> Changed {
        let inside = self.cursor.parent().is_some_and(|(parent, step)| {
            step.slot == SlotName::Body
                && match cursor::item_at(&self.root, parent, step.child).map(|item| &item.kind) {
                    Some(ItemKind::Group(_)) => close == '}',
                    Some(ItemKind::Delimited { right, .. }) => *right == crate::Delim::Char(close),
                    _ => false,
                }
        });
        if inside {
            return self.move_caret(Motion::Right, false);
        }
        self.snapshot_typing();
        self.insert_symbol(close)
    }

    /// Backspace: enters a preceding construct; at a slot start, unwraps the enclosing one.
    fn backspace(&mut self) -> Changed {
        if self.delete_selection() {
            return Changed::EDITED;
        }
        let Some(before) = self.cursor.index.checked_sub(1) else {
            return self.dissolve_parent(false);
        };
        let Some(slot) = cursor::resolve(&self.root, &self.cursor.path) else {
            return Changed::NOTHING;
        };
        let Some(item) = slot.items.get(before) else {
            return Changed::NOTHING;
        };
        // An unknown `\command` goes back to being typed, one character shorter.
        if let Some(name) = unknown_name(item) {
            self.reopened = Some(name.to_owned());
            let mut name = name.to_owned();
            name.pop();
            let Some(slot) = cursor::resolve_mut(&mut self.root, &self.cursor.path) else {
                return Changed::NOTHING;
            };
            splice(slot, before, 1, Vec::new());
            self.cursor.index = before;
            self.pending = Some(name);
            return Changed::EDITED;
        }
        if let Some(name) = item.slots_in_order().last().copied() {
            let len = item.slot(name).map_or(0, Slot::len);
            self.cursor.path.push(Step::new(before, name));
            self.cursor.index = len;
            return Changed::MOVED;
        }
        let Some(slot) = cursor::resolve_mut(&mut self.root, &self.cursor.path) else {
            return Changed::NOTHING;
        };
        splice(slot, before, 1, Vec::new());
        self.cursor.index = before;
        Changed::EDITED
    }

    /// Delete: backspace's mirror.
    fn delete_forwards(&mut self) -> Changed {
        if self.delete_selection() {
            return Changed::EDITED;
        }
        let at = self.cursor.index;
        let Some(slot) = cursor::resolve(&self.root, &self.cursor.path) else {
            return Changed::NOTHING;
        };
        let Some(item) = slot.items.get(at) else {
            return self.dissolve_parent(true);
        };
        if unknown_name(item).is_some() {
            self.cursor.path.push(Step::new(at, SlotName::Body));
            self.cursor.index = 0;
            return self.reopen_inside(false);
        }
        if let Some(name) = item.slots_in_order().first().copied() {
            self.cursor.path.push(Step::new(at, name));
            self.cursor.index = 0;
            return Changed::MOVED;
        }
        let Some(slot) = cursor::resolve_mut(&mut self.root, &self.cursor.path) else {
            return Changed::NOTHING;
        };
        splice(slot, at, 1, Vec::new());
        Changed::EDITED
    }

    /// Take the enclosing construct apart from inside it.
    fn dissolve_parent(&mut self, forwards: bool) -> Changed {
        let Some((parent, step)) = self.cursor.parent() else {
            return Changed::NOTHING;
        };
        let parent = parent.to_vec();
        // Cells merge into each other rather than unwrapping the environment.
        if let SlotName::Cell { row, col } = step.slot {
            if forwards || col > 0 || row > 0 {
                return self.merge_cell(&parent, step.child, row, col, forwards);
            }
        }
        let Some(item) = cursor::item_at(&self.root, &parent, step.child) else {
            return Changed::NOTHING;
        };
        let here = item
            .slots_in_order()
            .iter()
            .position(|n| *n == step.slot)
            .unwrap_or(0);
        let this_is_empty = item.slot(step.slot).is_some_and(Slot::is_empty);

        // An empty optional slot is removed, leaving the construct standing.
        if this_is_empty && is_optional(step.slot) {
            let Some(slot) = cursor::resolve_mut(&mut self.root, &parent) else {
                return Changed::NOTHING;
            };
            let Some(item) = slot.items.get_mut(step.child) else {
                return Changed::NOTHING;
            };
            clear_slot(item, step.slot);
            let degenerate =
                item.slots_in_order().len() <= 1 && matches!(item.kind, ItemKind::Scripts { .. });
            self.cursor = Cursor::at(parent.clone(), step.child + 1);
            if degenerate {
                self.unwrap_item(&parent, step.child, None);
            }
            return Changed::EDITED;
        }

        self.unwrap_item(&parent, step.child, Some(here));
        Changed::EDITED
    }

    /// Backspace or delete across the boundary between two cells.
    fn merge_cell(
        &mut self,
        parent: &[Step],
        child: usize,
        row: u16,
        col: u16,
        forwards: bool,
    ) -> Changed {
        let (keep, gone) = if forwards {
            (col, col.saturating_add(1))
        } else {
            match col.checked_sub(1) {
                Some(keep) => (keep, col),
                // First cell of a later row: the row goes only if it is empty.
                None => return self.drop_row(parent, child, row),
            }
        };

        // `saturating_add` may not have moved to a different cell.
        if keep == gone {
            return Changed::NOTHING;
        }
        let Some(rows) = self.env_rows_mut(parent, child) else {
            return Changed::NOTHING;
        };
        let Some(cells) = rows.get_mut(usize::from(row)) else {
            return Changed::NOTHING;
        };
        if usize::from(gone) >= cells.len() {
            return Changed::NOTHING;
        }
        let moved = cells.remove(usize::from(gone)).items;
        let Some(into) = cells.get_mut(usize::from(keep)) else {
            return Changed::NOTHING;
        };
        let join = into.items.len();
        into.items.extend(moved);

        self.go_to_cell(parent, child, row, keep, join);
        Changed::EDITED
    }

    /// Remove an empty row, putting the caret at the end of the one above.
    fn drop_row(&mut self, parent: &[Step], child: usize, row: u16) -> Changed {
        let Some(above) = row.checked_sub(1) else {
            return Changed::NOTHING;
        };
        let Some(rows) = self.env_rows_mut(parent, child) else {
            return Changed::NOTHING;
        };
        let empty = rows
            .get(usize::from(row))
            .is_some_and(|cells| cells.iter().all(Slot::is_empty));
        if !empty || rows.len() <= 1 {
            return Changed::NOTHING;
        }
        rows.remove(usize::from(row));
        let Some(cells) = rows.get(usize::from(above)) else {
            return Changed::NOTHING;
        };
        let Ok(col) = u16::try_from(cells.len().saturating_sub(1)) else {
            return Changed::NOTHING;
        };
        let index = cells.get(usize::from(col)).map_or(0, Slot::len);
        self.go_to_cell(parent, child, above, col, index);
        Changed::EDITED
    }

    /// Replace an item with the contents of its slots; the caret lands after `land_before` of them.
    fn unwrap_item(&mut self, parent: &[Step], child: usize, land_before: Option<usize>) {
        let Some(slot) = cursor::resolve_mut(&mut self.root, parent) else {
            return;
        };
        let Some(item) = slot.items.get(child) else {
            return;
        };
        let names = item.slots_in_order();
        let mut contents: Vec<Item> = Vec::new();
        let mut landing = child;
        for (index, name) in names.iter().enumerate() {
            if Some(index) == land_before {
                landing = child + contents.len();
            }
            if let Some(inner) = item.slot(*name) {
                contents.extend(inner.items.iter().cloned());
            }
        }
        if land_before.is_none() {
            landing = child + contents.len();
        }
        splice(slot, child, 1, contents);
        self.cursor = Cursor::at(parent.to_vec(), landing);
    }

    /// Remove the selected items, if any; always drops the anchor.
    fn delete_selection(&mut self) -> bool {
        let range = self.selection();
        self.anchor = None;
        let Some(range) = range else {
            return false;
        };
        let Some(slot) = cursor::resolve_mut(&mut self.root, &range.path) else {
            return false;
        };
        splice(slot, range.start, range.len(), Vec::new());
        self.cursor = Cursor::at(range.path, range.start);
        true
    }

    /// Run an operation, recording the tree for undo only if it changed.
    fn snapshotted(&mut self, edit: impl FnOnce(&mut Editor) -> Changed) -> Changed {
        // Redo is held, not cleared, in case this turns out to be a move.
        let redo = core::mem::take(&mut self.redo);
        self.snapshot();
        let changed = edit(self);
        if !changed.tree {
            self.undo.pop();
            self.redo = redo;
        }
        changed
    }

    /// Record the tree before an edit that is not a plain keystroke.
    fn snapshot(&mut self) {
        self.push_undo();
        self.coalescing = false;
    }

    /// Record it before a keystroke, coalescing a run of them into one entry.
    fn snapshot_typing(&mut self) {
        if !self.coalescing {
            self.push_undo();
            self.coalescing = true;
        }
    }

    fn push_undo(&mut self) {
        const DEPTH: usize = 200;
        self.redo.clear();
        self.undo.push((self.root.clone(), self.cursor.clone()));
        if self.undo.len() > DEPTH {
            self.undo.remove(0);
        }
    }

    fn undo(&mut self) -> Changed {
        let Some(state) = self.undo.pop() else {
            return Changed::NOTHING;
        };
        let current = self.restore(state);
        self.redo.push(current);
        Changed::EDITED
    }

    fn redo(&mut self) -> Changed {
        let Some(state) = self.redo.pop() else {
            return Changed::NOTHING;
        };
        let current = self.restore(state);
        self.undo.push(current);
        Changed::EDITED
    }

    /// Swap in a recorded state, returning the one it replaced.
    fn restore(&mut self, (root, cursor): (Slot, Cursor)) -> (Slot, Cursor) {
        self.anchor = None;
        self.coalescing = false;
        (
            core::mem::replace(&mut self.root, root),
            core::mem::replace(&mut self.cursor, cursor),
        )
    }
}

/// Depth-first path to an item's first empty slot; the first step's `child` is a placeholder.
fn first_hole(item: &Item) -> Option<Vec<Step>> {
    for name in item.slots_in_order() {
        let Some(slot) = item.slot(name) else {
            continue;
        };
        if slot.is_empty() {
            return Some(vec![Step::new(0, name)]);
        }
        for (index, inner) in slot.items.iter().enumerate() {
            if let Some(mut deeper) = first_hole(inner) {
                if let Some(first) = deeper.first_mut() {
                    first.child = index;
                }
                deeper.insert(0, Step::new(0, name));
                return Some(deeper);
            }
        }
    }
    None
}

/// One endpoint of a selection as a range in the common slot, widened to whole items.
fn endpoint(cursor: &Cursor, common: usize) -> (usize, usize) {
    match cursor.path.get(common) {
        Some(step) => (step.child, step.child + 1),
        None => (cursor.index, cursor.index),
    }
}

fn operand_start(slot: &Slot, index: usize) -> usize {
    let mut start = index.min(slot.items.len());
    while let Some(before) = start.checked_sub(1) {
        let Some(item) = slot.items.get(before) else {
            break;
        };
        let class = lower_item(item, Lowering::TEX).class();
        if !matches!(
            class,
            AtomClass::Ord | AtomClass::Close | AtomClass::Inner | AtomClass::Op
        ) {
            break;
        }
        start = before;
    }
    start
}

/// The width a typed space inserts, in math units: `\space`, one sixth of an em.
const SPACE_MU: i16 = 6;

/// Whether the caret is in one script of a pair and asking for the other.
fn is_sibling_script(here: SlotName, wanted: SlotName) -> bool {
    matches!(
        (here, wanted),
        (SlotName::Sub, SlotName::Sup)
            | (SlotName::Sup, SlotName::Sub)
            | (SlotName::Lower, SlotName::Sup)
            | (SlotName::Upper, SlotName::Sub)
    )
}

/// Whether a slot can be absent rather than merely empty.
fn is_optional(name: SlotName) -> bool {
    matches!(
        name,
        SlotName::Sub
            | SlotName::Sup
            | SlotName::Lower
            | SlotName::Upper
            | SlotName::Degree
            | SlotName::Over
            | SlotName::Under
    )
}

fn optional_slot_mut(item: &mut Item, name: SlotName) -> Option<&mut Option<Slot>> {
    Some(match (&mut item.kind, name) {
        (ItemKind::Scripts { sub, .. }, SlotName::Sub) => sub,
        (ItemKind::Scripts { sup, .. }, SlotName::Sup) => sup,
        (ItemKind::BigOp { lower, .. }, SlotName::Lower) => lower,
        (ItemKind::BigOp { upper, .. }, SlotName::Upper) => upper,
        (ItemKind::Sqrt { degree, .. }, SlotName::Degree) => degree,
        (ItemKind::Stack { over, .. }, SlotName::Over) => over,
        (ItemKind::Stack { under, .. }, SlotName::Under) => under,
        (ItemKind::XArrow { over, .. }, SlotName::Over) => over,
        (ItemKind::XArrow { under, .. }, SlotName::Under) => under,
        _ => return None,
    })
}

/// Give an item the named optional slot; returns whether it had to be created.
fn ensure_slot(item: &mut Item, name: SlotName, id: crate::edit::EditId) -> bool {
    match optional_slot_mut(item, name) {
        Some(hole @ None) => {
            *hole = Some(Slot::empty(id));
            true
        }
        _ => false,
    }
}

fn clear_slot(item: &mut Item, name: SlotName) {
    if let Some(hole) = optional_slot_mut(item, name) {
        *hole = None;
    }
}

/// Replace `count` items at `at` with `with`, returning what was there; clamps rather than panicking.
fn splice(slot: &mut Slot, at: usize, count: usize, with: Vec<Item>) -> Vec<Item> {
    let len = slot.items.len();
    let at = at.min(len);
    let end = at.saturating_add(count).min(len);
    slot.items.splice(at..end, with).collect()
}

/// Words that turn into their construct as soon as the last letter is typed.
///
/// None is a prefix of another (`sinh` and `limsup` are left out for that reason), so a
/// word never fires early.
const ALIASES: &[&str] = &[
    "sqrt", "sum", "prod", "coprod", "int", "iint", "oint", "lim", "infty", "sin", "cos", "tan",
    "sec", "csc", "cot", "log", "ln", "exp", "det", "max", "min", "gcd", "alpha", "beta", "gamma",
    "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa", "lambda", "mu", "nu", "xi", "pi",
    "rho", "sigma", "tau", "upsilon", "phi", "chi", "psi", "omega", "Gamma", "Delta", "Theta",
    "Lambda", "Xi", "Pi", "Sigma", "Upsilon", "Phi", "Psi", "Omega",
];

/// `raw` with its braces balanced, so that it can sit inside `\text{…}`.
fn balanced(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 2);
    let mut depth = 0usize;
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                out.push(ch);
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            '{' => {
                depth += 1;
                out.push(ch);
            }
            '}' if depth == 0 => {}
            '}' => {
                depth -= 1;
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out.extend(core::iter::repeat_n('}', depth));
    out
}

/// The name in an unknown-command item: red text that starts with a backslash.
fn unknown_name(item: &Item) -> Option<&str> {
    let ItemKind::Styled { decor, body } = &item.kind else {
        return None;
    };
    if decor.colour != Some(UNKNOWN_PAINT) {
        return None;
    }
    match body.items.as_slice() {
        [
            Item {
                kind: ItemKind::Text(text),
                ..
            },
        ] => text.strip_prefix('\\'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{Editor, SlotName};

    /// Type `text` key by key; a newline is Enter, which submits a raw `\\command`.
    fn typed(text: &str) -> Editor {
        let mut editor = Editor::new();
        type_into(&mut editor, text);
        editor
    }

    fn type_into(editor: &mut Editor, text: &str) {
        for ch in text.chars() {
            match ch {
                '\n' => editor.apply(Command::Submit),
                _ => editor.apply(Command::Insert(ch)),
            };
        }
    }

    #[test]
    fn a_slash_swallows_the_operand_and_nothing_more() {
        assert_eq!(typed("x+y/").tex(), r"x+\frac{y}{}");
        assert_eq!(typed("f(x)/").tex(), r"\frac{f\left(x\right)}{}");
        assert_eq!(typed("/").tex(), r"\frac{}{}");
        assert_eq!(typed("ab/c").tex(), r"\frac{ab}{c}");
        assert_eq!(typed("a=b/").tex(), r"a=\frac{b}{}");
    }

    #[test]
    fn a_slash_leaves_the_caret_in_the_denominator() {
        let editor = typed("x/");
        assert_eq!(
            editor.cursor().path.last().map(|s| s.slot),
            Some(SlotName::Den)
        );
        assert_eq!(editor.cursor().index, 0);
    }

    #[test]
    fn scripts_attach_to_what_is_before_them() {
        assert_eq!(typed("x^2").tex(), "x^2");
        assert_eq!(typed("x_i").tex(), "x_{i}");
        let mut editor = typed("x^2");
        editor.apply(Command::Move(Motion::Right));
        editor.insert_str("_i");
        assert_eq!(editor.tex(), "x_{i}^2");
    }

    #[test]
    fn a_big_operator_gets_limits_not_scripts() {
        let mut editor = Editor::new();
        editor.apply(Command::Named("sum".into()));
        editor.insert_str("_n");
        assert_eq!(editor.tex(), r"\sum_{n}");
    }

    #[test]
    fn a_second_script_attaches_to_the_same_base() {
        assert_eq!(typed("\\int\n_0^1").tex(), r"\int_0^1");
        assert_eq!(typed("\\int\n^1_0").tex(), r"\int_0^1");
        assert_eq!(typed("\\sum\n_{n}^{k}").tex(), r"\sum_{{n}}^{{k}}");
        assert_eq!(typed("x_i^2").tex(), "x_{i}^2");
        assert_eq!(typed("x^2_i").tex(), "x_{i}^2");
        assert_eq!(typed("\\int\n_0^1x").tex(), r"\int_0^{1x}");
    }

    #[test]
    fn a_script_with_no_base_gets_an_empty_one() {
        assert_eq!(typed("^2").tex(), "{}^2");
    }

    #[test]
    fn a_bracket_opens_a_pair_and_its_partner_steps_out() {
        assert_eq!(typed("(x").tex(), r"\left(x\right)");
        assert_eq!(typed("(x)").tex(), r"\left(x\right)");
        assert_eq!(typed("(x)y").tex(), r"\left(x\right)y");
        assert_eq!(typed("x)").tex(), "x)");
    }

    #[test]
    fn backspace_enters_a_construct_before_it_deletes_one() {
        let mut editor = typed("a/b");
        editor.apply(Command::Move(Motion::Right));
        editor.apply(Command::Move(Motion::Right));
        assert_eq!(editor.cursor().path.len(), 0, "not outside the fraction");
        editor.apply(Command::Backspace);
        assert_eq!(editor.tex(), r"\frac{a}{b}", "the fraction was deleted");
        assert_eq!(
            editor.cursor().path.last().map(|s| s.slot),
            Some(SlotName::Den)
        );
    }

    #[test]
    fn backspace_at_the_start_unwraps_rather_than_deleting() {
        let mut editor = typed("a/b");
        editor.apply(Command::Move(Motion::Home));
        editor.apply(Command::Backspace);
        assert_eq!(editor.tex(), "ab");
    }

    #[test]
    fn backspacing_out_of_an_empty_script_undoes_it() {
        let mut editor = typed("x^");
        editor.apply(Command::Backspace);
        assert_eq!(editor.tex(), "x");
    }

    #[test]
    fn an_unwrap_keeps_the_ids_of_what_it_frees() {
        let mut editor = typed("ab/cd");
        let before = ids_of(&editor);
        editor.apply(Command::Move(Motion::Home));
        editor.apply(Command::Backspace);
        assert_eq!(editor.tex(), "abcd");
        let after = ids_of(&editor);
        let kept = before.iter().filter(|id| after.contains(id)).count();
        assert!(kept >= 4, "only {kept} ids survived the unwrap");
    }

    #[test]
    fn inserting_leaves_every_other_id_alone() {
        let mut editor = typed("abc");
        let before = ids_of(&editor);
        editor.apply(Command::Move(Motion::Home));
        editor.apply(Command::Insert('z'));
        let after = ids_of(&editor);
        for id in &before {
            assert!(after.contains(id), "{id:?} was renumbered");
        }
    }

    fn ids_of(editor: &Editor) -> Vec<crate::edit::EditId> {
        fn walk(slot: &crate::edit::Slot, out: &mut Vec<crate::edit::EditId>) {
            out.push(slot.id);
            for item in &slot.items {
                out.push(item.id);
                for name in item.slots_in_order() {
                    if let Some(child) = item.slot(name) {
                        walk(child, out);
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(editor.root(), &mut out);
        out
    }

    #[test]
    fn a_typed_command_builds_the_construct_the_parser_knows() {
        assert_eq!(typed("\\sqrt\nx").tex(), r"\sqrt{x}");
        assert_eq!(typed("\\alpha\n").tex(), r"\alpha");
        assert_eq!(typed("\\binom\nnk").tex(), r"\binom{nk}{}");
        assert_eq!(typed("\\sin\nx").tex(), r"\sin x");
        let mut editor = Editor::new();
        editor.apply(Command::Named("pmatrix".into()));
        editor.insert_str("a");
        assert_eq!(editor.tex(), r"\begin{pmatrix}a \end{pmatrix}");
    }

    /// Until Enter, a raw command takes every key: braces, brackets and spaces are its own.
    #[test]
    fn a_raw_command_is_read_as_written_on_enter() {
        for (keys, tex) in [
            ("\\frac{a}{b}\n", r"\frac{a}{b}"),
            ("\\sqrt[3]{x}\n", r"\sqrt[3]{x}"),
            ("\\text{hi there}\n", r"\text{hi there}"),
            ("\\left(x\\right)\n", r"\left(x\right)"),
            ("\\textcolor{red}{x}\n", r"\textcolor{#ff0000}{x}"),
            ("\\alpha x\n", r"\alpha x"),
            ("\\left(a\\middle|b\\right)\n", r"\left(a\middle|b\right)"),
            ("\\mathscr{L}\n", r"\mathcal{L}"),
        ] {
            let editor = typed(keys);
            assert_eq!(editor.tex(), tex, "{keys:?}");
            assert!(editor.unknown_commands().is_empty(), "{keys:?}");
        }
        #[cfg(feature = "formatting")]
        for (keys, tex) in [
            ("\\boxed{x}\n", r"\boxed{x}"),
            ("\\cancel{x}\n", r"\cancel{x}"),
            ("\\colorbox{red}{hi}\n", r"\colorbox{#ff0000}{hi}"),
        ] {
            let editor = typed(keys);
            assert_eq!(editor.tex(), tex, "{keys:?}");
            assert!(editor.unknown_commands().is_empty(), "{keys:?}");
        }
        let mut editor = typed(r"x+\frac{a}{b} ");
        assert_eq!(editor.pending(), Some("frac{a}{b} "));
        assert_eq!(editor.tex(), "x+");
        editor.apply(Command::Submit);
        assert_eq!(editor.pending(), None);
        assert_eq!(editor.tex(), r"x+\frac{a}{b}");
    }

    #[test]
    fn enter_with_nothing_typed_is_not_a_submission() {
        let mut editor = typed("x");
        assert!(!editor.apply(Command::Submit).is_anything());
        // A lone backslash submits to nothing.
        type_into(&mut editor, "\\\n");
        assert_eq!(editor.tex(), "x");
        assert_eq!(editor.pending(), None);
    }

    #[test]
    fn an_alias_turns_into_its_construct() {
        assert_eq!(typed("sqrtx").tex(), r"\sqrt{x}");
        assert_eq!(typed("1+int_0^1").tex(), r"1+\int_0^1");
        assert_eq!(typed("2alpha").tex(), r"2\alpha");
        assert_eq!(typed("sinx").tex(), r"\sin x");
        assert_eq!(typed("theta").tex(), r"\theta");
        // Only a whole run of letters counts.
        assert_eq!(typed("point").tex(), "point");
        assert_eq!(typed("beta").tex(), r"\beta");
    }

    #[test]
    fn no_alias_is_a_prefix_of_another() {
        for a in ALIASES {
            for b in ALIASES {
                assert!(
                    a == b || !b.starts_with(a),
                    "{a} fires before {b} can be typed"
                );
            }
        }
    }

    #[test]
    fn an_unbalanced_raw_command_is_kept_balanced() {
        let editor = typed("\\frac{a\n");
        assert_eq!(editor.unknown_commands(), [r"\frac{a}"]);
        let back = Editor::from_tex(&editor.tex()).expect("it parses");
        assert_eq!(back.unknown_commands(), [r"\frac{a}"]);
        assert_eq!(typed("\\x}}\n").unknown_commands(), [r"\x"]);
    }

    #[test]
    fn an_unknown_command_stays_as_red_text() {
        let editor = typed("x\\notacommand\ny");
        assert_eq!(editor.unknown_commands(), [r"\notacommand"]);
        assert_eq!(editor.tex(), r"x\textcolor{#d02020}{\text{\notacommand}}y");
        // It survives a round trip through LaTeX.
        let back = Editor::from_tex(&editor.tex()).expect("it parses");
        assert_eq!(back.unknown_commands(), [r"\notacommand"]);
    }

    #[test]
    fn backspace_edits_a_half_typed_command() {
        let mut editor = typed(r"\alpz");
        editor.apply(Command::Backspace);
        assert_eq!(editor.pending(), Some("alp"));
        type_into(&mut editor, "ha\n");
        assert_eq!(editor.tex(), r"\alpha");
        // Backspace past the letters drops the backslash too.
        let mut editor = typed(r"\a");
        editor.apply(Command::Backspace);
        assert_eq!(editor.pending(), Some(""));
        editor.apply(Command::Backspace);
        assert_eq!(editor.pending(), None);
        assert_eq!(editor.tex(), "");
    }

    #[test]
    fn backspace_reopens_an_unknown_command() {
        let mut editor = typed("\\alhpa\n");
        assert_eq!(editor.unknown_commands(), [r"\alhpa"]);
        editor.apply(Command::Backspace);
        assert_eq!(editor.pending(), Some("alhp"));
        assert!(editor.unknown_commands().is_empty());
        editor.apply(Command::Backspace);
        editor.apply(Command::Backspace);
        assert_eq!(editor.pending(), Some("al"));
        type_into(&mut editor, "pha\n");
        assert_eq!(editor.tex(), r"\alpha");
        assert!(editor.unknown_commands().is_empty());
    }

    /// A red command is never stepped into: reaching it from any side reopens it as typed.
    #[test]
    fn a_red_command_reopens_rather_than_being_entered() {
        let red = "x\\nope{y}\n";
        // Left from after it.
        let mut editor = typed(red);
        editor.apply(Command::Move(Motion::Left));
        assert_eq!(editor.pending(), Some("nope{y}"));
        assert_eq!(editor.tex(), "x");
        editor.apply(Command::Backspace);
        editor.apply(Command::Backspace);
        type_into(&mut editor, "x}\n");
        assert_eq!(editor.unknown_commands(), [r"\nope{x}"]);
        // Right from before it.
        let mut editor = typed(red);
        editor.set_cursor(Cursor::at(Vec::new(), 1));
        assert_eq!(editor.pending(), None);
        editor.apply(Command::Move(Motion::Right));
        assert_eq!(editor.pending(), Some("nope{y}"));
        // Delete from before it.
        let mut editor = typed(red);
        editor.set_cursor(Cursor::at(Vec::new(), 1));
        editor.apply(Command::Delete);
        assert_eq!(editor.pending(), Some("nope{y}"));
        // A click inside it.
        let mut editor = typed(red);
        editor.set_cursor(Cursor::at(vec![Step::new(1, SlotName::Body)], 1));
        assert_eq!(editor.pending(), Some("nope{y}"));
        assert_eq!(editor.cursor(), &Cursor::at(Vec::new(), 1));
    }

    #[test]
    fn escape_puts_a_reopened_command_back() {
        let mut editor = typed("x\\nope\n");
        let before = editor.tex();
        editor.apply(Command::Move(Motion::Left));
        type_into(&mut editor, "zzz");
        editor.apply(Command::Escape);
        assert_eq!(editor.pending(), None);
        assert_eq!(editor.tex(), before);
        // Clicking elsewhere does the same.
        editor.apply(Command::Backspace);
        assert_eq!(editor.pending(), Some("nop"));
        editor.set_cursor(Cursor::at(Vec::new(), 0));
        assert_eq!(editor.tex(), before);
        // Undo after a reopen and an edit brings the original back too.
        let mut editor = typed("x\\nope\n");
        editor.apply(Command::Move(Motion::Left));
        type_into(&mut editor, "s\n");
        assert_eq!(editor.unknown_commands(), [r"\nopes"]);
        editor.apply(Command::Undo);
        assert_eq!(editor.tex(), before);
    }

    #[test]
    fn arrows_do_nothing_while_a_raw_command_is_open() {
        let mut editor = typed("ab\\fr");
        assert!(!editor.apply(Command::Move(Motion::Left)).is_anything());
        assert_eq!(editor.cursor(), &Cursor::at(Vec::new(), 2));
        assert_eq!(editor.pending(), Some("fr"));
    }

    #[test]
    fn a_selection_takes_a_red_command_whole() {
        let mut editor = typed("x\\nope\n");
        editor.apply(Command::Extend(Motion::Left));
        assert_eq!(editor.pending(), None);
        assert_eq!(
            editor.selected_tex(),
            Some(r"\textcolor{#d02020}{\text{\nope}}".into())
        );
    }

    #[test]
    fn escape_abandons_a_half_typed_command() {
        let mut editor = Editor::new();
        editor.insert_str(r"\alp");
        assert_eq!(editor.pending(), Some("alp"));
        editor.apply(Command::Escape);
        assert_eq!(editor.pending(), None);
        assert_eq!(editor.tex(), "");
    }

    #[test]
    fn typing_over_a_selection_replaces_it() {
        let mut editor = typed("abc");
        editor.apply(Command::Move(Motion::Home));
        editor.apply(Command::Extend(Motion::Right));
        editor.apply(Command::Extend(Motion::Right));
        let range = editor.selection().expect("a selection");
        assert_eq!((range.start, range.end), (0, 2));
        editor.apply(Command::Insert('z'));
        assert_eq!(editor.tex(), "zc");
    }

    #[test]
    fn a_selection_out_of_a_slot_widens_to_the_whole_item() {
        let mut editor = typed("a/b");
        editor.apply(Command::Extend(Motion::Up));
        editor.apply(Command::Extend(Motion::Left));
        let range = editor.selection().expect("a selection");
        assert!(range.path.is_empty(), "not in the root slot: {range:?}");
        assert_eq!((range.start, range.end), (0, 1));
    }

    #[test]
    fn a_cancelled_selection_does_not_come_back() {
        let mut editor = typed("abc");
        editor.apply(Command::Move(Motion::Home));
        editor.apply(Command::Extend(Motion::Right));
        editor.apply(Command::Extend(Motion::Left));
        assert!(editor.selection().is_none(), "the range should be empty");

        editor.apply(Command::Insert('z'));
        assert_eq!(editor.tex(), "zabc");
        assert!(
            editor.selection().is_none(),
            "typing conjured a selection: {:?}",
            editor.selection()
        );
    }

    #[test]
    fn undo_takes_back_a_run_of_typing_at_once() {
        let mut editor = typed("abc");
        assert!(editor.can_undo());
        editor.apply(Command::Undo);
        assert_eq!(editor.tex(), "");
        editor.apply(Command::Redo);
        assert_eq!(editor.tex(), "abc");
    }

    #[test]
    fn undo_separates_a_construct_from_the_typing_around_it() {
        let mut editor = Editor::new();
        editor.insert_str("x");
        editor.apply(Command::Named("sqrt".into()));
        editor.insert_str("y");
        assert_eq!(editor.tex(), r"x\sqrt{y}");
        editor.apply(Command::Undo);
        assert_eq!(editor.tex(), r"x\sqrt{}");
        editor.apply(Command::Undo);
        assert_eq!(editor.tex(), "x");
    }

    #[test]
    fn a_keystroke_that_only_moves_leaves_no_undo_entry() {
        let mut editor = typed("ab");
        editor.apply(Command::Insert('/'));
        editor.apply(Command::Insert('c'));
        assert_eq!(editor.tex(), r"\frac{ab}{c}");

        editor.apply(Command::Move(Motion::Right));
        let entered = editor.apply(Command::Backspace);
        assert!(!entered.tree, "stepping in changed the tree");
        assert!(entered.cursor);

        editor.apply(Command::Undo);
        assert_eq!(editor.tex(), r"\frac{ab}{}", "undo skipped the real edit");
    }

    #[test]
    fn a_keystroke_that_only_moves_keeps_the_redo() {
        let mut editor = typed("ab");
        editor.apply(Command::Undo);
        assert_eq!(editor.tex(), "");
        assert!(editor.can_redo());
        editor.apply(Command::Backspace);
        assert!(editor.can_redo(), "a refused keystroke dropped the redo");
        editor.apply(Command::Redo);
        assert_eq!(editor.tex(), "ab");
    }

    #[test]
    fn nesting_stops_before_the_formula_stops_parsing() {
        for ch in ['(', '/', '^', '{', '['] {
            let mut editor = Editor::new();
            for _ in 0..200 {
                editor.apply(Command::Insert(ch));
            }
            let tex = editor.tex();
            assert!(
                crate::parse(&tex).is_ok(),
                "{ch:?} held down produced {tex:?}, which does not parse"
            );
            assert!(editor.root().depth() < crate::edit::tree::MAX_DEPTH);
        }
    }

    #[test]
    fn a_paste_too_deep_to_write_back_out_is_refused() {
        let nest = |levels: usize| format!("{}x{}", r"\frac{".repeat(levels), "}{}".repeat(levels));

        let mut editor = Editor::new();
        assert!(
            editor.apply(Command::InsertText(nest(3))).tree,
            "an ordinary paste was refused"
        );

        let mut editor = Editor::new();
        let changed = editor.apply(Command::InsertText(nest(30)));
        assert!(!changed.tree, "a paste past the bound was accepted");
        assert_eq!(editor.tex(), "", "a refused paste left something behind");
    }

    #[test]
    fn a_matrix_cell_merges_rather_than_dissolving_the_matrix() {
        const M: &str = r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}";
        let at =
            |row, col, index| Cursor::at(vec![Step::new(0, SlotName::Cell { row, col })], index);
        let after = |cursor, command| {
            let mut editor = Editor::from_tex(M).expect("parses");
            editor.set_cursor(cursor);
            let changed = editor.apply(command);
            (changed.tree, editor.tex())
        };

        let (edited, tex) = after(at(0, 1, 0), Command::Backspace);
        assert!(edited);
        assert_eq!(tex, r"\begin{pmatrix}ab \\ c & d \end{pmatrix}");

        let (edited, tex) = after(at(0, 0, 1), Command::Delete);
        assert!(edited);
        assert_eq!(tex, r"\begin{pmatrix}ab \\ c & d \end{pmatrix}");

        let (edited, _) = after(at(1, 1, 1), Command::Delete);
        assert!(!edited);
        let (edited, _) = after(at(1, 0, 0), Command::Backspace);
        assert!(!edited);

        let (edited, tex) = after(at(0, 0, 0), Command::Backspace);
        assert!(edited);
        assert_eq!(tex, "abcd");
    }

    #[test]
    fn an_empty_row_is_removed_by_backspacing_into_it() {
        let mut editor = Editor::from_tex(r"\begin{pmatrix} a & b \end{pmatrix}").expect("parses");
        editor.set_cursor(Cursor::at(
            vec![Step::new(0, SlotName::Cell { row: 0, col: 1 })],
            1,
        ));
        editor.apply(Command::NewRow);
        assert_eq!(editor.tex(), r"\begin{pmatrix}a & b \\  &  \end{pmatrix}");
        editor.apply(Command::Backspace);
        assert_eq!(editor.tex(), r"\begin{pmatrix}a & b \end{pmatrix}");
        assert_eq!(editor.cursor().index, 1);
    }

    #[test]
    fn typing_a_command_asks_for_a_relayout_without_changing_the_formula() {
        let mut editor = Editor::from_tex("x+").expect("parses");
        for ch in ['\\', 'i', 'n', 't'] {
            let changed = editor.apply(Command::Insert(ch));
            assert!(changed.relayout, "{ch:?} left the screen stale");
            assert!(!changed.tree, "{ch:?} claimed to change the formula");
            assert_eq!(editor.tex(), "x+");
        }
        let mut abandoned = editor.clone();
        let changed = abandoned.apply(Command::Escape);
        assert!(changed.relayout && !changed.tree);
        assert_eq!(abandoned.tex(), "x+");

        let changed = editor.apply(Command::Submit);
        assert!(changed.relayout && changed.tree);
        assert_eq!(editor.tex(), r"x+\int");

        let changed = editor.apply(Command::Move(Motion::Left));
        assert!(!changed.relayout && changed.cursor);
    }

    #[test]
    fn a_selection_can_be_read_out_as_latex() {
        let mut editor = Editor::from_tex(r"x+\frac{a}{b}").expect("parses");
        assert_eq!(editor.selected_tex(), None, "nothing is selected yet");

        editor.apply(Command::Extend(Motion::Left));
        assert_eq!(editor.selected_tex().as_deref(), Some(r"\frac{a}{b}"));

        editor.apply(Command::SelectAll);
        assert_eq!(editor.selected_tex().as_deref(), Some(r"x+\frac{a}{b}"));

        let copied = editor.selected_tex().expect("a selection");
        let mut other = Editor::new();
        other.apply(Command::InsertText(copied));
        assert_eq!(other.tex(), editor.tex());
    }

    #[test]
    fn a_matrix_grows_by_cell_and_by_row() {
        let mut editor = Editor::from_tex(r"\begin{pmatrix} a \\ b \end{pmatrix}").expect("parses");
        editor.set_cursor(Cursor::at(
            vec![Step::new(0, SlotName::Cell { row: 0, col: 0 })],
            1,
        ));
        editor.apply(Command::Insert('&'));
        editor.apply(Command::Insert('x'));
        assert_eq!(editor.tex(), r"\begin{pmatrix}a & x \\ b \end{pmatrix}");

        editor.apply(Command::NewRow);
        editor.apply(Command::Insert('y'));
        assert_eq!(
            editor.tex(),
            r"\begin{pmatrix}a & x \\ y &  \\ b \end{pmatrix}"
        );
    }

    #[test]
    fn an_ampersand_outside_a_matrix_is_a_character() {
        let mut editor = typed("a&b");
        assert_eq!(editor.tex(), r"a\&b");
        assert!(crate::parse(&editor.tex()).is_ok());
        assert!(!editor.apply(Command::NewRow).tree);
    }

    #[test]
    fn pasting_latex_reads_it_rather_than_spelling_it() {
        let mut editor = Editor::new();
        editor.apply(Command::InsertText(r"\frac{a}{b}".into()));
        assert_eq!(editor.tex(), r"\frac{a}{b}");
        editor.apply(Command::Insert('c'));
        assert_eq!(editor.tex(), r"\frac{a}{b}c");
    }

    #[test]
    fn pasting_something_that_is_not_latex_goes_in_literally() {
        let mut editor = Editor::new();
        editor.apply(Command::InsertText("xy".into()));
        assert_eq!(editor.tex(), "xy");
        let mut editor = Editor::new();
        editor.apply(Command::InsertText(r"\notacommand".into()));
        assert!(crate::parse(&editor.tex()).is_ok());
    }

    #[test]
    fn every_prefix_of_a_typed_formula_still_parses() {
        for source in [
            r"x+y/2",
            r"(a+b)^2",
            r"\frac ab",
            r"\sqrt x+1",
            r"\sum_n^2 x",
            r"a_i^j",
        ] {
            let mut editor = Editor::new();
            for ch in source.chars() {
                editor.apply(Command::Insert(ch));
                let tex = editor.tex();
                assert!(
                    crate::parse(&tex).is_ok(),
                    "typing {source:?} produced {tex:?}, which does not parse"
                );
            }
        }
    }

    #[test]
    fn every_deletion_leaves_a_formula_that_parses() {
        for source in [r"\frac{a}{b}", r"x^2_i", r"\sqrt[3]{x}", r"\left(a\right)"] {
            let mut editor = Editor::from_tex(source).expect("parses");
            for _ in 0..40 {
                editor.apply(Command::Backspace);
                let tex = editor.tex();
                assert!(
                    crate::parse(&tex).is_ok(),
                    "deleting from {source:?} produced {tex:?}, which does not parse"
                );
            }
            assert_eq!(editor.tex(), "");
        }
    }
}
