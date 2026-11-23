package net.minecraft.world.level.storage;

/** NMS {@code LevelStorageSource$LevelStorageAccess} (Pl3xMap). */
public class LevelStorageSource$LevelStorageAccess {
    public LevelStorageSource$LevelStorageAccess() {
    }

    public java.nio.file.Path getDimensionPath(net.minecraft.resources.ResourceKey<?> dimension) {
        String name = dimension == null || dimension.location() == null
                ? "" : dimension.location().getPath();
        java.nio.file.Path root = java.nio.file.Path.of("world");
        return switch (name) {
            case "the_nether" -> root.resolve("DIM-1");
            case "the_end" -> root.resolve("DIM1");
            default -> root;
        };
    }
}
