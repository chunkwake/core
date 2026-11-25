# Динамические миры (`WorldCreator` / Multiverse)

`Bukkit.createWorld(WorldCreator)` и `Multiverse-Core` теперь могут создать
новый мир на Chunkwake. Раньше `CraftServer.createWorld` уходил в
`ChunkwakeServer.createWorld`, которого не было, и возвращал `null`
(`WORLD_CREATOR_FAILED` у Multiverse).

## Как работает

- В `PlayPlan` предвыделены `CUSTOM_SLOTS` (8) измерений
  `Dimension::Custom(3..)`. Они не тикают и не попадают в `Bukkit.getWorlds`,
  пока плагин не создаст мир.
- `createworld <name> [seed]` в Rust занимает свободный слот, очищает
  `DimState`, регистрирует ключ `minecraft:<name>` и возвращает его.
  Одноаргументный `createworld <name>` (старый формат) тоже принимается.
- Java `ChunkwakeServer.createWorld` передаёт `WorldCreator.seed()`. Если сид
  `null`, Rust выводит стабильный сид из имени мира (FNV-1a): после
  перезапуска и повторного создания мир получает тот же рельеф.
- Для шумового базового мира `createWorld` лениво собирает отдельный
  `chunkwake_worldgen::pipeline::Overworld` с тем же `noise_settings` и
  биомным источником, что у оверворлда, но со своим сидом. Генератор ставится
  в `DimState` и отвечает за чанки, `/locate biome` и `/locate structure`.
  Восемь предвыделенных слотов не держат восемь копий генератора: он
  строится только при создании мира.
- Java `ChunkwakeServer.createWorld` создаёт каталог
  `world/dimensions/minecraft/<name>`, сбрасывает кэш `worlds` и отдаёт
  `WorldImpl`; `WorldCreator.createWorld()` возвращает обычный `World`.
- `Bukkit.unloadWorld`/`Multiverse` unload вызывают `unloadworld`, который
  гасит слот, освобождает имя и отпускает генератор (`DynamicGen`).
- После создания динамического мира Java шлёт Paper-события `WorldInitEvent`
  и `WorldLoadEvent` (в потоке плагина, вызвавшего `createWorld`). Перед
  выгрузкой шлётся отменяемый `WorldUnloadEvent`: при отмене
  `Bukkit.unloadWorld` возвращает `false`, мир остаётся загруженным, Rust
  `unloadworld` не вызывается. Это нужно Multiverse: по событию он обновляет
  свой `WorldStore`, и последующий `deleteWorld` не падает с `REMOVE_FAILED`.
  Ошибки слушателей логируются, выгрузка/создание продолжаются.
- Динамический мир использует шаблон/тип мира базового уровня (для
  flat-сервера — flat, для обычного — шумовой оверворлд с тем же
  `generator-settings`), но свой сид. Это независимый `DimState`: блоки,
  чанки и правки не пересекаются с оверворлдом. Сохранённые чанки с диска
  по-прежнему важнее генерации.
- `World.getSeed()` и `/seed` в динамическом мире возвращают его сид.

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

Финальный прогон с `WorldCreator.seed()` и событиями жизненного цикла
(чистый стенд, релизный бинарь 25.11.2026):

```text
BEHAVIOR seed a=12345 b=54321 surface-a=0@WATER,0@CLAY,0@DIORITE, surface-b=0@DEEPSLATE,0@DEEPSLATE,0@DEEPSLATE, independent=true
BEHAVIOR unload=true
BEHAVIOR multiverse created=true reason=ok loaded=true normal=true deleted=true delete-reason=ok
BEHAVIOR after-delete world=null
```

`independent=true` — два мира с разными сидами дали разный рельеф;
`World.getSeed()` вернул 12345 и 54321. `WorldUnloadEvent` теперь доходит до
Multiverse, поэтому ручная выгрузка перед `deleteWorld` больше не оставляет
`REMOVE_FAILED`.

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

- Поддерживается независимый **сид** шумового мира: тот же тип и
  `noise_settings`, что у базового оверворлда. `WorldCreator.environment()` и
  `WorldCreator.type()` не меняют тип динамического мира: слот наследует
  измерение/шаблон оверворлда. Создать в обычном мире отдельный flat-мир или
  мир Ада/Края через `WorldCreator` пока нельзя.
- `WorldCreator.generatorSettings()` и кастомный `ChunkGenerator` не
  применяются: используется `noise_settings` базового уровня.
- `World.getSeed()` возвращает сид динамического мира, но скопированный в
  каталог мира `level.dat` по-прежнему содержит сид базового мира (нужен для
  Multiverse/BlueMap). Плагины, читающие `level.dat` напрямую, увидят чужой сид.
- Мир не восстанавливается автоматически после перезапуска сервера: его
  должен снова создать плагин (например, Multiverse). При повторном создании
  с тем же именем производный сид совпадает, поэтому сохранённые чанки
  остаются на месте.
- Persistence работает для загруженных чанков: регионы пишутся под
  `minecraft:<name>`, читаются при `getChunkAt`/`loadchunk` и при первом
  `getBlockAt`. Полного `level.dat`/spawn/entity-состояния пока нет.
- Multiverse `deleteWorld` теперь возвращает `deleted=true`: в каталог
  мира копируется `level.dat` (иначе `WorldNameChecker` не считает папку
  миром), а `unloadWorld` скрывает мир из `getWorld(String)`/`getWorlds`,
  но оставляет его в UID-таблице до завершения удаления.
- События `WorldInitEvent`/`WorldLoadEvent` шлются только для миров,
  созданных через `Bukkit.createWorld`/`WorldCreator`. Стартовые оверворлд,
  Ад и Край этих событий не получают (как и раньше, при запуске сервера).
- Слотов 8; после исчерпания `WorldCreator.createWorld` вернёт `null`, пока
  слот не освободят. Построение генератора идёт в момент создания мира и
  может занять заметное время (как загрузка мира в Paper).
