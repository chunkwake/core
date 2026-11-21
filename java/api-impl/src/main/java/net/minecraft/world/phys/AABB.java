package net.minecraft.world.phys;

/** Заглушка NMS AABB: оси-выровненный бокс. */
public class AABB {
    public static final AABB INFINITE = new AABB(
            Double.NEGATIVE_INFINITY, Double.NEGATIVE_INFINITY, Double.NEGATIVE_INFINITY,
            Double.POSITIVE_INFINITY, Double.POSITIVE_INFINITY, Double.POSITIVE_INFINITY);

    public final double minX;
    public final double minY;
    public final double minZ;
    public final double maxX;
    public final double maxY;
    public final double maxZ;

    /** {@code AABB.ofSize(Vec3,double,double,double)} (CraftEngine). */
    public static AABB ofSize(Vec3 center, double dx, double dy, double dz) {
        return new AABB(center.x - dx / 2, center.y - dy / 2, center.z - dz / 2,
                center.x + dx / 2, center.y + dy / 2, center.z + dz / 2);
    }

    /** {@code AABB.move(Vec3)} (CraftEngine). */
    public AABB move(Vec3 offset) {
        return this.move(offset.x, offset.y, offset.z);
    }

    /** {@code AABB.move(double,double,double)} (CraftEngine). */
    public AABB move(double dx, double dy, double dz) {
        return new AABB(this.minX + dx, this.minY + dy, this.minZ + dz,
                this.maxX + dx, this.maxY + dy, this.maxZ + dz);
    }

    /** {@code AABB.setMaxX(double)} (CraftEngine). */
    public AABB setMaxX(double maxX) {
        return new AABB(this.minX, this.minY, this.minZ, maxX, this.maxY, this.maxZ);
    }

    /** {@code AABB.setMinZ(double)} (CraftEngine). */
    public AABB setMinZ(double minZ) {
        return new AABB(this.minX, this.minY, minZ, this.maxX, this.maxY, this.maxZ);
    }

    /** {@code AABB.setMinY(double)} (CraftEngine). */
    public AABB setMinY(double minY) {
        return new AABB(this.minX, minY, this.minZ, this.maxX, this.maxY, this.maxZ);
    }

    /** {@code AABB.setMinX(double)} (CraftEngine). */
    public AABB setMinX(double minX) {
        return new AABB(minX, this.minY, this.minZ, this.maxX, this.maxY, this.maxZ);
    }

    public AABB(double minX, double minY, double minZ, double maxX, double maxY, double maxZ) {
        this.minX = minX;
        this.minY = minY;
        this.minZ = minZ;
        this.maxX = maxX;
        this.maxY = maxY;
        this.maxZ = maxZ;
    }
}
