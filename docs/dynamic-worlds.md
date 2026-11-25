# Динамические миры (`WorldCreator` / Multiverse)

`Bukkit.createWorld(WorldCreator)` и `Multiverse-Core` теперь могут создать
новый мир на Chunkwake. Раньше `CraftServer.createWorld` уходил в
`ChunkwakeServer.createWorld`, которого не было, и возвращал `null`
(`WORLD_CREATOR_FAILED` у Multiverse).

## Как работает

- В `PlayPlan` предвыделены `CUSTOM_SLOTS` (8) измерений
  `Dimension::Custom(3..)`. Они не тикают и не попадают в `Bukkit.getWorlds`,
  пока плагин не создаст мир.
- `createworld <name>` в Rust занимает свободный слот, очищает `DimState`,
  регистрирует ключ `minecraft:<name>` и возвращает его.
- Java `ChunkwakeServer.createWorld` создаёт каталог
  `world/dimensions/minecraft/<name>`, сбрасывает кэш `worlds` и отдаёт
  `WorldImpl`; `WorldCreator.createWorld()` возвращает обычный `World`.
- `Bukkit.unloadWorld`/`Multiverse` unload вызывают `unloadworld`, который
  гасит слот и освобождает имя.
- Динамический мир использует шаблон/генератор оверворлда (для flat-сервера —
  flat, для обычного — тот же генератор). Это независимый `DimState`:
  блоки, чанки и правки не пересекаются с оверворлдом.

## Проверка

`BehaviorProbe` (локальный, не входит в репозиторий) на Paper и Chunkwake:

```text
Paper:
BEHAVIOR worldedit placed=16 verified=16
BEHAVIOR multiverse created=true loaded=true normal=true deleted=true

Chunkwake:
BEHAVIOR worldedit placed=16 verified=16
BEHAVIOR bukkit-world created=true
BEHAVIOR worldinfo name=behaviorprobe key=minecraft:behaviorprobe
BEHAVIOR dynamic-edit verified=16 isolated=true
BEHAVIOR multiverse created=true loaded=true normal=true deleted=true delete-reason=ok
BEHAVIOR after-delete world=null
```

`dynamic-edit isolated=true` — WorldEdit-правка в новом мире не изменила
оверворлд.

Persistence (отдельный прогон, один и тот же стенд):

```text
BEHAVIOR persist mode=write edited=16
BEHAVIOR persist mode=check block=STONE
```

Чанки пишутся в `world/dimensions/minecraft/<name>/region` и читаются обратно
при загрузке чанка (`getChunkAt`/`loadchunk`). `getBlockAt` в динамическом
мире сам подгружает сохранённый чанк (`WorldImpl.ensureChunk`): прогон
`mode=check` без `getChunkAt` вернул `block=STONE`.

## Ограничения

- Мир использует генератор/сид оверворлда; независимый seed, generator,
  biome provider и `WorldCreator`-настройки пока не применяются.
- Persistence работает для загруженных чанков: регионы пишутся под
  `minecraft:<name>`, читаются при `getChunkAt`/`loadchunk` и при первом
  `getBlockAt`. Полного `level.dat`/spawn/entity-состояния пока нет.
- Multiverse `deleteWorld` теперь возвращает `deleted=true`: в каталог
  мира копируется `level.dat` (иначе `WorldNameChecker` не считает папку
  миром), а `unloadWorld` скрывает мир из `getWorld(String)`/`getWorlds`,
  но оставляет его в UID-таблице до завершения удаления.
- Слотов 8; после исчерпания `WorldCreator.createWorld` вернёт `null`, пока
  слот не освободят.
