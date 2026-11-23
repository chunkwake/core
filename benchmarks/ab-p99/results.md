# Paper vs Chunkwake: p99 MSPT / TPS

Raw data: `target\ab-p99`; rounds: [1, 2, 3, 4, 5].

`p99` and all percentiles are computed from raw `ServerTickEndEvent` samples; `TPS` uses `server_ticks` (Server.getCurrentTick delta) divided by wall-clock length of the same window.

## Per-round windows

| round | side | scenario | events | server ticks | p50 ms | p95 ms | p99 ms | max ms | TPS (window) | TPS API |
|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | Chunkwake | cows200 | 300 | 300 | 0.621 | 0.814 | 0.907 | 3.729 | 19.96 | 20.00 |
| 1 | Chunkwake | cows1000 | 301 | 301 | 1.130 | 1.515 | 1.847 | 2.354 | 20.00 | 20.00 |
| 1 | Chunkwake | redstone | 323 | 323 | 1.180 | 1.472 | 1.631 | 1.869 | 20.04 | 20.00 |
| 1 | Paper | cows200 | 301 | 301 | 2.115 | 2.666 | 3.108 | 3.704 | 20.00 | 19.88 |
| 1 | Paper | cows1000 | 301 | 301 | 7.370 | 8.890 | 11.459 | 21.833 | 20.01 | 20.00 |
| 1 | Paper | redstone | 323 | 323 | 7.885 | 14.044 | 38.426 | 91.227 | 20.00 | 20.00 |
| 2 | Chunkwake | cows200 | 301 | 301 | 0.530 | 0.711 | 0.884 | 0.987 | 20.02 | 20.00 |
| 2 | Chunkwake | cows1000 | 301 | 301 | 1.142 | 1.428 | 1.638 | 2.035 | 20.00 | 20.00 |
| 2 | Chunkwake | redstone | 324 | 324 | 1.137 | 1.429 | 1.563 | 1.795 | 20.05 | 20.00 |
| 2 | Paper | cows200 | 301 | 301 | 2.118 | 3.078 | 4.150 | 47.142 | 20.00 | 19.84 |
| 2 | Paper | cows1000 | 301 | 301 | 7.555 | 8.641 | 10.455 | 21.978 | 20.00 | 20.00 |
| 2 | Paper | redstone | 323 | 323 | 8.895 | 17.646 | 28.904 | 56.149 | 20.00 | 20.00 |
| 3 | Chunkwake | cows200 | 301 | 301 | 0.545 | 0.757 | 0.928 | 1.442 | 19.99 | 20.00 |
| 3 | Chunkwake | cows1000 | 301 | 301 | 1.020 | 1.302 | 1.473 | 1.729 | 20.00 | 20.00 |
| 3 | Chunkwake | redstone | 324 | 324 | 1.053 | 1.315 | 1.435 | 1.492 | 19.95 | 20.00 |
| 3 | Paper | cows200 | 301 | 301 | 1.898 | 2.287 | 2.648 | 3.259 | 20.01 | 19.92 |
| 3 | Paper | cows1000 | 301 | 301 | 8.774 | 10.339 | 11.073 | 11.606 | 20.00 | 20.00 |
| 3 | Paper | redstone | 324 | 324 | 8.100 | 9.963 | 10.803 | 28.257 | 20.00 | 20.00 |
| 4 | Chunkwake | cows200 | 300 | 300 | 0.634 | 0.808 | 0.900 | 1.905 | 19.95 | 20.00 |
| 4 | Chunkwake | cows1000 | 301 | 301 | 1.133 | 1.383 | 1.460 | 1.655 | 20.00 | 20.00 |
| 4 | Chunkwake | redstone | 325 | 325 | 1.135 | 1.420 | 1.545 | 1.739 | 20.00 | 20.00 |
| 4 | Paper | cows200 | 300 | 300 | 2.311 | 2.806 | 3.208 | 3.678 | 19.96 | 19.90 |
| 4 | Paper | cows1000 | 301 | 301 | 8.236 | 17.275 | 41.582 | 64.211 | 20.00 | 20.00 |
| 4 | Paper | redstone | 324 | 324 | 8.415 | 10.384 | 12.254 | 28.075 | 20.00 | 20.00 |
| 5 | Chunkwake | cows200 | 301 | 301 | 0.580 | 0.775 | 0.945 | 6.220 | 20.01 | 20.00 |
| 5 | Chunkwake | cows1000 | 301 | 301 | 1.100 | 1.392 | 1.563 | 1.949 | 20.02 | 20.00 |
| 5 | Chunkwake | redstone | 324 | 324 | 1.089 | 1.376 | 1.530 | 2.257 | 19.99 | 20.00 |
| 5 | Paper | cows200 | 301 | 301 | 2.240 | 3.000 | 3.479 | 4.051 | 20.00 | 19.77 |
| 5 | Paper | cows1000 | 301 | 301 | 8.341 | 9.550 | 10.285 | 12.185 | 20.01 | 20.00 |
| 5 | Paper | redstone | 322 | 322 | 7.801 | 9.017 | 10.362 | 28.612 | 19.99 | 20.00 |

## Pooled windows

| scenario | side | rounds | ticks | avg ms | p50 ms | p95 ms | p99 ms | p99 round median | max ms | TPS (pooled window) | TPS min | TPS API mean |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | Chunkwake | 5 | 1503 | 0.607 | 0.583 | 0.790 | 0.928 | 0.907 | 6.220 | 19.99 | 19.95 | 20.00 |
| cows200 | Paper | 5 | 1504 | 2.216 | 2.122 | 2.811 | 3.347 | 3.208 | 47.142 | 20.00 | 19.96 | 19.86 |
| cows1000 | Chunkwake | 5 | 1505 | 1.126 | 1.106 | 1.411 | 1.642 | 1.563 | 2.354 | 20.00 | 20.00 | 20.00 |
| cows1000 | Paper | 5 | 1505 | 8.454 | 8.098 | 10.161 | 18.874 | 11.073 | 64.211 | 20.00 | 20.00 | 20.00 |
| redstone | Chunkwake | 5 | 1620 | 1.137 | 1.117 | 1.419 | 1.550 | 1.545 | 2.257 | 20.01 | 19.95 | 20.00 |
| redstone | Paper | 5 | 1616 | 8.870 | 8.148 | 11.676 | 26.213 | 12.254 | 91.227 | 20.00 | 19.99 | 20.00 |

## Paired p99 and TPS deltas (Chunkwake - Paper)

| scenario | pairs | p99 delta ms (mean / median) | p99 CW wins | p99 p(CW better) | p99 p(Paper better) | pooled p99 CW / Paper | p99 ratio | TPS delta (mean) | TPS CW wins | TPS p(CW better) | TPS p(Paper better) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | 5 | -2.406 / -2.309 | 5/5 | 0.031 | 1.000 | 0.928 / 3.347 | 0.277x | -0.01 | 2/5 | 0.812 | 0.500 |
| cows1000 | 5 | -15.375 / -9.600 | 5/5 | 0.031 | 1.000 | 1.642 / 18.874 | 0.087x | -0.00 | 1/5 | 0.750 | 0.750 |
| redstone | 5 | -18.609 / -10.709 | 5/5 | 0.031 | 1.000 | 1.550 / 26.213 | 0.059x | 0.01 | 2/5 | 0.500 | 0.875 |

Negative p99 delta means Chunkwake had lower p99. Ties are excluded from the sign test.
The table gives both one-sided p-values: `p(CW better)` tests `Chunkwake < Paper`; `p(Paper better)` tests the opposite direction.

`TPS (window)` uses the authoritative `server_ticks` delta from `Server.getCurrentTick()`, not the number of Java tick events. `TPS API` from `Server.getTPS()[0]` is shown as a cross-check (Chunkwake: last-100-tick meter, Paper: 1-minute average).

Chunkwake tick duration = Rust core tick + Java scheduler/plugin tick for the same tick; Paper tick duration is the full server tick measured by Paper.
