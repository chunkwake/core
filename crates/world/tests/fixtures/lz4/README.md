# Фикстуры lz4-java

`empty.lz4` и `mixed.lz4` созданы настоящим `LZ4BlockOutputStream` из
`org.lz4:lz4-java:1.8.0`, с размером блока 1024 байта (уровень 0).
`mixed.nbt` — несжатый эталон: корневой compound с ByteArray `payload`.
Псевдослучайное начало и повторяющийся хвост дают RAW- и LZ4-блоки,
несколько полных блоков и неполный последний блок. `empty.lz4` содержит
только терминатор. Во время тестов Java и сеть не нужны.

Перегенерация из корня репозитория (Java 11+):

```sh
curl -fLo /tmp/lz4-java-1.8.0.jar https://repo.maven.apache.org/maven2/org/lz4/lz4-java/1.8.0/lz4-java-1.8.0.jar
java --class-path /tmp/lz4-java-1.8.0.jar crates/world/tests/fixtures/lz4/Generate.java crates/world/tests/fixtures/lz4
```

SHA-256 jar: `d74a3334fb35195009b338a951f918203d6bbca3d1d359033dc33edd1cadc9ef`.
Генератор дополнительно сверяет результат через `LZ4BlockInputStream`.
Правила заголовка сверены с
[LZ4BlockInputStream 1.8.0](https://github.com/lz4/lz4-java/blob/1.8.0/src/java/net/jpountz/lz4/LZ4BlockInputStream.java).

Интеграционный тест оборачивает Java-поток в регион Anvil, проверяет чтение
через `RegionStore` внутри `.mca` и из внешнего `.mcc`, а затем проверяет
ошибку повреждённого терминатора. Сравнивается весь NBT, не только размер.
