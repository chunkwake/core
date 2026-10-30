# Команды

Дерево команд объявлено один раз в
[`crates/net/src/play/commands/registry.rs`](../crates/net/src/play/commands/registry.rs):
из него собираются пакет `commands` для клиента, подсказки tab-complete и
проверка прав. Поэтому дерево, права и выполнение не могут разойтись.

Форма узлов совпадает с ванильным `reports/commands.json` 26.3: имена
аргументов, типы парсеров и флаг `executable`. Это проверяет
[`commands_parity.rs`](../crates/net/tests/commands_parity.rs)
(`nodes_match_vanilla_shape`), иначе клиент красит рабочую команду как ошибку.

Ванильные алиасы — это `redirect` в пакете и разворот имени при разборе:
`tell`/`w` → `msg`, `tm` → `teammsg`, `tp` → `teleport`, `xp` → `experience`.
`/execute` объявлен подкомандами (`as`, `at`, `positioned`, `rotated`,
`facing`, `in`, `if`/`unless`, `run`) с возвратом разбора в свой корень;
`align`, `anchored`, `on`, `store` обработчик пока не разбирает и в дереве
не объявлены.

Подкоманда — всегда литерал дерева, поэтому она приходит в `Call::path`, а
не в хвосте строки. Обработчик, чей узел не исполняем (например
`/time query`), выполняет ближайшего предка с обработчиком.

Уровни прав ванильные (`reports/commands.json` версии 26.3):

| Уровень | Кто | В ваниле |
|---|---|---|
| 0 | все | — |
| 2 | гейм-мастер | `gamemasters` |
| 3 | админ | `admins` |
| 4 | владелец сервера | `owners` |

## Реализовано

| Команда | Уровень | Что делает | Отличия от ванили |
|---|---|---|---|
| `help [command]` | 0 | список доступных команд или использование | без наведения и кликабельных ссылок |
| `list [uuids]` | 0 | игроки онлайн | `uuids` принимается, но не печатает UUID |
| `tps` | 0 | строка TPS/MSPT | **расширение Chunkwake** (формат для `chunkwake-lab`) |
| `top [cpu\|tick\|mem\|world\|net]` | 2 | снимок нагрузки | **расширение Chunkwake** |
| `say <message>` | 2 | сообщение от имени сервера | — |
| `me <action>` | 0 | действие от своего имени | — |
| `msg`/`tell`/`w` | 0 | личное сообщение | нет истории и `/socialspy` |
| `teammsg`/`tm` | 0 | сообщение команде | команды (`/team`) пока нет, поэтому видно всем |
| `tellraw` | 2 | компонент в чат | — |
| `title` | 2 | заголовок на экране | — |
| `playsound`/`stopsound` | 2 | звуки | позиция — у цели |
| `give` | 2 | предмет в инвентарь | без NBT-компонентов в аргументе |
| `clear` | 2 | очистить инвентарь | предмет и `maxCount` принимаются, но не фильтруют |
| `effect give\|clear` | 2 | эффекты | `infinite` есть, `hideParticles` принимается и игнорируется |
| `xp add\|set\|query` | 2 | опыт | — |
| `gamemode`/`defaultgamemode` | 2 | режим игры | — |
| `kill` | 2 | убить | мобов и предметы тоже |
| `tp`/`teleport` | 2 | телепорт | `facing entity\|pos` и `rotation` принимаются, но поворот применяется только в `/execute` |
| `damage` | 2 | урон | `fall`, `out_of_world`, `starve`, `magic`, `wither`, `arrow` |
| `rotate` | 2 | повернуть | — |
| `spawnpoint` | 2 | точка возрождения | хранится в памяти сессии |
| `setblock` | 2 | поставить блок | `destroy`/`keep` ведут себя как `replace` |
| `fill` | 2 | залить область | предел 32 768 блоков; `destroy` не дропает предметы |
| `summon` | 2 | призвать моба | корова, свинья, овца, курица |
| `weather` | 2 | погода | дождь виден, но тик осадков не считается |
| `gamerule` | 2 | правила мира | 59 правил 26.3; тик читает часть |
| `time set\|add\|query` | 2 | время | `query` — `time` и `gametime`, как в 26.3 (`daytime` ещё принимается) |
| `difficulty` | 2 | сложность | — |
| `seed` | 2 | зерно мира | — |
| `version` | 2 | версия сервера | — |
| `setworldspawn` | 2 | точка появления мира | — |
| `forceload` | 2 | чанки вне симуляции | очередь загрузки не подключена |
| `scoreboard objectives` | 2 | цели счёта | `modify` нет |
| `scoreboard players` | 2 | значения счёта | — |
| `data get block\|entity` | 2 | NBT блок-сущности или игрока | `merge`/`modify` и пути NBT — позже |
| `execute … run <command>` | 2 | цепочка контекстов | `as`, `at`, `positioned`, `rotated`, `facing`, `in`, `if`/`unless entity\|block\|score`; нет `store`, `align`, `anchored`, `on` |
| `op`/`deop` | 3 | права оператора | `ops.json` ванилы |
| `kick` | 3 | выгнать | причина попадает в ответ |
| `whitelist` | 3 | белый список | список правится, проверка при входе — позже |
| `ban`/`ban-ip`/`banlist`/`pardon`/`pardon-ip` | 3 | баны | в `gamerules.json`, проверка при входе — позже |
| `reload` | 2 | перечитать `ops.json` | датапаков нет |
| `tick` | 3 | `query`, `rate`, `freeze`, `step`, `sprint` | `sprint` ускоряет цикл |
| `debug start\|stop` | 3 | профиль тика в `debug/profile-*.txt` | запись MSPT по тикам, не `spark` |
| `perf start\|stop` | 4 | снимок нагрузки и профиль | — |
| `stop` | 4 | сохранить мир и выйти | — |
| `save-all`/`save-off`/`save-on` | 4 | автосейв | `save-all` просит автосейв вне расписания |
| `publish`/`unpublish` | 4 | LAN-публикация | только флаг, UPnP не трогаем |
| `transfer` | 3 | переслать на другой сервер | пакет `transfer` не отправляется: сессия закрывается |
| `setidletimeout` | 3 | таймаут простоя | значение хранится, отключение — позже |

## Чего пока нет

Перечислено явно: тест
[`commands_parity.rs`](../crates/net/tests/commands_parity.rs) сверяет дерево с
ванильным 26.3 и падает, если список ниже разойдётся с кодом.

`advancement`, `attribute`, `bossbar`, `clone`, `compute`, `datapack`,
`dialog`, `enchant`, `fetchprofile`, `fillbiome`, `function`, `item`, `jfr`,
`locate`, `loot`, `particle`, `place`, `posteffect`, `random`, `recipe`,
`return`, `ride`, `schedule`, `spectate`, `spreadplayers`, `stopwatch`,
`swing`, `tag`, `team`, `test`, `trigger`, `waypoint`, `worldborder`.

Часть из них требует подсистем, которых ещё нет: scoreboard в тике, загрузчик
датапаков, генератор структур, рецепты для игрока.

## Селекторы

Поддерживаются `@s`, `@p`, `@r`, `@a`, `@e` и имя игрока с аргументами:

```
@a[distance=..20,gamemode=creative]
@e[type=minecraft:cow,limit=5,sort=nearest,tag=farm]
@p[x=0,y=64,z=0,dx=10,dy=10,dz=10]
@a[name=Ali*,level=1..]
```

Разбираются `sort`, `limit`, `distance`, `dx`/`dy`/`dz`, `gamemode`, `name`,
`tag`, `scores`, `rot`; `type=` отличает моба от игрока.

## Подсказки

Литералы и аргументы клиент подсказывает сам по пакету `commands`. На
`command_suggestion` сервер отвечает тем, что клиент знать не может: имена
команд плагинов и то, что пришло от плагинов (`TabComplete`). Список
дополняется, а не заменяется: пустой ответ не затирает дерево.

## Консоль

Те же команды, что и в чате, без `/`: `chunkwake` читает `stdin` построчно.
`stop` завершает сервер, `top` печатает снимок нагрузки в лог.
