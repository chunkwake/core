package net.minecraft.core;

/** NMS {@code Vec3i}. */
public class Vec3i {
    public static final Vec3i ZERO = new Vec3i(0, 0, 0);
    protected int x;
    protected int y;
    protected int z;

    public Vec3i(int x, int y, int z) {
        this.x = x;
        this.y = y;
        this.z = z;
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

    public Vec3i offset(int dx, int dy, int dz) {
        return new Vec3i(this.x + dx, this.y + dy, this.z + dz);
    }

    public Vec3i offset(Vec3i other) {
        return this.offset(other.x, other.y, other.z);
    }

    public Vec3i above() {
        return this.above(1);
    }

    public Vec3i above(int steps) {
        return this.offset(0, steps, 0);
    }

    public Vec3i relative(Direction direction) {
        return this.offset(direction.getStepX(), direction.getStepY(), direction.getStepZ());
    }

    public long asLong() {
        return ((long) this.x & 0x3FFFFFFL) << 38
                | ((long) this.z & 0x3FFFFFFL) << 12
                | ((long) this.y & 0xFFFL);
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof Vec3i v && v.x == this.x && v.y == this.y && v.z == this.z;
    }

    @Override
    public int hashCode() {
        return (this.y + this.z * 31) * 31 + this.x;
    }

    @Override
    public String toString() {
        return "Vec3i[" + this.x + ", " + this.y + ", " + this.z + "]";
    }
}
