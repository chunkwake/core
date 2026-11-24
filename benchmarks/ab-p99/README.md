# Paper vs Chunkwake p99/TPS benchmark (2026-11-23)

Raw data behind [`docs/ab-p99.md`](../../docs/ab-p99.md).

- `results.md` - generated report with all 30 per-round windows, pooled
  percentiles, exact `server_ticks` TPS and paired sign tests.
- `samples/chunkwake/samples-rN-<scenario>.csv` and
  `samples/paper/samples-rN-<scenario>.csv` - raw `ServerTickEndEvent`
  durations in milliseconds, one value per line.
- Scenarios: `cows200`, `cows1000`, `redstone`. Rounds: 1..5.
- Every round uses fresh server directories and the same bot command script.

Build and run:

```powershell
cd java
gradlew --offline --no-daemon :perf-probe:jar dist

cd ..
cargo build --release -p chunkwake-server
powershell -ExecutionPolicy Bypass -File scripts/ab_p99.ps1 -Rounds 5 -SampleMs 15000
# Low-load mode for a shared workstation:
powershell -ExecutionPolicy Bypass -File scripts/ab_p99.ps1 -Rounds 5 -SampleMs 15000 `
  -AffinityMask 255 -ProcessPriority BelowNormal
py -3 scripts/analyze_ab_p99.py
```

The checked-in `results.md` and `samples/` are from the final release build
(`172406d`) and a paired low-load run: both servers used affinity `0xFF` and
`BelowNormal`, 5 rounds, 15 s windows. Paper `cache/` and `versions/` are
pre-copied into every fresh stand so Paperclip network failures cannot abort a
round.

The benchmark runs one `PerfProbe` 1.2 plugin on each side; it is the only
plugin. Tick duration on Chunkwake includes the Rust tick plus the Java
scheduler and plugin tick for the same tick. Paper's duration is the full
server tick. TPS is the `Server.getCurrentTick()` delta over the window, not
the number of Java tick events. Chunkwake uses the release binary and the
`MissedTickBehavior::Burst` tick loop.

The earlier debug/`Skip` dataset is preserved in git history (commit
`ae9275e`) and at `target/ab-p99-debug-skip` in the working tree; it is
superseded by this release/Burst run.
