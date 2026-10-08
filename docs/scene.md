# The scene

[← README](../README.md)

`layout` produces positions, not pixels:

```eeltex
\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
```

```rust
pub struct Scene {
    pub glyphs: Vec<GlyphInstance>,  // { glyph, font, x, y, size }
    pub rules: Vec<Rule>,            // fraction bars, radical rules, \overline
    pub backgrounds: Vec<Rule>,      // \colorbox fills: draw these first
    pub strokes: Vec<Stroke>,        // \cancel lines: Stroke::corners() is a quad
    pub width: f32,
    pub ascent: f32,                 // above the baseline
    pub descent: f32,                // below it
    pub font_size: f32,
    pub missing: Vec<char>,          // diagnostics; never fatal by default
}
```

Coordinates are pixels, y **down**, origin at the left edge on the baseline.

Draw `backgrounds` before everything else, then the rest in any order. A
background comes from `\colorbox`, a `formatting` command, and is only drawn
with `attrs`, which carries its colour; without `attrs` it lays out its padding
and draws no fill.

Layout is exactly linear in the font size, so `Scene::scaled(k)` equals a
second layout at `k` times the size. Zooming needs no relayout.

The default build has no colour, transforms, z-order or identity. The `attrs`
feature adds them: one field on each item and two tables on the scene.

## Identity

With `attrs`, every sub-expression becomes a `Node`, and every glyph and rule
records the innermost node that produced it. Nodes live in a flat arena, parents
before children.

```rust
let node = scene.hit(x, y)?;                    // innermost node under the pointer
scene.set_paint(node, Paint::rgb(255, 200, 0)); // tint it; no relayout
for glyph in scene.glyphs_of(node) { /* … */ }
```

## Spans

Spans map a byte offset in the source to the glyphs it produced, and back. They
are off by default because they cost an allocation per atom.

```rust
let expr = eeltex::parse_with(src, &ParseOptions::default().with_spans(true))?;
let scene = eeltex::layout(&expr, &fonts, &opts)?;
let node = scene.node_at_offset(caret)?;       // what is the caret inside?
let span = scene.node(scene.hit(x, y)?)?.span; // what did I click on?
```

## Colour

`\color{spec}` runs to the end of its group. `\textcolor{spec}{…}` takes its
body as an argument. `spec` is one of the nineteen `color` package names,
`#rgb`, `#rrggbb`, `#rrggbbaa` or `rgb(r, g, b)`. Anything else is a parse
error. Colour never changes geometry.
