# Editing

[← README](../README.md)

`partial` repairs a formula that is still arriving as text. `edit` changes a
formula from inside the rendered view.

## `partial::repair`

Almost every prefix of a valid formula is invalid: `\frac{a}` has no
denominator, `\left(` has no `\right`.

```rust
let repaired = eeltex::partial::repair(r"\frac{-b \pm \sqrt{b^2");
assert_eq!(repaired, r"\frac{-b \pm \sqrt{b^2}}{}");
```

The result always parses. `repair` closes what is open and adds empty arguments
to a command that wants more. If that fails, it drops the last token and tries
again, back to the empty string.

## The `edit` feature

Allows you to edit a formula in a similar way to mathquill. The demo page uses this.

```rust
use eeltex::edit::{Command, Editor};

let mut editor = Editor::new();
editor.apply(Command::Insert('x'));
editor.apply(Command::Insert('/'));   // wraps the `x` into a numerator
editor.apply(Command::Insert('2'));
assert_eq!(editor.tex(), r"\frac{x}2");
```

### The tree

The state is a tree of `edit::Slot` and `edit::Item`, where every child
position is a list. `Expr` cannot hold "a superscript exists and is empty",
which is the state just after `^` is pressed. `Expr` is the lowering target,
rebuilt per keystroke.

A `Cursor` is a path of named steps (`Num`, `Sup`, `Cell { row, col }`) and an
index, so navigation needs no font or layout pass. Walking right from the start
visits every position exactly once.

Nesting stops at `edit::tree::MAX_DEPTH`, because the parser has a depth limit
too: a deeper tree would write `tex()` that cannot be read back.

### Keystrokes

The keystrokes follow MathQuill.

| key | effect |
| --- | --- |
| `/` | Takes the operand before it: `x+y/` is `x+\frac{y}{}`, `f(x)/` is `\frac{f(x)}{}` |
| `^` `_` | Attach to what is before them. On a big operator they fill a limit |
| Backspace after a fraction | Enters it |
| Backspace at the start of a fraction | Unwraps it and keeps the parts |
| `&` in an environment | Adds a cell. Elsewhere it is a literal ampersand |
| `\` | Starts a raw command; see below |
| Enter (`Command::Submit`) | Submits a raw command |
| `Command::NewRow` | Adds a row |
| Backspace at the start of a cell | Joins it to the cell before. At the first cell, unwraps the environment |

### Aliases

A run of letters that spells a common command becomes it on the last letter:
`sqrt`, `sum`, `prod`, `int`, `oint`, `lim`, `infty`, the Greek letters, and
`sin`, `cos`, `log`, `ln`, `exp` and the other function names. Only a whole run
counts: `2alpha` is `2\alpha`, `point` stays five letters. No alias is a prefix
of another, so none fires early; `sinh` and `limsup` are left out for that
reason.

### Raw commands

`\` starts a raw command. Every key after it is part of the command, braces and
spaces included, until `Command::Submit` (Enter) ends it. Nothing is inferred
while it is typed, so `\frac{a}{b}` is typed exactly as written.

On submit, a bare name is offered to the parser as `\begin{name}\end{name}`,
then `\name`, `\name{}`, `\name{}{}` …, and the first it accepts is inserted
with the caret in its first empty slot. Anything else is parsed as written.

A raw command in progress is drawn dimmed where it is typed and is not in
`tex()`. Backspace removes its last character, then the backslash; Escape
abandons it. `Changed::relayout` is set when the screen should change.
`Changed::tree` is set only when the formula changes.

A command the parser refuses is kept as red text,
`\textcolor{#d02020}{\text{\name}}`, with its braces balanced, so a typo stays
visible and survives a round trip through LaTeX. `Editor::unknown_commands`
lists them.

A red command is edited as a raw command again, never stepped into. Arrowing
into it, clicking it, or Delete before it reopens it as typed; Backspace after
it reopens it one character shorter. Escape, or a click elsewhere, puts it back
unchanged, and one undo undoes a reopen and its resubmission together. A
selection takes it whole.

While a raw command is open the caret stays put: arrows do nothing until Enter
or Escape.

### Paste and copy

`Command::InsertText` is read as LaTeX if it parses, otherwise as characters.
`Editor::selected_tex` returns the selection as LaTeX. The editor does not touch
the clipboard.

### Placeholders and the caret

An empty slot is lowered as a tinted `□`, so it has a box to click into.
`Editor::is_placeholder` identifies those glyphs.

```rust
let mut scene = editor.layout(&fonts, &options)?;
if let Some(caret) = editor.caret(&scene) {
    scene.rules.push(caret.into_rule());   // every backend already draws rules
}
```

### Identity

Every slot and item carries an `EditId` that survives any edit that does not
destroy it, re-parenting included. Lowering writes those ids into `Span`s, and
layout copies them onto nodes, so `Scene::node_at_offset` maps an id to its
node. A glyph matcher that pairs by span therefore pairs by identity, and a
formula can slide to make room for a character instead of flickering.

A span on an edited scene is therefore an id, not a byte range.
`EditOptions::with_spans(SpanMode::Bytes)` restores byte ranges; `caret`,
`cursor_at` and `node_of` then return `None`. Check with
`Editor::ids_in_spans`.

### Selection

A `Selection` is a range of whole items within one slot, so shift-arrowing out
of a numerator selects the whole fraction. `Selection::nodes` returns its
`Vec<NodeId>`.

## Highlighting

`syntax` (no other feature) exposes the tokenizer, so an editor colours a
formula with the same grammar that renders it:

```rust
use eeltex::syntax::{highlight, TokenKind};

for (span, kind) in highlight(r"\frac{a}{b}") {
    // span.start .. span.end are byte offsets into the source
}
```

Spans skip whitespace; paint the gaps as plain text. `highlight` also parses,
and marks the token the parser rejected as `TokenKind::Error`: `\intt`
tokenizes like any command, and only the parser knows it is a typo. If you have
already parsed, pass the error to `highlight_with`.
