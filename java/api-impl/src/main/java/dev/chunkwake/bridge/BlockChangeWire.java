package dev.chunkwake.bridge;

/** Общий формат строки `blocks x y z state …` для `Player.sendBlockChanges`. */
final class BlockChangeWire {
    /** Сколько блоков в одном запросе: ответ моста ограничен 8 КиБ. */
    static final int BATCH = 64;

    private BlockChangeWire() {
    }

    static void append(StringBuilder out, int x, int y, int z, String state) {
        out.append(' ').append(x)
                .append(' ').append(y)
                .append(' ').append(z)
                .append(' ').append(state);
    }
}
