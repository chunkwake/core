package net.minecraft.world.level.border;

/** NMS {@code WorldBorder}: границы мира (squaremap). */
public class WorldBorder {
    public static final double MAX_SIZE = 29_999_984.0D;

    private double centerX;
    private double centerZ;
    private double size = MAX_SIZE;

    public WorldBorder() {
    }

    public double getCenterX() {
        return this.centerX;
    }

    public double getCenterZ() {
        return this.centerZ;
    }

    public double getSize() {
        return this.size;
    }

    public void setCenter(double x, double z) {
        this.centerX = x;
        this.centerZ = z;
    }

    public void setSize(double size) {
        this.size = size;
    }

    public double getMinX() {
        return this.centerX - this.size / 2.0D;
    }

    public double getMinZ() {
        return this.centerZ - this.size / 2.0D;
    }

    public double getMaxX() {
        return this.centerX + this.size / 2.0D;
    }

    public double getMaxZ() {
        return this.centerZ + this.size / 2.0D;
    }
}
