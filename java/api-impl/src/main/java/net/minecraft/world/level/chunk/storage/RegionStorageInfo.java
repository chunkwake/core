package net.minecraft.world.level.chunk.storage;

/** NMS {@code RegionStorageInfo}. */
public class RegionStorageInfo {
    private final String level;
    private final String dimension;

    public RegionStorageInfo(String level, String dimension) {
        this.level = level;
        this.dimension = dimension;
    }

    public String level() {
        return this.level;
    }

    public String dimension() {
        return this.dimension;
    }
}
