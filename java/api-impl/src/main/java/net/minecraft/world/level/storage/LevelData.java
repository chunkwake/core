package net.minecraft.world.level.storage;

import net.minecraft.core.BlockPos;

/** NMS {@code LevelData}: данные уровня (26.3 — интерфейс). */
public interface LevelData {
    RespawnData getRespawnData();

    /** NMS {@code LevelData.RespawnData}: точка возрождения. */
    final class RespawnData {
        public static final RespawnData DEFAULT =
                new RespawnData(BlockPos.ZERO, 0.0F, 0.0F);

        private final BlockPos pos;
        private final float yaw;
        private final float pitch;

        public RespawnData(BlockPos pos, float yaw, float pitch) {
            this.pos = pos == null ? BlockPos.ZERO : pos;
            this.yaw = yaw;
            this.pitch = pitch;
        }

        public BlockPos pos() {
            return this.pos;
        }

        public float yaw() {
            return this.yaw;
        }

        public float pitch() {
            return this.pitch;
        }
    }
}
