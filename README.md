# eeltex

Minimal TeX-style math formula typesetting in Rust. Why? Because I couldn't find something that I was happy with.
This project does not attempt, and will likely never attempt to be a rigourous and 100% faithful implementation of TeX.
Nor does it implement the majority of features required of a robust typesetting system. Look elsewhere if that is what you need.
My goal with this project was simple: *LaTeX math string to pretty math without all the dependencies".

- One dependency: [`ttf-parser`].

- Should be fast enough to re-lay-out on every keystroke. 

- Includes an optional feature for mathquill style live editing.

- Supports building for wasm. There is a web-demo at particlecat314.github.io/eeltex

```rust
use eeltex::{Fonts, Options};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("newcm-math.otf")?;
    let fonts = Fonts::new(&bytes)?;
    let options = Options::default().with_font_size(32.0);

    let scene = eeltex::render(r"\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}", &fonts, &options)?;
    for glyph in &scene.glyphs {
        println!("{:?} at ({}, {})", glyph.glyph, glyph.x, glyph.y);
    }
    Ok(())
}
```

## Features

`default = []` gives parse → layout → `Scene`. Everything else is optional and
adds no dependencies.

| feature | what it gives you |
| --- | --- |
| `svg` | `export::svg::to_svg`: a standalone SVG document |
| `sdf` | `export::sdf::sdf_glyph`: a signed distance field per glyph, sharp at every size |
| `raster` | `export::raster::rasterise_glyph`: an anti-aliased 8-bit coverage bitmap |
| `atlas` | `export::atlas::Atlas`: packs either bitmap backend into one texture and turns a `Scene` into quads |
| `attrs` | The node arena, source spans, hit-testing and per-item colour |
| `edit` | `edit::Editor`: a mutable formula tree, a caret and MathQuill-style keystrokes. See [Editing](docs/editing.md) |
| `syntax` | `syntax::highlight`: the tokenizer, for colouring a formula as it is typed |
| `formatting` | Boxes, colour boxes, cancels, `\rule` and struts, `\smash` and the laps, `\raisebox`, `\tiny` … `\Huge`, and `array`'s `\hline` and column rules |
| `embed-math-font` | `Fonts::embedded()`, with New Computer Modern Math built in |


## Documentation

| | |
| --- | --- |
| [The scene](docs/scene.md) | Coordinates, the node arena, hit-testing, spans, colour |
| [Backends](docs/backends.md) | SVG, coverage, distance fields, the atlas |
| [Editing](docs/editing.md) | `partial::repair`, the `edit` and `syntax` features |
| [Fonts](docs/fonts.md) | What a `MATH` table has to provide |
| [Architecture](docs/architecture.md) | The pipeline and the layer boundaries |
| [Testing](docs/testing.md) | What each tier of test is for |
| [Supported LaTeX](docs/supported.md) | Currently supported LaTeX commands|

## Building and testing

```sh
cargo test                              # parser only; no font needed
cargo test --all-features              # the full suite
cargo clippy --all-features --all-targets -- -D warnings
cargo build --target wasm32-unknown-unknown --all-features
```

CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) also builds each
feature on its own and checks the MSRV.

```sh
cargo run --example render_svg   --features svg,embed-math-font          -- 'x^2 + \frac{a}{b}' out.svg
cargo run --example raster_check --features raster,embed-math-font       -- '\sqrt{\pi}' out.pgm
cargo run --example sdf_render   --features atlas,sdf,embed-math-font    -- '\sqrt{\pi}' out.pgm 8
```

A re-render benchmark is in [`demos/egui`](demos/egui):

```sh
cd demos/egui && cargo run --release
```

Library code denies `unsafe_code`, `unwrap_used`, `expect_used`, `panic` and
`indexing_slicing`: a malformed formula returns an error, it does not panic.

## Useful resources/references
- Knuth, Donald Ervin, 1938 - The TeXbook. ISBN: 0-201-13447-0


## Statement of Disclosure
A significant portion of this project was created with the aid of various LLM-based tools. 
Any LLM output has been, and will continue to be, extensively reviewed by a human.

## License

MIT. The bundled New Computer Modern Math font is under the
[GUST Font License](assets/GUST-FONT-LICENSE.txt).

[`ttf-parser`]: https://github.com/RazrFalcon/ttf-parser
