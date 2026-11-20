package net.minecraft.world.level.levelgen;

/** NMS Heightmap: нужен только тип {@code Heightmap.Types} для LevelReader. */
public final class Heightmap {
    public enum Types {
        WORLD_SURFACE,
        OCEAN_FLOOR,
        MOTION_BLOCKING,
        MOTION_BLOCKING_NO_LEAVES
    }

    private Heightmap() {
    }
}
