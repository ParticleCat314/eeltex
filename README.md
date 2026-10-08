# eeltex demo page

The GitHub Pages site for [eeltex](https://github.com/ParticleCat314/eeltex): a
formula you can type into, rendered to SVG by the library compiled to WebAssembly, with its
node boxes, metrics and timings on display.

| | |
| --- | --- |
| `index.html`, `app.js` | The page. No build step, no dependencies |
| `wasm/` | A raw C-ABI wrapper over `eeltex::edit`; no `wasm-bindgen` |
| `.github/workflows/pages.yml` | Builds the module and deploys on every push to this branch |

The module depends on eeltex's `main` by git, pinned by `wasm/Cargo.lock`. Running the
workflow by hand builds against the newest `main`.

```sh
cargo build --release --target wasm32-unknown-unknown --manifest-path wasm/Cargo.toml
cp wasm/target/wasm32-unknown-unknown/release/eeltex_demo.wasm .
python3 -m http.server     # http://localhost:8000
```
