# Paper vs Chunkwake: p99 MSPT / TPS

Raw data: `target\ab-p99`; rounds: [1, 2, 3, 4, 5].

`p99` and all percentiles are computed from raw `ServerTickEndEvent` samples; `TPS` is ticks divided by the wall-clock length of the same window.

## Per-round windows

| round | side | scenario | ticks | p50 ms | p95 ms | p99 ms | max ms | TPS (window) | TPS API |
|---:|---|---|---:|---:|---:|---:|---:|---:|---:|
| 1 | Chunkwake | cows200 | 300 | 2.857 | 3.488 | 3.813 | 4.208 | 19.95 | 19.30 |
| 1 | Chunkwake | cows1000 | 296 | 5.551 | 6.320 | 7.161 | 7.249 | 19.68 | 19.30 |
| 1 | Chunkwake | redstone | 317 | 5.620 | 6.521 | 7.241 | 9.156 | 19.74 | 19.40 |
| 1 | Paper | cows200 | 301 | 2.190 | 2.690 | 3.169 | 3.717 | 19.99 | 19.76 |
| 1 | Paper | cows1000 | 301 | 8.832 | 10.281 | 11.509 | 21.029 | 20.00 | 20.00 |
| 1 | Paper | redstone | 324 | 8.264 | 10.097 | 10.992 | 20.554 | 20.00 | 20.00 |
| 2 | Chunkwake | cows200 | 296 | 3.791 | 4.456 | 5.256 | 7.635 | 19.68 | 19.60 |
| 2 | Chunkwake | cows1000 | 289 | 6.624 | 7.526 | 8.339 | 8.920 | 19.21 | 19.00 |
| 2 | Chunkwake | redstone | 313 | 6.854 | 8.074 | 8.441 | 10.681 | 19.32 | 19.20 |
| 2 | Paper | cows200 | 301 | 2.433 | 2.899 | 3.280 | 3.504 | 19.99 | 19.82 |
| 2 | Paper | cows1000 | 301 | 9.272 | 11.102 | 12.517 | 19.407 | 20.01 | 20.00 |
| 2 | Paper | redstone | 324 | 8.675 | 10.388 | 11.822 | 19.485 | 20.00 | 20.00 |
| 3 | Chunkwake | cows200 | 282 | 3.671 | 4.667 | 6.233 | 9.552 | 18.74 | 18.70 |
| 3 | Chunkwake | cows1000 | 299 | 5.810 | 8.376 | 13.549 | 35.480 | 19.88 | 19.20 |
| 3 | Chunkwake | redstone | 308 | 7.862 | 11.195 | 25.294 | 56.742 | 18.95 | 19.10 |
| 3 | Paper | cows200 | 302 | 2.753 | 4.072 | 4.739 | 5.748 | 20.03 | 19.83 |
| 3 | Paper | cows1000 | 301 | 7.563 | 9.547 | 11.313 | 13.925 | 20.00 | 20.00 |
| 3 | Paper | redstone | 324 | 6.722 | 8.059 | 10.158 | 19.398 | 20.04 | 20.00 |
| 4 | Chunkwake | cows200 | 297 | 3.908 | 4.839 | 5.391 | 7.707 | 19.74 | 19.70 |
| 4 | Chunkwake | cows1000 | 300 | 6.557 | 7.826 | 8.314 | 9.659 | 19.94 | 20.00 |
| 4 | Chunkwake | redstone | 324 | 5.877 | 7.356 | 7.892 | 8.534 | 19.97 | 20.00 |
| 4 | Paper | cows200 | 300 | 2.461 | 3.000 | 3.368 | 3.889 | 19.96 | 19.92 |
| 4 | Paper | cows1000 | 301 | 9.301 | 11.198 | 11.673 | 12.411 | 20.01 | 20.00 |
| 4 | Paper | redstone | 323 | 8.310 | 9.600 | 11.724 | 17.892 | 20.00 | 20.00 |
| 5 | Chunkwake | cows200 | 282 | 3.722 | 4.631 | 13.027 | 37.266 | 18.76 | 18.20 |
| 5 | Chunkwake | cows1000 | 287 | 5.276 | 6.081 | 6.641 | 7.128 | 19.06 | 18.80 |
| 5 | Chunkwake | redstone | 307 | 6.229 | 7.308 | 7.791 | 8.503 | 19.03 | 18.80 |
| 5 | Paper | cows200 | 301 | 2.390 | 2.986 | 3.575 | 4.506 | 19.99 | 19.90 |
| 5 | Paper | cows1000 | 301 | 8.769 | 9.737 | 10.317 | 11.208 | 20.00 | 20.00 |
| 5 | Paper | redstone | 324 | 8.380 | 9.289 | 10.316 | 19.463 | 20.00 | 20.00 |

## Pooled windows

| scenario | side | rounds | ticks | avg ms | p50 ms | p95 ms | p99 ms | p99 round median | max ms | TPS (pooled window) | TPS min | TPS API mean |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | Chunkwake | 5 | 1457 | 3.692 | 3.627 | 4.605 | 5.655 | 5.391 | 37.266 | 19.37 | 18.74 | 19.10 |
| cows200 | Paper | 5 | 1505 | 2.495 | 2.428 | 3.283 | 4.077 | 3.368 | 5.748 | 19.99 | 19.96 | 19.85 |
| cows1000 | Chunkwake | 5 | 1471 | 6.115 | 5.942 | 7.430 | 8.621 | 8.314 | 35.480 | 19.55 | 19.06 | 19.26 |
| cows1000 | Paper | 5 | 1505 | 8.842 | 8.802 | 10.709 | 11.673 | 11.509 | 21.029 | 20.00 | 20.00 | 20.00 |
| redstone | Chunkwake | 5 | 1569 | 6.702 | 6.322 | 8.760 | 11.195 | 7.892 | 56.742 | 19.40 | 18.95 | 19.30 |
| redstone | Paper | 5 | 1619 | 8.156 | 8.168 | 9.928 | 11.164 | 10.992 | 20.554 | 20.01 | 20.00 | 20.00 |

## Paired p99 and TPS deltas (Chunkwake - Paper)

| scenario | pairs | p99 delta ms (mean / median) | p99 CW wins | p99 p(CW better) | p99 p(Paper better) | pooled p99 CW / Paper | p99 ratio | TPS delta (mean) | TPS CW wins | TPS p(CW better) | TPS p(Paper better) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | 5 | 3.118 / 1.976 | 0/5 | 1.000 | 0.031 | 5.655 / 4.077 | 1.387x | -0.62 | 0/5 | 1.000 | 0.031 |
| cows1000 | 5 | -2.665 / -3.676 | 4/5 | 0.188 | 0.969 | 8.621 / 11.673 | 0.739x | -0.45 | 0/5 | 1.000 | 0.031 |
| redstone | 5 | 0.330 / -3.381 | 4/5 | 0.188 | 0.969 | 11.195 / 11.164 | 1.003x | -0.61 | 0/5 | 1.000 | 0.031 |

Negative p99 delta means Chunkwake had lower p99. Ties are excluded from the sign test.
The table gives both one-sided p-values: `p(CW better)` tests `Chunkwake < Paper`; `p(Paper better)` tests the opposite direction.

`TPS (window)` is computed from the ticks observed by PerfProbe in that window. On Paper this is one event per server tick. On Chunkwake the Java tick queue can coalesce events, so its window TPS is a conservative lower bound; `TPS API` comes from `Server.getTPS()[0]` (Chunkwake: last-100-tick meter, Paper: 1-minute average).

Chunkwake tick duration = Rust core tick + Java scheduler/plugin tick for the same tick; Paper tick duration is the full server tick measured by Paper.
