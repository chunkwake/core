package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.bukkit.Material;
import org.bukkit.block.data.BlockData;
import org.junit.jupiter.api.Test;

/** Legacy data byte по состоянию блока (CraftMagicNumbers). */
final class LegacyDataTest {
    /** BlockData без Bukkit-сервера: canonical() читает только getAsString(). */
    private static BlockData data(String state) {
        return (BlockData) java.lang.reflect.Proxy.newProxyInstance(
                LegacyDataTest.class.getClassLoader(),
                new Class<?>[] {BlockData.class},
                (proxy, method, args) -> switch (method.getName()) {
                    case "getAsString" -> state;
                    case "getMaterial" -> Material.STONE;
                    case "clone" -> proxy;
                    case "hashCode" -> 0;
                    case "equals" -> proxy == args[0];
                    case "toString" -> state;
                    default -> null;
                });
    }

    @Test
    void legacyDataMatchesFlattening() {
        assertEquals(0, ChunkSnapshotImpl.legacyData(data("minecraft:stone")));
        assertEquals(4, ChunkSnapshotImpl.legacyData(data("minecraft:oak_log[axis=x]")));
        assertEquals(0, ChunkSnapshotImpl.legacyData(data("minecraft:oak_log[axis=y]")));
        assertEquals(8, ChunkSnapshotImpl.legacyData(data("minecraft:oak_log[axis=z]")));
        assertEquals(0, ChunkSnapshotImpl.legacyData(null));
    }
}
