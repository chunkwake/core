package net.minecraft.core;

/** NMS {@code BlockPos} (+ изменяемый вариант). */
public class BlockPos extends Vec3i {
    public static final BlockPos ZERO = new BlockPos(0, 0, 0);

    public BlockPos(int x, int y, int z) {
        super(x, y, z);
    }

    public static BlockPos containing(double x, double y, double z) {
        return new BlockPos((int) Math.floor(x), (int) Math.floor(y), (int) Math.floor(z));
    }

    public static BlockPos of(long packed) {
        int x = (int) (packed >> 38);
        int y = (int) (packed << 52 >> 52);
        int z = (int) (packed << 26 >> 38);
        return new BlockPos(x, y, z);
    }

    @Override
    public BlockPos offset(int dx, int dy, int dz) {
        return new BlockPos(this.x + dx, this.y + dy, this.z + dz);
    }

    @Override
    public BlockPos offset(Vec3i other) {
        return this.offset(other.getX(), other.getY(), other.getZ());
    }

    @Override
    public BlockPos above() {
        return this.above(1);
    }

    @Override
    public BlockPos above(int steps) {
        return this.offset(0, steps, 0);
    }

    public BlockPos below() {
        return this.below(1);
    }

    public BlockPos below(int steps) {
        return this.offset(0, -steps, 0);
    }

    @Override
    public BlockPos relative(Direction direction) {
        return this.offset(direction.getStepX(), direction.getStepY(), direction.getStepZ());
    }

    public BlockPos relative(Direction direction, int steps) {
        return this.offset(direction.getStepX() * steps, direction.getStepY() * steps,
                direction.getStepZ() * steps);
    }

    public BlockPos north() {
        return this.relative(Direction.NORTH);
    }

    public BlockPos south() {
        return this.relative(Direction.SOUTH);
    }

    public BlockPos east() {
        return this.relative(Direction.EAST);
    }

    public BlockPos west() {
        return this.relative(Direction.WEST);
    }

    public BlockPos immutable() {
        return this;
    }

    public long asLong() {
        return pack(this.getX(), this.getY(), this.getZ());
    }

    public static long pack(int x, int y, int z) {
        return ((long) (x & 0x3FFFFFF) << 38)
                | ((long) (y & 0xFFF) << 26)
                | (z & 0x3FFFFFF);
    }

    public MutableBlockPos mutable() {
        return new MutableBlockPos(this.x, this.y, this.z);
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
            this.x = x;
            return this;
        }

        public MutableBlockPos setY(int y) {
            this.y = y;
            return this;
        }

        public MutableBlockPos setZ(int z) {
            this.z = z;
            return this;
        }

        public MutableBlockPos set(int x, int y, int z) {
            this.x = x;
            this.y = y;
            this.z = z;
            return this;
        }

        public MutableBlockPos move(int dx, int dy, int dz) {
            this.x += dx;
            this.y += dy;
            this.z += dz;
            return this;
        }

        public MutableBlockPos move(Direction direction) {
            return this.move(direction.getStepX(), direction.getStepY(), direction.getStepZ());
        }
    }
}
