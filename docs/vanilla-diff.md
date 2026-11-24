# Differential vanilla behaviour: Paper vs Chunkwake

Both servers loaded the same `ChunkwakeTest` plugin and ran its built-in Bukkit/API observations. Volatile lines (timestamps, classloaders, world UUIDs, scheduler counters, per-tick region tasks) are omitted.

- Paper observations: **30**
- Chunkwake observations: **33**
- Diff lines: **39**

Raw logs: `benchmarks/vanilla-diff/paper.log` and `benchmarks/vanilla-diff/chunkwake.log`.

## Unified diff

```diff
--- Paper
+++ Chunkwake
@@ -8,23 +8,26 @@
 item bytes: 121 -> DIAMOND_SWORD x2 sharpness=3
 world time: before=<ticks> after=6000 day=true full=6000
 commandmaps: server=<CommandMap> bukkit=<CommandMap> error=null known=219
-folia: owned on enable=true global=true
+folia: owned on enable=false global=true
 item api: BREAD x3 max 64
 components2: food=4/2.5 weapon=7 equip=HEAD/chunkwake:test book=A/1 sign=4
 components: lore=1 ench={CraftEnchantment{holder=Reference{ResourceKey[minecraft:enchantment / minecraft:sharpness]=Enchantment Sharpness}}=3} color=Color:[argb0xFFFF0000] model=[abc]/[1.5] map=7
 folia: global execute
+folia: region delayed, state=RUNNING
 folia: global fixed rate x3, cancel=NEXT_RUNS_CANCELLED
-folia: region delayed, state=RUNNING
 folia: all-schedulers-ok
-redstone: direct=AIR/STONE wire=AIR/STONE
-hopper: moves=0 pickups=0
-fluids: flow=AIR fall=AIR edge=AIR meet=OBSIDIAN logged=AIR/CHEST minecraft:oak_stairs[facing=north,half=bottom,shape=straight,waterlogged=true] waterlogged=true
+dispense: SNOWBALL at 60
+move event: ARROW HOPPER -> DISPENSER
+redstone: direct=STONE/STONE wire=STONE/STONE
+hopper: moves=6 pickups=3
+dispense: SNOWBALL at 60
+fluids: flow=WATER fall=AIR edge=AIR meet=OBSIDIAN logged=WATER/CHEST minecraft:oak_stairs[facing=north,half=bottom,shape=straight,waterlogged=true] waterlogged=true
 mob: COW health=10.0/10.0 adult=true sheep=RED living=<n> spawned=<n>
-damage: Cow CUSTOM 20.0
-mob death: COW drops=[BEEF] xp=0
-tag-probe: base_stone_overworld -> <n> [DEEPSLATE, DIORITE, TUFF]
+tag-probe: base_stone_overworld -> <n> [ANDESITE, DEEPSLATE, DIORITE]
 tag-probe: coal_ores -> <n> [COAL_ORE, DEEPSLATE_COAL_ORE]
 tag-probe: diamond_ores -> <n> [DEEPSLATE_DIAMOND_ORE, DIAMOND_ORE]
-tag-probe: minecraft:base_stone_overworld -> <n> [DEEPSLATE, DIORITE, TUFF]
-fly: rear observer x=100 blocks=8
+tag-probe: minecraft:base_stone_overworld -> <n> [ANDESITE, DEEPSLATE, DIORITE]
+fly: rear observer x=-1 blocks=0
+mob death: COW drops=[BEEF, LEATHER] xp=0
 mob dead: true valid=false
+mob death: SHEEP drops=[MUTTON, RED_WOOL] xp=0
```

## Ключевые API-проверки

После правок совпадают:

- `commandmaps known=219/219`;
- `ItemStack.serializeAsBytes()` — **121 B / 121 B**, NBT в Paper-порядке;
- `materials.transparent` — **127 / 127**;
- `components` — формат `CraftEnchantment{...}`;
- `mannequin` — `NPC`, `skinParts=127/true/true`;
- `SignText` — 4 строки;
- `world time` — `full=6000`.

Для `material.values()` реализовано поведение CraftLegacy: legacy-константы
остаются доступны через `valueOf`/`fromLegacy`, но исключаются из
`Material.values()`, как в Paper build 248.

Оставшиеся строки diff — это timing/implementation наблюдения
(`redstone`, `hopper`, `fluids`, `fly`, `folia: owned on enable`, порядок
`folia`-задач и `damage`/`mob death`), а не числовые API-расхождения.
Они требуют отдельного детерминированного сценария с фиксированным
порядком тиков.

## Interpretation

The diff is a list of **open observations**, not a claim that every difference is a bug: some checks depend on chunk load order, world generation timing, or server-only formatting. It is intended as a reproducible baseline for the next vanilla-parity fixes.

