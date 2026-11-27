package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

/** `Player.sendBlockChanges`: формат строки и размер пачки. */
final class BlockChangeWireTest {
    @Test
    void appendsCoordinatesAndState() {
        StringBuilder line = new StringBuilder("blocks");
        BlockChangeWire.append(line, 1, 2, 3, "minecraft:stone");
        BlockChangeWire.append(line, -4, 64, 7, "minecraft:oak_stairs[facing=east]");
        assertEquals(
                "blocks 1 2 3 minecraft:stone"
                        + " -4 64 7 minecraft:oak_stairs[facing=east]",
                line.toString());
        assertEquals(64, BlockChangeWire.BATCH);
    }
}
