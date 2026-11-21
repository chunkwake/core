package net.minecraft.world.level.dimension;

/** Заглушка NMS DimensionType (LevelReader.dimensionType). */
public class DimensionType {
    /** Поле, которое ищет CraftEngine (getMinY). */
    public int minY = -64;

    public DimensionType() {
    }

    public boolean hasCeiling() {
        return false;
    }

    public java.nio.file.Path getStorageFolder(
            net.minecraft.resources.ResourceKey<net.minecraft.world.level.Level> key,
            java.nio.file.Path root) {
        if (key == null) {
            return root;
        }
        return root.resolve("dimensions")
                .resolve(key.location().getNamespace())
                .resolve(key.location().getPath());
    }
}
