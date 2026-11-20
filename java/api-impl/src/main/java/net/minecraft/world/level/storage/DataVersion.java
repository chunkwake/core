package net.minecraft.world.level.storage;

/** NMS {@code DataVersion}. */
public class DataVersion {
    private final int version;

    public DataVersion(int version) {
        this.version = version;
    }

    public int version() {
        return this.version;
    }

    @Override
    public String toString() {
        return Integer.toString(this.version);
    }
}
