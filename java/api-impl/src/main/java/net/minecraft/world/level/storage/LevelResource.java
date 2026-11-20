package net.minecraft.world.level.storage;

/** NMS {@code LevelResource}. */
public class LevelResource {
    public static final LevelResource ROOT = new LevelResource("");

    private final String name;

    public LevelResource(String name) {
        this.name = name;
    }

    public String getName() {
        return this.name;
    }

    @Override
    public String toString() {
        return this.name;
    }
}
