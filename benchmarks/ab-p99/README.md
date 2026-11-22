# Paper vs Chunkwake p99/TPS benchmark (2026-11-22)

This directory contains the raw data behind [`docs/ab-p99.md`](../../docs/ab-p99.md).

- `results.md` - generated report with all per-round windows, pooled
  percentiles and paired sign tests.
- `samples/chunkwake/samples-rN-<scenario>.csv` and
  `samples/paper/samples-rN-<scenario>.csv` - raw `ServerTickEndEvent`
  durations in milliseconds, one value per line.
- Scenarios: `cows200`, `cows1000`, `redstone`.
- Rounds: 1..5. Every round uses fresh server directories and the same bot
  command script.

Reproduce:

```powershell
cd java
gradlew --offline --no-daemon :perf-probe:jar
cd ..
powershell -ExecutionPolicy Bypass -File scripts/ab_p99.ps1 -Rounds 5 -SampleMs 15000
py -3 scripts/analyze_ab_p99.py
```

The benchmark runs one `PerfProbe` plugin on each side; it is the only plugin.
Tick duration on Chunkwake includes the Rust tick plus the Java scheduler and
plugin tick for the same tick. Paper's duration is the full server tick.
Window TPS on Chunkwake is a conservative lower bound because the Java tick
queue can coalesce events; the API TPS column is reported separately.
