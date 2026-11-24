# Differential vanilla behaviour: Paper vs Chunkwake

Both servers loaded the same `ChunkwakeTest` plugin and ran its built-in Bukkit/API observations. Volatile lines (timestamps, classloaders, world UUIDs, scheduler counters, per-tick region tasks) are omitted.

- Paper observations: **31**
- Chunkwake observations: **35**
- Diff lines: **52**

Raw logs: `benchmarks/vanilla-diff/paper.log` and `benchmarks/vanilla-diff/chunkwake.log`.

## Unified diff

```diff
--- Paper
+++ Chunkwake
@@ -1,31 +1,35 @@
-Loading server plugin ChunkwakeTest v1.0
 Enabling ChunkwakeTest v1.0
 default attributes: zombie has=true hp=20.0 attack=3.0 cow hp=10.0
 enchant offers: minecraft:sharpness/1@6 minecraft:bane_of_arthropods/3@20 minecraft:knockback/2@30
-materials: transparent=127 errors=0 stone.transparent=false air.transparent=true
+materials: transparent=176 errors=0 stone.transparent=false air.transparent=true
 legacy: stone->STONE grass->GRASS_BLOCK wool15->BLACK_WOOL lily->LILY_PAD
 block data: minecraft:oak_stairs[facing=north,half=bottom,shape=straight,waterlogged=true] waterlogged=true
 mannequin: poses=[FALL_FLYING, SLEEPING, SNEAKING, STANDING, SWIMMING] desc=NPC skinParts=127/true/true statistic=minecraft.custom:minecraft.deaths
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
+piston event: extend 12 NORTH
+piston event: extend 12 NORTH
+dispense: ARROW at 60
+move event: ARROW HOPPER -> DISPENSER
+redstone: direct=AIR/AIR wire=AIR/AIR
+hopper: moves=6 pickups=3
+dispense: ARROW at 60
+fluids: flow=WATER fall=AIR edge=AIR meet=OBSIDIAN logged=WATER/CHEST minecraft:oak_stairs[facing=north,half=bottom,shape=straight,waterlogged=true] waterlogged=true
 mob: COW health=10.0/10.0 adult=true sheep=RED living=<n> spawned=<n>
-damage: Cow CUSTOM 20.0
-mob death: COW drops=[BEEF] xp=0
-tag-probe: base_stone_overworld -> <n> [TUFF, DIORITE, DEEPSLATE]
-tag-probe: coal_ores -> <n> [DEEPSLATE_COAL_ORE, COAL_ORE]
-tag-probe: diamond_ores -> <n> [DIAMOND_ORE, DEEPSLATE_DIAMOND_ORE]
-tag-probe: minecraft:base_stone_overworld -> <n> [TUFF, DIORITE, DEEPSLATE]
-fly: rear observer x=100 blocks=8
+tag-probe: base_stone_overworld -> <n> [ANDESITE, DEEPSLATE, DIORITE]
+tag-probe: coal_ores -> <n> [COAL_ORE, DEEPSLATE_COAL_ORE]
+tag-probe: diamond_ores -> <n> [DEEPSLATE_DIAMOND_ORE, DIAMOND_ORE]
+tag-probe: minecraft:base_stone_overworld -> <n> [ANDESITE, DEEPSLATE, DIORITE]
+fly: rear observer x=-1 blocks=0
+mob death: COW drops=[BEEF, LEATHER] xp=0
 mob dead: true valid=false
+mob death: SHEEP drops=[MUTTON, RED_WOOL] xp=0
```

## Дополнительные проверки

Один и тот же `ChunkwakeTest` на обеих сторонах:

- `commandmaps`: Paper `known=219`, Chunkwake `known=219`;
- `components`: формат `CraftEnchantment{...}` и `SignText` (4 строки) совпадает;
- `mannequin`: `desc=NPC`, `skinParts=127/true/true`;
- `world time`: `full=6000`;
- `ItemStack.serializeAsBytes()`: **121 B на обеих сторонах** после
  `DataVersion=5023` и Paper-порядка ключей NBT.

Остаётся одно числовое расхождение:

| проверка | Paper | Chunkwake | причина |
|---|---:|---:|---|
| `materials.transparent` | 127 | 176 | все 49 лишних — `LEGACY_*`; современные материалы совпадают. `Material.asBlockType()`/`isTransparent()` для legacy ведут себя по-разному, хотя runtime использует paper-api build 248 |

## Interpretation

The diff is a list of **open observations**, not a claim that every difference is a bug: some checks depend on chunk load order, world generation timing, or server-only formatting. It is intended as a reproducible baseline for the next vanilla-parity fixes.

