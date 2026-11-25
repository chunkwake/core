# Paper vs Chunkwake: p99 MSPT / TPS

Raw data: `target\ab-p99-plugin`; rounds: [1, 2, 3, 4, 5].

`p99` and all percentiles are computed from raw `ServerTickEndEvent` samples; `TPS` uses `server_ticks` (Server.getCurrentTick delta) divided by wall-clock length of the same window.

## Per-round windows

| round | side | scenario | events | server ticks | p50 ms | p95 ms | p99 ms | max ms | TPS (window) | TPS API |
|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | Chunkwake | cows200 | 300 | 300 | 0.550 | 0.770 | 0.940 | 2.167 | 20.00 | 20.00 |
| 1 | Chunkwake | cows1000 | 300 | 300 | 1.348 | 1.799 | 2.322 | 2.846 | 20.00 | 20.00 |
| 1 | Chunkwake | redstone | 300 | 300 | 1.297 | 1.790 | 2.054 | 2.406 | 20.00 | 20.00 |
| 1 | Paper | cows200 | 300 | 300 | 4.061 | 7.308 | 38.906 | 175.821 | 20.00 | 19.78 |
| 1 | Paper | cows1000 | 300 | 300 | 11.132 | 13.382 | 15.916 | 53.604 | 20.02 | 20.00 |
| 1 | Paper | redstone | 300 | 300 | 9.383 | 12.000 | 14.620 | 41.329 | 19.98 | 20.00 |
| 2 | Chunkwake | cows200 | 300 | 300 | 0.671 | 0.998 | 1.174 | 2.466 | 20.00 | 20.00 |
| 2 | Chunkwake | cows1000 | 300 | 300 | 1.430 | 2.050 | 2.891 | 3.804 | 19.99 | 20.00 |
| 2 | Chunkwake | redstone | 300 | 300 | 1.478 | 2.061 | 2.334 | 2.781 | 19.93 | 20.00 |
| 2 | Paper | cows200 | 300 | 300 | 3.692 | 5.989 | 45.706 | 224.246 | 20.00 | 19.66 |
| 2 | Paper | cows1000 | 300 | 300 | 11.360 | 13.707 | 16.037 | 32.739 | 20.00 | 20.00 |
| 2 | Paper | redstone | 300 | 300 | 11.513 | 14.778 | 24.566 | 47.559 | 20.00 | 20.00 |
| 3 | Chunkwake | cows200 | 300 | 300 | 0.592 | 0.908 | 1.000 | 2.478 | 20.01 | 20.00 |
| 3 | Chunkwake | cows1000 | 300 | 300 | 1.255 | 1.721 | 2.274 | 2.789 | 19.96 | 20.00 |
| 3 | Chunkwake | redstone | 300 | 300 | 1.256 | 1.672 | 2.052 | 2.247 | 20.01 | 20.00 |
| 3 | Paper | cows200 | 300 | 300 | 4.124 | 6.231 | 30.232 | 74.058 | 20.00 | 19.67 |
| 3 | Paper | cows1000 | 300 | 300 | 10.045 | 12.968 | 14.545 | 32.616 | 20.00 | 20.00 |
| 3 | Paper | redstone | 300 | 300 | 8.676 | 11.838 | 13.168 | 42.095 | 20.00 | 20.00 |
| 4 | Chunkwake | cows200 | 300 | 300 | 0.575 | 0.836 | 0.938 | 2.678 | 20.00 | 20.00 |
| 4 | Chunkwake | cows1000 | 300 | 300 | 1.232 | 1.651 | 2.749 | 3.818 | 20.00 | 20.00 |
| 4 | Chunkwake | redstone | 300 | 300 | 1.228 | 1.585 | 2.378 | 4.195 | 20.00 | 20.00 |
| 4 | Paper | cows200 | 300 | 300 | 3.339 | 5.222 | 28.105 | 81.095 | 20.00 | 19.70 |
| 4 | Paper | cows1000 | 300 | 300 | 9.366 | 11.619 | 13.988 | 33.306 | 20.00 | 20.00 |
| 4 | Paper | redstone | 300 | 300 | 8.918 | 11.355 | 14.047 | 42.401 | 20.00 | 20.00 |
| 5 | Chunkwake | cows200 | 300 | 300 | 0.598 | 0.851 | 0.979 | 2.300 | 19.99 | 20.00 |
| 5 | Chunkwake | cows1000 | 300 | 300 | 1.293 | 1.652 | 1.996 | 2.800 | 20.00 | 20.00 |
| 5 | Chunkwake | redstone | 300 | 300 | 1.334 | 1.793 | 2.315 | 3.110 | 20.00 | 20.00 |
| 5 | Paper | cows200 | 300 | 300 | 4.020 | 7.390 | 46.826 | 143.214 | 20.00 | 19.63 |
| 5 | Paper | cows1000 | 300 | 300 | 11.016 | 13.236 | 15.491 | 33.474 | 20.00 | 20.00 |
| 5 | Paper | redstone | 300 | 300 | 10.669 | 13.527 | 18.521 | 44.942 | 20.00 | 20.00 |

## Pooled windows

| scenario | side | rounds | ticks | avg ms | p50 ms | p95 ms | p99 ms | p99 round median | max ms | TPS (pooled window) | TPS min | TPS API mean |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | Chunkwake | 5 | 1500 | 0.638 | 0.597 | 0.887 | 1.085 | 0.979 | 2.678 | 20.00 | 19.99 | 20.00 |
| cows200 | Paper | 5 | 1500 | 5.276 | 3.845 | 6.322 | 44.306 | 38.906 | 224.246 | 20.00 | 20.00 | 19.69 |
| cows1000 | Chunkwake | 5 | 1500 | 1.369 | 1.312 | 1.889 | 2.560 | 2.322 | 3.818 | 19.99 | 19.96 | 20.00 |
| cows1000 | Paper | 5 | 1500 | 10.808 | 10.731 | 13.229 | 15.642 | 15.491 | 53.604 | 20.00 | 20.00 | 20.00 |
| redstone | Chunkwake | 5 | 1500 | 1.371 | 1.308 | 1.849 | 2.241 | 2.315 | 4.195 | 19.99 | 19.93 | 20.00 |
| redstone | Paper | 5 | 1500 | 10.343 | 9.926 | 13.184 | 17.134 | 14.620 | 47.559 | 20.00 | 19.98 | 20.00 |

## Paired p99 and TPS deltas (Chunkwake - Paper)

| scenario | pairs | p99 delta ms (mean / median) | p99 CW wins | p99 p(CW better) | p99 p(Paper better) | pooled p99 CW / Paper | p99 ratio | TPS delta (mean) | TPS CW wins | TPS p(CW better) | TPS p(Paper better) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | 5 | -36.949 / -37.966 | 5/5 | 0.031 | 1.000 | 1.085 / 44.306 | 0.024x | 0.00 | 1/5 | 0.750 | 0.750 |
| cows1000 | 5 | -12.749 / -13.146 | 5/5 | 0.031 | 1.000 | 2.560 / 15.642 | 0.164x | -0.01 | 0/5 | 1.000 | 0.125 |
| redstone | 5 | -14.758 / -12.566 | 5/5 | 0.031 | 1.000 | 2.241 / 17.134 | 0.131x | -0.01 | 2/5 | 0.500 | 0.875 |

Negative p99 delta means Chunkwake had lower p99. Ties are excluded from the sign test.
The table gives both one-sided p-values: `p(CW better)` tests `Chunkwake < Paper`; `p(Paper better)` tests the opposite direction.

`TPS (window)` uses the authoritative `server_ticks` delta from `Server.getCurrentTick()`, not the number of Java tick events. `TPS API` from `Server.getTPS()[0]` is shown as a cross-check (Chunkwake: last-100-tick meter, Paper: 1-minute average).

Chunkwake tick duration = Rust core tick + Java scheduler/plugin tick for the same tick; Paper tick duration is the full server tick measured by Paper.
