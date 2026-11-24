# Paper vs Chunkwake: p99 MSPT / TPS

Raw data: `target\ab-p99`; rounds: [1, 2, 3, 4, 5].

`p99` and all percentiles are computed from raw `ServerTickEndEvent` samples; `TPS` uses `server_ticks` (Server.getCurrentTick delta) divided by wall-clock length of the same window.

## Per-round windows

| round | side | scenario | events | server ticks | p50 ms | p95 ms | p99 ms | max ms | TPS (window) | TPS API |
|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | Chunkwake | cows200 | 300 | 301 | 0.437 | 0.653 | 0.931 | 3.280 | 20.02 | 20.00 |
| 1 | Chunkwake | cows1000 | 300 | 300 | 0.353 | 0.461 | 0.529 | 0.644 | 19.95 | 20.00 |
| 1 | Chunkwake | redstone | 323 | 323 | 0.360 | 0.473 | 0.574 | 0.638 | 20.00 | 20.00 |
| 1 | Paper | cows200 | 301 | 301 | 1.771 | 2.052 | 2.408 | 3.250 | 20.01 | 19.92 |
| 1 | Paper | cows1000 | 302 | 302 | 7.115 | 9.018 | 10.037 | 22.049 | 20.05 | 20.00 |
| 1 | Paper | redstone | 324 | 324 | 6.771 | 7.590 | 8.433 | 23.139 | 20.01 | 20.00 |
| 2 | Chunkwake | cows200 | 301 | 301 | 0.469 | 0.684 | 0.831 | 0.886 | 20.00 | 20.00 |
| 2 | Chunkwake | cows1000 | 300 | 300 | 0.498 | 0.655 | 0.856 | 0.878 | 19.95 | 20.00 |
| 2 | Chunkwake | redstone | 324 | 324 | 0.498 | 0.696 | 0.874 | 2.002 | 19.96 | 20.00 |
| 2 | Paper | cows200 | 302 | 302 | 2.159 | 2.904 | 3.167 | 3.711 | 20.05 | 19.82 |
| 2 | Paper | cows1000 | 301 | 300 | 8.022 | 8.993 | 9.576 | 20.866 | 19.96 | 20.00 |
| 2 | Paper | redstone | 325 | 325 | 7.809 | 8.490 | 9.079 | 22.686 | 20.01 | 20.00 |
| 3 | Chunkwake | cows200 | 301 | 301 | 0.413 | 0.568 | 0.641 | 1.343 | 20.00 | 20.00 |
| 3 | Chunkwake | cows1000 | 301 | 301 | 0.399 | 0.570 | 0.635 | 0.773 | 20.00 | 20.00 |
| 3 | Chunkwake | redstone | 323 | 323 | 0.421 | 0.572 | 0.657 | 0.800 | 20.00 | 20.00 |
| 3 | Paper | cows200 | 300 | 300 | 2.220 | 4.197 | 10.695 | 24.638 | 19.95 | 19.92 |
| 3 | Paper | cows1000 | 301 | 301 | 7.487 | 9.439 | 11.415 | 21.869 | 20.00 | 20.00 |
| 3 | Paper | redstone | 324 | 324 | 7.764 | 9.898 | 14.090 | 56.995 | 20.00 | 20.00 |
| 4 | Chunkwake | cows200 | 301 | 301 | 0.389 | 0.533 | 0.562 | 0.632 | 20.00 | 20.00 |
| 4 | Chunkwake | cows1000 | 301 | 301 | 0.396 | 0.529 | 0.581 | 1.539 | 20.00 | 20.00 |
| 4 | Chunkwake | redstone | 325 | 325 | 0.381 | 0.513 | 0.595 | 0.630 | 20.00 | 20.00 |
| 4 | Paper | cows200 | 301 | 300 | 2.184 | 2.725 | 3.371 | 9.472 | 19.94 | 19.92 |
| 4 | Paper | cows1000 | 301 | 301 | 7.118 | 7.764 | 8.085 | 12.772 | 20.01 | 20.00 |
| 4 | Paper | redstone | 325 | 325 | 6.740 | 7.444 | 8.130 | 20.337 | 20.00 | 20.00 |
| 5 | Chunkwake | cows200 | 301 | 301 | 0.362 | 0.539 | 1.276 | 11.694 | 20.00 | 20.00 |
| 5 | Chunkwake | cows1000 | 301 | 301 | 0.327 | 0.474 | 0.512 | 0.575 | 20.00 | 20.00 |
| 5 | Chunkwake | redstone | 325 | 325 | 0.294 | 0.470 | 0.774 | 2.856 | 19.99 | 20.00 |
| 5 | Paper | cows200 | 300 | 300 | 2.353 | 2.748 | 3.041 | 3.160 | 19.94 | 19.86 |
| 5 | Paper | cows1000 | 301 | 301 | 8.202 | 9.369 | 10.115 | 11.245 | 20.00 | 20.00 |
| 5 | Paper | redstone | 325 | 325 | 8.098 | 13.120 | 33.197 | 45.779 | 20.00 | 20.00 |

## Pooled windows

| scenario | side | rounds | ticks | avg ms | p50 ms | p95 ms | p99 ms | p99 round median | max ms | TPS (pooled window) | TPS min | TPS API mean |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | Chunkwake | 5 | 1504 | 0.437 | 0.413 | 0.606 | 0.842 | 0.831 | 11.694 | 20.00 | 20.00 | 20.00 |
| cows200 | Paper | 5 | 1504 | 2.257 | 2.151 | 2.891 | 4.497 | 3.167 | 24.638 | 19.98 | 19.94 | 19.89 |
| cows1000 | Chunkwake | 5 | 1503 | 0.400 | 0.389 | 0.577 | 0.678 | 0.581 | 1.539 | 19.98 | 19.95 | 20.00 |
| cows1000 | Paper | 5 | 1506 | 7.749 | 7.644 | 9.047 | 10.500 | 10.037 | 22.049 | 20.00 | 19.96 | 20.00 |
| redstone | Chunkwake | 5 | 1620 | 0.401 | 0.387 | 0.584 | 0.740 | 0.657 | 2.856 | 19.99 | 19.96 | 20.00 |
| redstone | Paper | 5 | 1623 | 7.713 | 7.444 | 9.192 | 16.779 | 9.079 | 56.995 | 20.00 | 20.00 | 20.00 |

## Paired p99 and TPS deltas (Chunkwake - Paper)

| scenario | pairs | p99 delta ms (mean / median) | p99 CW wins | p99 p(CW better) | p99 p(Paper better) | pooled p99 CW / Paper | p99 ratio | TPS delta (mean) | TPS CW wins | TPS p(CW better) | TPS p(Paper better) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | 5 | -3.688 / -2.337 | 5/5 | 0.031 | 1.000 | 0.842 / 4.497 | 0.187x | 0.03 | 4/5 | 0.188 | 0.969 |
| cows1000 | 5 | -9.223 / -9.507 | 5/5 | 0.031 | 1.000 | 0.678 / 10.500 | 0.065x | -0.02 | 0/5 | 1.000 | 0.125 |
| redstone | 5 | -13.891 / -8.206 | 5/5 | 0.031 | 1.000 | 0.740 / 16.779 | 0.044x | -0.01 | 0/5 | 1.000 | 0.125 |

Negative p99 delta means Chunkwake had lower p99. Ties are excluded from the sign test.
The table gives both one-sided p-values: `p(CW better)` tests `Chunkwake < Paper`; `p(Paper better)` tests the opposite direction.

`TPS (window)` uses the authoritative `server_ticks` delta from `Server.getCurrentTick()`, not the number of Java tick events. `TPS API` from `Server.getTPS()[0]` is shown as a cross-check (Chunkwake: last-100-tick meter, Paper: 1-minute average).

Chunkwake tick duration = Rust core tick + Java scheduler/plugin tick for the same tick; Paper tick duration is the full server tick measured by Paper.
