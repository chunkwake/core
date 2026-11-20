package net.minecraft.world.level.levelgen;

import java.util.function.Predicate;
import net.minecraft.util.Mth;
import net.minecraft.util.SimpleBitStorage;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.block.state.BlockState;

/** NMS {@code Heightmap}: карта высот чанка. */
public final class Heightmap {
    public enum Types {
        WORLD_SURFACE(true),
        OCEAN_FLOOR(false),
        MOTION_BLOCKING(true),
        MOTION_BLOCKING_NO_LEAVES(true);

        private final boolean opaque;

        Types(boolean opaque) {
            this.opaque = opaque;
        }

        public Predicate<BlockState> isOpaque() {
            if (!this.opaque) {
                return state -> !state.isAir();
            }
            return state -> !state.isAir();
        }

        public String getSerializationKey() {
            return this.name().toLowerCase(java.util.Locale.ROOT);
        }
    }

    private final Types type;
    private final long[] raw;

    public Heightmap(Types type, LevelHeightAccessor heightAccessor) {
        this.type = type;
        int bits = Mth.ceillog2(heightAccessor.getHeight() + 1);
        this.raw = new long[(int) (((long) 256 * bits + 63L) / 64L)];
    }

    public Types type() {
        return this.type;
    }

    public long[] getRawData() {
        return this.raw;
    }

    public int getFirstAvailable(int x, int z) {
        return 0;
    }

    public int getHeight(int x, int z) {
        return 0;
    }

    public void setHeight(int x, int z, int height) {
    }

    public void update(int x, int y, int z, BlockState state) {
    }
}
