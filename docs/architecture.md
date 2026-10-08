# Architecture

[← README](../README.md)

## Pipeline

1. **Parsing** (`src/parsing/`): `parse`, `parse_with`, `partial::repair`.
   Tokenizer, parser, and the symbol, class and colour tables.
2. **Model** (`src/model/`): the parsed [`Expr`](../src/model/ast.rs), the
   laid-out [`Scene`](../src/model/scene.rs), fonts, styles, TeX writing.
3. **Layout** (`src/layout/`): turns an `Expr` into positioned glyphs and rules.
4. **Export** (`src/export/`): SVG, the two bitmap backends, the atlas.
5. **Editing** (`src/edit/`): see [Editing](editing.md).

The file tree is the module tree: no `#[path]` attributes.

`model` and `parsing` are private. `lib.rs` aliases their contents to the crate
root, so internal code writes `crate::scene::Scene`. The two public modules
inside them, `attrs` and `partial`, are re-exported at the root.

## Public API

Every public item has exactly one path.

- The root is the unfeatured crate: `parse`, `parse_with`, `ParseOptions`,
  `layout`, `render`, `Options`, `MissingGlyph`, `Expr` and the AST enums,
  `Scene`, `GlyphInstance`, `Rule`, `Fonts`, `MathStyle`, `AtomClass`, `Error`,
  `to_tex`.
- Feature-gated items sit under a namespace: `eeltex::export`,
  `eeltex::attrs`, `eeltex::edit`.
- `eeltex::partial` needs no feature.

## Scene core and `attrs`

The default scene is geometry only: glyphs, rules, width, ascent, descent, font
size and missing-glyph diagnostics.

`attrs` adds what an interactive renderer needs: node ids, source spans, node
boxes, paint tables and per-item opacity. A static renderer pays only for
geometry.

## Backends

`outline` is the shared base: contour flattening and point sampling. `raster`
and `sdf` build on it. `atlas` packs either into a texture and turns a `Scene`
into quads. `svg` writes outlines as path data and rasterises nothing.

There are no file exporters. A `Scene` plus
`fonts.math.face().outline_glyph(…)` is enough to write a PDF or PNG.
