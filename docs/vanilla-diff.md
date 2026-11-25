# Differential vanilla behaviour: Paper vs Chunkwake

Both servers loaded the same `ChunkwakeTest` plugin and ran its built-in Bukkit/API observations. Volatile lines (timestamps, classloaders, world UUIDs, scheduler counters, per-tick region tasks) are omitted. Independent scheduler callbacks are compared as a sorted set.

- Paper observations: **18**
- Chunkwake observations: **18**
- Diff lines: **0**

Raw logs: `benchmarks/vanilla-diff/paper.txt` and `benchmarks/vanilla-diff/chunkwake.txt`.

## Unified diff

```diff
```

## Interpretation

All extracted observations matched on this run. This is still a
**finite** onEnable API probe: it covers Bukkit/Paper/Folia values
and scheduler callbacks, not arbitrary plugin behaviour.

