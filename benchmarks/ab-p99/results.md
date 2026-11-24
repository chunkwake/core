# Paper vs Chunkwake: p99 MSPT / TPS

Raw data: `target\ab-p99`; rounds: [1, 2, 3, 4, 5].

`p99` and all percentiles are computed from raw `ServerTickEndEvent` samples; `TPS` uses `server_ticks` (Server.getCurrentTick delta) divided by wall-clock length of the same window.

## Per-round windows

| round | side | scenario | events | server ticks | p50 ms | p95 ms | p99 ms | max ms | TPS (window) | TPS API |
|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | Chunkwake | cows200 | 301 | 301 | 0.392 | 0.508 | 0.541 | 0.570 | 20.01 | 20.00 |
| 1 | Chunkwake | cows1000 | 301 | 301 | 0.300 | 0.448 | 0.504 | 0.531 | 20.00 | 20.00 |
| 1 | Chunkwake | redstone | 323 | 323 | 0.303 | 0.463 | 0.520 | 0.560 | 20.00 | 20.00 |
| 1 | Paper | cows200 | 302 | 302 | 2.026 | 2.394 | 2.567 | 2.789 | 20.06 | 19.87 |
| 1 | Paper | cows1000 | 300 | 300 | 7.945 | 9.451 | 10.393 | 21.653 | 19.95 | 20.00 |
| 1 | Paper | redstone | 325 | 325 | 7.152 | 8.779 | 10.232 | 23.624 | 20.00 | 20.00 |
| 2 | Chunkwake | cows200 | 301 | 301 | 0.432 | 0.661 | 0.755 | 1.113 | 20.00 | 20.00 |
| 2 | Chunkwake | cows1000 | 301 | 301 | 0.492 | 0.744 | 0.864 | 12.638 | 20.02 | 20.00 |
| 2 | Chunkwake | redstone | 325 | 325 | 0.481 | 0.665 | 0.785 | 0.834 | 20.03 | 20.00 |
| 2 | Paper | cows200 | 301 | 301 | 2.264 | 3.081 | 4.584 | 28.628 | 20.02 | 19.85 |
| 2 | Paper | cows1000 | 301 | 301 | 8.544 | 10.824 | 12.720 | 26.424 | 20.00 | 20.00 |
| 2 | Paper | redstone | 325 | 325 | 7.901 | 9.683 | 12.134 | 26.046 | 20.00 | 20.00 |
| 3 | Chunkwake | cows200 | 301 | 301 | 0.445 | 0.625 | 0.934 | 0.980 | 20.00 | 20.00 |
| 3 | Chunkwake | cows1000 | 301 | 301 | 0.494 | 0.665 | 0.799 | 1.052 | 20.01 | 20.00 |
| 3 | Chunkwake | redstone | 327 | 327 | 0.497 | 0.629 | 0.771 | 1.197 | 20.02 | 20.00 |
| 3 | Paper | cows200 | 301 | 301 | 2.066 | 4.423 | 25.896 | 47.854 | 20.01 | 19.89 |
| 3 | Paper | cows1000 | 301 | 301 | 6.637 | 8.579 | 10.953 | 20.898 | 19.99 | 20.00 |
| 3 | Paper | redstone | 322 | 322 | 6.416 | 15.759 | 28.932 | 57.732 | 20.01 | 20.00 |
| 4 | Chunkwake | cows200 | 301 | 301 | 0.390 | 0.543 | 0.650 | 1.186 | 20.00 | 20.00 |
| 4 | Chunkwake | cows1000 | 301 | 301 | 0.408 | 0.571 | 0.650 | 1.178 | 20.00 | 20.00 |
| 4 | Chunkwake | redstone | 323 | 322 | 0.374 | 0.541 | 0.728 | 0.991 | 20.00 | 20.00 |
| 4 | Paper | cows200 | 301 | 301 | 1.993 | 2.450 | 3.016 | 3.926 | 20.01 | 19.89 |
| 4 | Paper | cows1000 | 301 | 301 | 8.133 | 9.584 | 10.830 | 22.150 | 20.00 | 20.00 |
| 4 | Paper | redstone | 325 | 325 | 7.442 | 8.858 | 9.357 | 22.695 | 20.00 | 20.00 |
| 5 | Chunkwake | cows200 | 301 | 301 | 0.477 | 0.628 | 0.694 | 0.768 | 20.00 | 20.00 |
| 5 | Chunkwake | cows1000 | 301 | 301 | 0.523 | 0.654 | 0.748 | 0.791 | 20.02 | 20.00 |
| 5 | Chunkwake | redstone | 326 | 326 | 0.495 | 0.646 | 0.716 | 0.854 | 20.00 | 20.00 |
| 5 | Paper | cows200 | 301 | 301 | 2.066 | 2.614 | 3.014 | 4.376 | 20.01 | 19.89 |
| 5 | Paper | cows1000 | 301 | 301 | 6.832 | 7.872 | 8.829 | 20.093 | 20.00 | 20.00 |
| 5 | Paper | redstone | 324 | 324 | 6.694 | 7.684 | 9.737 | 22.377 | 20.00 | 20.00 |

## Pooled windows

| scenario | side | rounds | ticks | avg ms | p50 ms | p95 ms | p99 ms | p99 round median | max ms | TPS (pooled window) | TPS min | TPS API mean |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | Chunkwake | 5 | 1505 | 0.428 | 0.419 | 0.604 | 0.732 | 0.694 | 1.186 | 20.00 | 20.00 | 20.00 |
| cows200 | Paper | 5 | 1506 | 2.298 | 2.078 | 2.922 | 5.406 | 3.016 | 47.854 | 20.02 | 20.01 | 19.88 |
| cows1000 | Chunkwake | 5 | 1505 | 0.462 | 0.446 | 0.655 | 0.779 | 0.748 | 12.638 | 20.01 | 20.00 | 20.00 |
| cows1000 | Paper | 5 | 1504 | 7.832 | 7.684 | 9.803 | 11.597 | 10.830 | 26.424 | 19.99 | 19.95 | 20.00 |
| redstone | Chunkwake | 5 | 1624 | 0.440 | 0.433 | 0.624 | 0.738 | 0.728 | 1.197 | 20.01 | 20.00 | 20.00 |
| redstone | Paper | 5 | 1621 | 7.485 | 7.133 | 9.136 | 17.201 | 10.232 | 57.732 | 20.00 | 20.00 | 20.00 |

## Paired p99 and TPS deltas (Chunkwake - Paper)

| scenario | pairs | p99 delta ms (mean / median) | p99 CW wins | p99 p(CW better) | p99 p(Paper better) | pooled p99 CW / Paper | p99 ratio | TPS delta (mean) | TPS CW wins | TPS p(CW better) | TPS p(Paper better) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | 5 | -7.101 / -2.365 | 5/5 | 0.031 | 1.000 | 0.732 / 5.406 | 0.135x | -0.02 | 0/5 | 1.000 | 0.031 |
| cows1000 | 5 | -10.032 / -10.153 | 5/5 | 0.031 | 1.000 | 0.779 / 11.597 | 0.067x | 0.02 | 4/5 | 0.062 | 1.000 |
| redstone | 5 | -13.374 / -9.713 | 5/5 | 0.031 | 1.000 | 0.738 / 17.201 | 0.043x | 0.01 | 2/5 | 0.250 | 1.000 |

Negative p99 delta means Chunkwake had lower p99. Ties are excluded from the sign test.
The table gives both one-sided p-values: `p(CW better)` tests `Chunkwake < Paper`; `p(Paper better)` tests the opposite direction.

`TPS (window)` uses the authoritative `server_ticks` delta from `Server.getCurrentTick()`, not the number of Java tick events. `TPS API` from `Server.getTPS()[0]` is shown as a cross-check (Chunkwake: last-100-tick meter, Paper: 1-minute average).

Chunkwake tick duration = Rust core tick + Java scheduler/plugin tick for the same tick; Paper tick duration is the full server tick measured by Paper.
