# Demos

| | |
| --- | --- |
| [`egui/`](egui) | Benchmark: how many times per second a formula can be re-rendered (parse, layout, atlas) |

Each demo is its own cargo workspace, so no demo dependency can reach the
library.

```sh
cd egui && cargo run --release          # add `-- --bench 10` for a timed run
```
