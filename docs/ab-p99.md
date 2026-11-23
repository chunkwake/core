# Paper vs Chunkwake: p99 MSPT и TPS (23.11.2026)

Строгий A/B на одинаковой внешней нагрузке: 5 независимых раундов, свежие
миры, release-сборка Chunkwake и одинаковый бот-сценарий. Каждый раунд
содержит три 15-секундных окна:

1. 200 коров;
2. 1000 коров;
3. 1000 коров + активная redstone-линия (переключение рычага каждые ~250 мс).

Метрики снимает `PerfProbe` (Paper API `ServerTickEndEvent`): p50/p95/p99
длительности тика и TPS. TPS считается по `Server.getCurrentTick()`, а не по
числу Java-событий, поэтому схлопывание очереди тиков не влияет на цифру.
Сырые samples: [`benchmarks/ab-p99/samples`](../benchmarks/ab-p99/samples).
Полный отчёт со всеми 30 окнами:
[`benchmarks/ab-p99/results.md`](../benchmarks/ab-p99/results.md).

## Стенд

- AMD Ryzen 9 7945HX, 16 ядер / 32 потока, 64 GiB RAM.
- Paper 26.3 build 248, JDK 25, `-Xms2G -Xmx2G`.
- Chunkwake: `cargo build --release -p chunkwake-server`, JDK 25,
  встроенный API bundle, `memory-profile=normal`.
- Один бот, `view-distance=8`, `simulation-distance=8`, плоский мир,
  `difficulty=peaceful`, `spawn_mobs=false`, `max_entity_cramming=0`.
- `PerfProbe` 1.2 — единственный плагин на обеих сторонах.

## Итоговая таблица

`p99` — pooled по всем сырым тикам; `p99 median` — медиана p99 по пяти
раундам. `TPS` — сумма `server_ticks` / сумма wall-clock окон. Отрицательная
дельта p99 означает, что Chunkwake быстрее.

| сценарий | сторона | p99 pooled, ms | p99 median, ms | avg ms | TPS pooled |
|---|---|---:|---:|---:|---:|
| 200 коров | Chunkwake | **0.966** | 0.960 | 0.684 | 20.00 |
| 200 коров | Paper | 3.713 | 3.763 | 2.362 | 19.99 |
| 1000 коров | Chunkwake | **1.638** | 1.630 | 1.182 | 19.99 |
| 1000 коров | Paper | 12.018 | 10.645 | 8.478 | 20.00 |
| 1000 коров + redstone | Chunkwake | **1.650** | 1.646 | 1.197 | 20.00 |
| 1000 коров + redstone | Paper | 10.534 | 10.049 | 7.639 | 19.99 |

### Парные сравнения

| сценарий | p99 CW лучше | p99 p(CW лучше) | p99 ratio | TPS delta | TPS CW лучше | TPS p(Paper лучше) |
|---|---:|---:|---:|---:|---:|---:|
| 200 коров | **5/5** | **0.031** | 0.260x | +0.01 | 2/5 | 0.875 |
| 1000 коров | **5/5** | **0.031** | 0.136x | −0.01 | 2/5 | 0.500 |
| 1000 коров + redstone | **5/5** | **0.031** | 0.157x | +0.01 | 3/5 | 0.812 |

p — односторонний точный sign-test. При N=5 результат 5/5 даёт минимально
возможный p=0.031; TPS-разница в пределах ±0.01 тика/с и статистически
неотличима от нуля.

## Вывод

- В этом сценарии Chunkwake стабильно быстрее по p99: 5/5 раундов во всех
  трёх нагрузках, p=0.031, p99 в 3.8–7.3 раза ниже Paper.
- TPS у обеих сторон 20.00: после исправления тик-планировщика Chunkwake
  больше не теряет тики и не уступает Paper по пропускной способности.
- Это **узкая** заявка: один игрок, ванильные коровы с AI, плоский мир,
  единственный плагин `PerfProbe`. Она не доказывает преимущество на
  произвольных плагинах, мирах и редстоун-схемах.

## Что было исправлено по пути

Первый прогон (debug-бинарник, `MissedTickBehavior::Skip`) дал TPS 18–19 и
слабый p99-выигрыш только на 1000 мобах. Причины:

1. **`Skip` навсегда терял тик**, если тик-луп опаздывал на несколько
   миллисекунд. Заменено на `Burst`: пропущенные дедлайны догоняются, как в
   ванильном tick loop. На release-сборке TPS сразу стал 20.0.
2. **Debug-бинарник** имел MSPT в разы выше release. A/B теперь запускается
   на `target/release/chunkwake.exe`; `scripts/ab_p99.ps1` проверяет, что
   бинарник новее API-jar.
3. `PerfProbe` 1.2 считает `server_ticks` через `Server.getCurrentTick()`;
   для Chunkwake добавлен downcall `world("tick")` в `TickStats::total()`.

## Воспроизведение

```powershell
cd java
gradlew --offline --no-daemon :perf-probe:jar dist

cd ..
cargo build --release -p chunkwake-server
powershell -ExecutionPolicy Bypass -File scripts/ab_p99.ps1 -Rounds 5 -SampleMs 15000
py -3 scripts/analyze_ab_p99.py
```

## Ограничения

- Длительность тика Chunkwake — Rust-тик плюс Java-часть
  (`scheduler.tick()` + `ChunkEvents` + `ServerTickEndEvent`) для того же
  тика. У Paper — полный тик, включая всю Java-работу.
- N=5 достаточно для направления и даёт p=0.031 при 5/5, но не заменяет
  длительный многопроцессный benchmark на разных машинах.
- 100% совместимость с произвольными Paper/Folia-плагинами этим прогоном
  не проверяется; для неё нужен отдельный расширенный plugin gate.
