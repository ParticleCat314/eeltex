# Demos

| | |
| --- | --- |
| [`egui/`](egui) | Benchmark: how many times per second a formula can be re-rendered (parse, layout, atlas) |
| [`wasm/`](wasm) | The browser build: a plain `wasm32-unknown-unknown` module with a C ABI, one `batch` call for a page of formulas |

Each demo is its own cargo workspace, so no demo dependency can reach the
library.

```sh
cd egui && cargo run --release          # add `-- --bench 10` for a timed run
```
