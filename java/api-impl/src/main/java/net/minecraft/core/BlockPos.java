package net.minecraft.core;

/** NMS {@code BlockPos}. */
public class BlockPos {
    public static final BlockPos ZERO = new BlockPos(0, 0, 0);

    private final int x;
    private final int y;
    private final int z;

    public BlockPos(int x, int y, int z) {
        this.x = x;
        this.y = y;
        this.z = z;
    }

    public static BlockPos containing(double x, double y, double z) {
        return new BlockPos((int) Math.floor(x), (int) Math.floor(y), (int) Math.floor(z));
    }

    public int getX() {
        return this.x;
    }

    public int getY() {
        return this.y;
    }

    public int getZ() {
        return this.z;
    }

    public long asLong() {
        return ((long) this.x & 0x3FFFFFFL) << 38
                | ((long) this.z & 0x3FFFFFFL) << 12
                | ((long) this.y & 0xFFFL);
    }

    public static BlockPos of(long packed) {
        int x = (int) (packed >> 38);
        int y = (int) (packed << 52 >> 52);
        int z = (int) (packed << 26 >> 38);
        return new BlockPos(x, y, z);
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof BlockPos p && p.x == this.x && p.y == this.y && p.z == this.z;
    }

    @Override
    public int hashCode() {
        return (this.y + this.z * 31) * 31 + this.x;
    }

    @Override
    public String toString() {
        return "BlockPos[" + this.x + ", " + this.y + ", " + this.z + "]";
    }

    /** Изменяемая позиция (NMS {@code BlockPos.MutableBlockPos}). */
    public static class MutableBlockPos extends BlockPos {
        public MutableBlockPos() {
            this(0, 0, 0);
        }

        public MutableBlockPos(int x, int y, int z) {
            super(x, y, z);
        }

        public MutableBlockPos setX(int x) {
            return this.set(x, this.getY(), this.getZ());
        }

        public MutableBlockPos setY(int y) {
            return this.set(this.getX(), y, this.getZ());
        }

        public MutableBlockPos setZ(int z) {
            return this.set(this.getX(), this.getY(), z);
        }

        public MutableBlockPos set(int x, int y, int z) {
            return new MutableBlockPos(x, y, z);
        }

        public MutableBlockPos move(int dx, int dy, int dz) {
            return this.set(this.getX() + dx, this.getY() + dy, this.getZ() + dz);
        }
    }
}
