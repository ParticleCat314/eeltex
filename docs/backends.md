# Backends

[← README](../README.md)

Every backend lives under `eeltex::export` behind its own feature. None is
required: walk `scene.glyphs` and call `fonts.math.face().outline_glyph(…)` to
write your own, which is what `to_svg` does.

## SVG

```rust
use eeltex::export::svg::{to_svg, SvgOptions};

let scene = eeltex::render(r"\frac{a}{b}", &fonts, &Options::default())?;
let text = to_svg(&scene, &fonts, &SvgOptions::default())?;
```

`fonts` must be the faces the scene was laid out against: a `GlyphInstance`
names a glyph by index.

The output is one `<path>` per glyph in absolute coordinates, with no `<defs>`,
`<use>` or `transform`. A `<use>` cannot be scaled without a wrapping
`<g transform>`, which costs more bytes than it saves on a short formula.
Absolute coordinates also keep golden-file diffs readable.

`SvgOptions::fill` defaults to `None`, which emits `currentColor`. `background`
defaults to `None`, which leaves the document transparent.

### Many formulas on one page

`SvgSprite` is for a list or table of formulas. Each distinct glyph outline is
written once, in font units, and every formula becomes a small `<svg>` of
`<use>` references into that table:

```rust
use eeltex::export::svg::{SpriteOptions, SvgSprite};

let mut sprite = SvgSprite::new(SpriteOptions::default());
let a = sprite.fragment(&scene_a, &fonts)?;
let b = sprite.fragment(&scene_b, &fonts)?;
page.push_str(&sprite.take_defs()); // the outlines used so far, once
page.push_str(&a);
page.push_str(&b);
```

`take_defs` returns only what is new since the last call, so a page that renders
rows as they scroll into view appends to one hidden `<svg><defs>` and never
rebuilds it. With `inline` (the default) each fragment carries
`style="vertical-align:…"` so it sits on the text baseline. The references are
by id, so the defs and the fragments must be in the same document; `prefix`
keeps two sprites apart.

`demos/wasm` wraps this as a C-ABI WebAssembly module: `batch` takes many
formulas in one call and returns the fragments plus the new defs.

## Coverage or distance field

Both fill the same `Atlas` and produce the same `Quad`s:

```rust
let mut atlas = Atlas::new(512, 2048, Source::Coverage);
let mut atlas = Atlas::new(512, 2048, Source::signed_distance());
```

| | coverage (`raster`) | distance field (`sdf`) |
| --- | --- | --- |
| Atlas entries | One per glyph per size. Scripts alone give up to three per character | One per glyph, for every size and zoom |
| Strokes | Exact at the baked size | Hairlines and stretched-arrow shafts look softer |
| Corners | Exact | Round off past about 8× magnification |
| Effects | Colour only | Outline, bold, glow, dissolve |

Use a field for anything that zooms, warps or glows. Use coverage for maths at
a fixed size.

The distance-field fragment shader is:

```glsl
float d = texture(atlas, uv).r;
float w = fwidth(d);
float a = smoothstep(0.5 - w, 0.5 + w, d);
```

Moving the threshold off `0.5` gives outlines and bolding.

Each part of an assembled glyph is its own quad, and two fields do not blend
across a seam, so the assembler overlaps the parts by a tenth of an em.

`examples/sdf_render.rs` runs the field path on the CPU: it lays a formula out
at 18 pixels, packs the fields, and draws it magnified.

```sh
cargo run --example sdf_render --features atlas,sdf,embed-math-font -- \
    '\sum_{n=1}^{\infty} \frac{1}{n^2}' out.pgm 8
```

## egui

Upload the atlas as a texture and push quads:

```rust
atlas.prepare(&scene, &fonts);
if atlas.take_dirty() { /* upload atlas.data(), 8-bit, atlas.width() x atlas.height() */ }

let mut mesh = egui::Mesh::with_texture(texture);
for glyph in &scene.glyphs {
    if let Some(q) = atlas.quad(glyph) {
        mesh.add_rect_with_uv(rect(origin, q.min, q.max), uv(q), colour);
    }
}
for rule in &scene.rules { painter.rect_filled(/* … */); }
for stroke in &scene.strokes { /* stroke.corners(): a quad over atlas.solid_uv() */ }
```

Fill `scene.backgrounds` before the glyphs; they are `\colorbox` fills.
