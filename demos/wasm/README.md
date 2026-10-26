# eeltex in the browser

A `wasm32-unknown-unknown` module with a C ABI: no `wasm-bindgen`, no JavaScript glue
beyond the thirty lines below. The embedded New Computer Modern Math makes it about
1.1 MB; load it once and keep it.

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown --manifest-path demos/wasm/Cargo.toml
# → demos/wasm/target/wasm32-unknown-unknown/release/eeltex_wasm.wasm
```

## Exports

| export | |
| --- | --- |
| `memory` | the module's linear memory |
| `in_ptr(len) -> ptr` | a buffer of `len` bytes to write UTF-8 input into |
| `out_ptr()`, `out_len()` | the result of the last call, UTF-8 |
| `init(font_px) -> 0 \| 1` | load the font and set the pixel size; `0` on failure, with the error in the output |
| `render(len, display) -> 0 \| 1` | one formula from the input buffer → a standalone SVG document (`to_svg`), or the error message |
| `batch(len, display) -> n` | many formulas, separated by `\x1e`, through one `SvgSprite` → `n` fragments separated by `\x1e`, each `<svg>…</svg>` or `\x1f` + error, then `\x1d` + the new `<defs>` (possibly empty) |
| `reset()` | forget the sprite's glyph table, e.g. after the page removed its `<defs>` |

`batch` is the point: a page that shows a list of formulas appends the returned
`<defs>` block to the document once, then drops the fragments into the rows. Every
glyph outline is sent once per page rather than once per formula.

## JavaScript

```js
const { instance } = await WebAssembly.instantiateStreaming(fetch('eeltex_wasm.wasm'), {});
const w = instance.exports, enc = new TextEncoder(), dec = new TextDecoder();
const put = s => { const b = enc.encode(s); new Uint8Array(w.memory.buffer, w.in_ptr(b.length), b.length).set(b); return b.length; };
const get = () => dec.decode(new Uint8Array(w.memory.buffer, w.out_ptr(), w.out_len()));
if (!w.init(16)) throw new Error(get());

const svg = w.render(put('\\frac{a}{b}'), 0) ? get() : null;          // one formula, text style

const out = get.call(null, w.batch(put(['x^2', '\\sqrt{2}'].join('\x1e')), 0)) ?? get();
const [body, defs] = out.split('\x1d');
const fragments = body.split('\x1e');                                   // one per formula, in order
if (defs) document.body.insertAdjacentHTML('beforeend', defs);         // the shared outlines
```

`in_ptr` returns a buffer that is reused by the next call, so copy the input before
calling again; `out_ptr`/`out_len` likewise describe one result at a time.
