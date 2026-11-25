# Console-command parity: real plugins

Один и тот же набор плагинов на Paper 26.3 и Chunkwake `d095970`:
WorldEdit, Multiverse-Core, PlaceholderAPI, Chunky, BlueMap, ViaVersion,
Plan, Skript, LuckPerms, TAB. Каждая команда отправлена в консоль
по одной, вывод вырезан из `logs/latest.log`.

| команда | Paper | Chunkwake | ожидаемый ответ |
|---|---|---|---|
| `worldedit version` | ok | ok | `7.4.6` |
| `mv list` | ok | ok | `multiverse world list` |
| `papi list` | ok | ok | `placeholder hook(s) are active` |
| `chunky` | ok | ok | `chunky commands` |
| `bluemap` | ok | ok | `bluemap is not loaded` |
| `viaversion` | ok | ok | `5.12.3` |
| `plan` | ok | ok | `view the server page` |
| `skript info` | ok | ok | `skript` |
| `lp info` | ok | ok | `luckperms` |
| `tab` | ok | ok | `/tab reload` |

Итог: **10/10** команд распознаны и
вернули ожидаемый ответ на обеих сторонах.

`bluemap` до фикса падал на Chunkwake с `NullPointerException` в
`CommandSender.spigot().sendMessage(BaseComponent[])`; добавлен
`Console.spigot()` с BaseComponent-пересылкой в лог. Теперь обе стороны
отвечают `BlueMap is not loaded!`.

Сырые срезы: `benchmarks/plugin-commands/paper.txt` и
`benchmarks/plugin-commands/chunkwake.txt`.

Ограничение: проверяется распознавание/ответ 10 команд, а не полная
семантика каждого плагина.

