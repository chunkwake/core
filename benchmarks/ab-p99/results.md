# Paper vs Chunkwake: p99 MSPT / TPS

Raw data: `target\ab-p99`; rounds: [1, 2, 3, 4, 5].

`p99` and all percentiles are computed from raw `ServerTickEndEvent` samples; `TPS` uses `server_ticks` (Server.getCurrentTick delta) divided by wall-clock length of the same window.

## Per-round windows

| round | side | scenario | events | server ticks | p50 ms | p95 ms | p99 ms | max ms | TPS (window) | TPS API |
|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | Chunkwake | cows200 | 301 | 300 | 0.622 | 0.745 | 0.809 | 1.623 | 19.95 | 20.00 |
| 1 | Chunkwake | cows1000 | 300 | 300 | 1.056 | 1.252 | 1.310 | 1.421 | 19.94 | 20.00 |
| 1 | Chunkwake | redstone | 323 | 323 | 1.052 | 1.276 | 1.350 | 1.464 | 19.95 | 20.00 |
| 1 | Paper | cows200 | 301 | 301 | 1.968 | 2.304 | 2.849 | 3.173 | 19.99 | 19.93 |
| 1 | Paper | cows1000 | 301 | 301 | 7.117 | 7.893 | 10.356 | 15.086 | 20.01 | 20.00 |
| 1 | Paper | redstone | 323 | 323 | 7.160 | 9.014 | 9.985 | 17.439 | 20.00 | 20.00 |
| 2 | Chunkwake | cows200 | 301 | 301 | 0.683 | 0.828 | 0.960 | 1.620 | 20.01 | 20.00 |
| 2 | Chunkwake | cows1000 | 301 | 301 | 1.187 | 1.504 | 1.770 | 2.081 | 19.99 | 20.00 |
| 2 | Chunkwake | redstone | 324 | 324 | 1.226 | 1.539 | 1.741 | 2.151 | 19.97 | 20.00 |
| 2 | Paper | cows200 | 301 | 301 | 2.327 | 2.839 | 3.244 | 4.123 | 19.99 | 19.85 |
| 2 | Paper | cows1000 | 300 | 300 | 8.257 | 9.753 | 10.517 | 12.493 | 19.95 | 20.00 |
| 2 | Paper | redstone | 324 | 324 | 6.885 | 8.692 | 10.049 | 17.627 | 19.99 | 20.00 |
| 3 | Chunkwake | cows200 | 301 | 301 | 0.686 | 0.899 | 1.000 | 1.034 | 20.02 | 20.00 |
| 3 | Chunkwake | cows1000 | 301 | 301 | 1.231 | 1.492 | 1.662 | 1.994 | 20.02 | 20.00 |
| 3 | Chunkwake | redstone | 325 | 325 | 1.246 | 1.578 | 1.736 | 2.019 | 20.02 | 20.00 |
| 3 | Paper | cows200 | 300 | 300 | 2.658 | 3.350 | 4.065 | 7.261 | 19.96 | 19.80 |
| 3 | Paper | cows1000 | 301 | 301 | 9.754 | 11.854 | 13.914 | 19.391 | 20.00 | 20.00 |
| 3 | Paper | redstone | 324 | 324 | 6.935 | 8.260 | 9.322 | 20.024 | 20.00 | 20.00 |
| 4 | Chunkwake | cows200 | 301 | 301 | 0.729 | 0.912 | 0.966 | 1.072 | 20.01 | 20.00 |
| 4 | Chunkwake | cows1000 | 301 | 301 | 1.230 | 1.484 | 1.630 | 2.267 | 20.00 | 20.00 |
| 4 | Chunkwake | redstone | 324 | 325 | 1.198 | 1.493 | 1.646 | 3.302 | 20.02 | 20.00 |
| 4 | Paper | cows200 | 301 | 301 | 2.306 | 2.954 | 3.763 | 4.179 | 20.01 | 19.90 |
| 4 | Paper | cows1000 | 301 | 301 | 8.738 | 9.900 | 10.645 | 11.076 | 20.02 | 20.00 |
| 4 | Paper | redstone | 322 | 322 | 8.200 | 9.762 | 11.370 | 20.146 | 19.95 | 20.00 |
| 5 | Chunkwake | cows200 | 301 | 301 | 0.694 | 0.855 | 0.929 | 1.719 | 20.01 | 20.00 |
| 5 | Chunkwake | cows1000 | 301 | 301 | 1.197 | 1.446 | 1.561 | 2.626 | 19.99 | 20.00 |
| 5 | Chunkwake | redstone | 325 | 325 | 1.202 | 1.438 | 1.552 | 1.631 | 20.04 | 20.00 |
| 5 | Paper | cows200 | 301 | 301 | 2.331 | 3.114 | 3.932 | 4.125 | 20.01 | 19.78 |
| 5 | Paper | cows1000 | 301 | 301 | 8.398 | 10.296 | 11.063 | 11.602 | 20.02 | 20.00 |
| 5 | Paper | redstone | 324 | 324 | 8.095 | 10.078 | 11.433 | 21.017 | 20.00 | 20.00 |

## Pooled windows

| scenario | side | rounds | ticks | avg ms | p50 ms | p95 ms | p99 ms | p99 round median | max ms | TPS (pooled window) | TPS min | TPS API mean |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | Chunkwake | 5 | 1505 | 0.684 | 0.680 | 0.867 | 0.966 | 0.960 | 1.719 | 20.00 | 19.95 | 20.00 |
| cows200 | Paper | 5 | 1504 | 2.362 | 2.291 | 3.126 | 3.713 | 3.763 | 7.261 | 19.99 | 19.96 | 19.85 |
| cows1000 | Chunkwake | 5 | 1504 | 1.182 | 1.176 | 1.462 | 1.638 | 1.630 | 2.626 | 19.99 | 19.94 | 20.00 |
| cows1000 | Paper | 5 | 1504 | 8.478 | 8.293 | 10.815 | 12.018 | 10.645 | 19.391 | 20.00 | 19.95 | 20.00 |
| redstone | Chunkwake | 5 | 1621 | 1.197 | 1.181 | 1.498 | 1.650 | 1.646 | 3.302 | 20.00 | 19.95 | 20.00 |
| redstone | Paper | 5 | 1617 | 7.639 | 7.466 | 9.324 | 10.534 | 10.049 | 21.017 | 19.99 | 19.95 | 20.00 |

## Paired p99 and TPS deltas (Chunkwake - Paper)

| scenario | pairs | p99 delta ms (mean / median) | p99 CW wins | p99 p(CW better) | p99 p(Paper better) | pooled p99 CW / Paper | p99 ratio | TPS delta (mean) | TPS CW wins | TPS p(CW better) | TPS p(Paper better) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | 5 | -2.638 / -2.797 | 5/5 | 0.031 | 1.000 | 0.966 / 3.713 | 0.260x | 0.01 | 2/5 | 0.500 | 0.875 |
| cows1000 | 5 | -9.712 / -9.046 | 5/5 | 0.031 | 1.000 | 1.638 / 12.018 | 0.136x | -0.01 | 2/5 | 0.812 | 0.500 |
| redstone | 5 | -8.827 / -8.635 | 5/5 | 0.031 | 1.000 | 1.650 / 10.534 | 0.157x | 0.01 | 3/5 | 0.500 | 0.812 |

Negative p99 delta means Chunkwake had lower p99. Ties are excluded from the sign test.
The table gives both one-sided p-values: `p(CW better)` tests `Chunkwake < Paper`; `p(Paper better)` tests the opposite direction.

`TPS (window)` uses the authoritative `server_ticks` delta from `Server.getCurrentTick()`, not the number of Java tick events. `TPS API` from `Server.getTPS()[0]` is shown as a cross-check (Chunkwake: last-100-tick meter, Paper: 1-minute average).

Chunkwake tick duration = Rust core tick + Java scheduler/plugin tick for the same tick; Paper tick duration is the full server tick measured by Paper.
