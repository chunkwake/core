package net.minecraft.world.phys;

/** NMS {@code Vec3}. */
public class Vec3 {
    public static final Vec3 ZERO = new Vec3(0, 0, 0);
    public final double x;
    public final double y;
    public final double z;

    public Vec3(double x, double y, double z) {
        this.x = x;
        this.y = y;
        this.z = z;
    }

    public double x() {
        return this.x;
    }

    public double y() {
        return this.y;
    }

    public double z() {
        return this.z;
    }

    /** {@code Vec3.subtract(double,double,double)} (CraftEngine). */
    public Vec3 subtract(double dx, double dy, double dz) {
        return new Vec3(this.x - dx, this.y - dy, this.z - dz);
    }

    public Vec3 subtract(double amount) {
        return this.subtract(amount, amount, amount);
    }

    /** {@code Vec3.multiply(double,double,double)} (CraftEngine). */
    public Vec3 multiply(double fx, double fy, double fz) {
        return new Vec3(this.x * fx, this.y * fy, this.z * fz);
    }

    /** {@code Vec3.multiply(Vec3)} (CraftEngine). */
    public Vec3 multiply(Vec3 other) {
        return this.multiply(other.x, other.y, other.z);
    }

    public Vec3 add(double dx, double dy, double dz) {
        return new Vec3(this.x + dx, this.y + dy, this.z + dz);
    }

    public Vec3 add(Vec3 other) {
        return this.add(other.x, other.y, other.z);
    }

    public Vec3 subtract(Vec3 other) {
        return new Vec3(this.x - other.x, this.y - other.y, this.z - other.z);
    }

    public double distanceTo(Vec3 other) {
        return Math.sqrt(this.distanceToSqr(other));
    }

    public double distanceToSqr(Vec3 other) {
        double dx = this.x - other.x;
        double dy = this.y - other.y;
        double dz = this.z - other.z;
        return dx * dx + dy * dy + dz * dz;
    }

    public double length() {
        return Math.sqrt(this.x * this.x + this.y * this.y + this.z * this.z);
    }

    @Override
    public String toString() {
        return "Vec3[" + this.x + ", " + this.y + ", " + this.z + "]";
    }
}
