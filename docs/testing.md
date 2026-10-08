# Testing

[← README](../README.md) · commands are under
[Building and testing](../README.md#building-and-testing).

| test | checks |
| --- | --- |
| `tests/parser.rs` | A corpus of real OCR output, and constructs that must be rejected |
| `tests/robustness.rs` | Hostile input never panics; see below |
| `tests/layout.rs` | Font-independent layout invariants |
| `tests/constructs.rs`, `tests/additions.rs` | The shape of each construct |
| `tests/formatting.rs` | The `formatting` feature |
| `tests/golden.rs` | SVG snapshots. Re-bless with `EELTEX_BLESS=1` |
| `tests/attrs.rs` | The node tree matches the source |
| `tests/tex.rs` | `to_tex` round-trips |
| `tests/symbols.rs` | Every symbol in the table resolves to a glyph |
| `tests/namespaces.rs` | Each public item sits at its documented path |
| `tests/edit.rs` | Editor identity survives edits |

## Robustness

Two thousand strings from a fixed seed, weighted towards characters the
tokenizer has state for, plus known parser-breakers: truncated commands,
unbalanced braces, a stray `\end`, NUL, a byte-order mark, astral-plane
characters. It asserts:

- nothing panics in parsing or layout,
- a parse error's offset is inside the source and on a character boundary,
- `partial::repair` is total and idempotent,
- a scene's box is finite at a font size of `NaN`,
- no non-finite number reaches an SVG document,
- bitmap buffers match their dimensions at every size and spread.

It is deterministic and runs on stable in seconds, so it runs on every commit.

## Layout invariants

- the fraction bar sits between the parts,
- a script sits above the baseline and is smaller,
- a bracket grows only when it has to,
- an integral's subscript tucks under its bowl,
- layout is exactly linear in the font size.

## Golden files are SVG only

The SVG writer rounds coordinates and normalises `-0`, so the bytes should match
across platforms. The raster and SDF bakes go through `f32::hypot`, whose last
bit depends on libm. Assert properties of bitmaps, never their bytes:

- a counter stays hollow,
- the distance field's sign is right,
- the edge sits at the same fraction of an em at every sample resolution,
- an atlas quad lands where blitting the bitmap by hand would put it.
