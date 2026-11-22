package net.minecraft.world.level;

/** NMS {@code WorldGenLevel}: уровень на этапе генерации. */
public interface WorldGenLevel extends LevelAccessor {
    long getSeed();

    default net.minecraft.server.level.ServerLevel getLevel() {
        return null;
    }
}
