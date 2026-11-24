# Differential vanilla behaviour: Paper vs Chunkwake

Both servers loaded the same `ChunkwakeTest` plugin and ran its built-in Bukkit/API observations. Volatile lines (timestamps, classloaders, world UUIDs, scheduler counters, per-tick region tasks) are omitted.

- Paper observations: **31**
- Chunkwake observations: **34**
- Diff lines: **54**

Raw logs: `benchmarks/vanilla-diff/paper.log` and `benchmarks/vanilla-diff/chunkwake.log`.

## Unified diff

```diff
--- Paper
+++ Chunkwake
@@ -1,31 +1,34 @@
-Loading server plugin ChunkwakeTest v1.0
 Enabling ChunkwakeTest v1.0
 default attributes: zombie has=true hp=20.0 attack=3.0 cow hp=10.0
 enchant offers: minecraft:sharpness/1@6 minecraft:bane_of_arthropods/3@20 minecraft:knockback/2@30
-materials: transparent=127 errors=0 stone.transparent=false air.transparent=true
+materials: transparent=176 errors=0 stone.transparent=false air.transparent=true
 legacy: stone->STONE grass->GRASS_BLOCK wool15->BLACK_WOOL lily->LILY_PAD
 block data: minecraft:oak_stairs[facing=north,half=bottom,shape=straight,waterlogged=true] waterlogged=true
 mannequin: poses=[FALL_FLYING, SLEEPING, SNEAKING, STANDING, SWIMMING] desc=NPC skinParts=127/true/true statistic=minecraft.custom:minecraft.deaths
-item bytes: 121 -> DIAMOND_SWORD x2 sharpness=3
+item bytes: 122 -> DIAMOND_SWORD x2 sharpness=3
 world time: before=<ticks> after=6000 day=true full=6000
-commandmaps: server=org.bukkit.craftbukkit.command.CraftCommandMap bukkit=org.bukkit.craftbukkit.command.CraftCommandMap error=null known=219
-folia: owned on enable=true global=true
+commandmaps: server=org.bukkit.command.SimpleCommandMap bukkit=org.bukkit.command.SimpleCommandMap error=null known=212
+folia: owned on enable=false global=true
 item api: BREAD x3 max 64
 components2: food=4/2.5 weapon=7 equip=HEAD/chunkwake:test book=A/1 sign=4
-components: lore=1 ench={CraftEnchantment{holder=Reference{ResourceKey[minecraft:enchantment / minecraft:sharpness]=Enchantment Sharpness}}=3} color=Color:[argb0xFFFF0000] model=[abc]/[1.5] map=7
+components: lore=1 ench={minecraft:sharpness=3} color=Color:[argb0xFFFF0000] model=[abc]/[1.5] map=7
 folia: global execute
+folia: region delayed, state=RUNNING
 folia: global fixed rate x3, cancel=NEXT_RUNS_CANCELLED
-folia: region delayed, state=RUNNING
 folia: all-schedulers-ok
-redstone: direct=AIR/STONE wire=AIR/STONE
-hopper: moves=0 pickups=0
-fluids: flow=AIR fall=AIR edge=AIR meet=OBSIDIAN logged=AIR/CHEST minecraft:oak_stairs[facing=north,half=bottom,shape=straight,waterlogged=true] waterlogged=true
+piston event: extend 6 NORTH
+piston event: extend 6 NORTH
+dispense: SNOWBALL at 60
+move event: ARROW HOPPER -> DISPENSER
+redstone: direct=STONE/PISTON_HEAD wire=STONE/PISTON_HEAD
+hopper: moves=6 pickups=3
+fluids: flow=AIR fall=AIR edge=AIR meet=AIR logged=AIR/AIR minecraft:air waterlogged=false
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

## Дополнительный probe (без ChunkwakeTest)

Один и тот же `CmdMapProbe` на Paper и Chunkwake дал:

| проверка | Paper | Chunkwake | статус |
|---|---:|---:|---|
| `getKnownCommands().size()` | 209 | 207 | −2; ванильные имена из `commands.json` зарегистрированы, часть Paper/Bukkit-алиасов ещё нет |
| `Material.isTransparent()` count | 127 | 176 | все 49 лишних — ровно `LEGACY_*`; современный набор совпадает |
| `ItemStack.serializeAsBytes()` diamond sword | 121 B | 122 B | NBT raw размер одинаковый (130 B), разница в порядке ключей/gzip; `DataVersion=5023` добавлен |

`materials`-расхождение связано с тем, что `Material.asBlockType()` в paper-api
build 209 для legacy-констант конвертирует их в современный блок, а Paper
build 248 считает legacy-материалы не-блоками. Это различие API-сборки, а не
поведения современного блока; оно оставлено открытым и зафиксировано, чтобы
не маскировать его фильтром.

## Interpretation

The diff is a list of **open observations**, not a claim that every difference is a bug: some checks depend on chunk load order, world generation timing, or server-only formatting. It is intended as a reproducible baseline for the next vanilla-parity fixes.

