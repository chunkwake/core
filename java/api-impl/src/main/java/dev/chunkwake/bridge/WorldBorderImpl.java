package dev.chunkwake.bridge;

import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.WorldBorder;

/**
 * Функциональный {@link WorldBorder} (ванильные значения и поведение API).
 * Хранит состояние в памяти; визуальная граница пока не синхронизируется с
 * Rust-миром.
 */
final class WorldBorderImpl implements WorldBorder {
    private static final double MAX_SIZE = 29999984.0;

    private final World world;
    private double size = MAX_SIZE;
    private double centerX;
    private double centerZ;
    private double damageBuffer = 5.0;
    private double damageAmount = 0.2;
    private int warningTime = 15;
    private int warningDistance = 5;

    WorldBorderImpl(World world) {
        this.world = world;
    }

    @Override public World getWorld() { return this.world; }

    @Override public void reset() {
        this.size = MAX_SIZE;
        this.centerX = 0.0;
        this.centerZ = 0.0;
        this.damageBuffer = 5.0;
        this.damageAmount = 0.2;
        this.warningTime = 15;
        this.warningDistance = 5;
    }

    @Override public double getSize() { return this.size; }

    @Override public void setSize(double size) { this.size = clampSize(size); }

    @Override public void changeSize(double size, long seconds) {
        setSize(size);
    }

    @Override public Location getCenter() {
        return new Location(this.world, this.centerX, 0.0, this.centerZ);
    }

    @Override public void setCenter(double x, double z) {
        this.centerX = clampCenter(x);
        this.centerZ = clampCenter(z);
    }

    @Override public void setCenter(Location location) {
        if (location != null) {
            setCenter(location.getX(), location.getZ());
        }
    }

    @Override public double getDamageBuffer() { return this.damageBuffer; }

    @Override public void setDamageBuffer(double blocks) { this.damageBuffer = Math.max(0.0, blocks); }

    @Override public double getDamageAmount() { return this.damageAmount; }

    @Override public void setDamageAmount(double damage) { this.damageAmount = Math.max(0.0, damage); }

    @Override public int getWarningTimeTicks() { return this.warningTime; }

    @Override public void setWarningTimeTicks(int ticks) { this.warningTime = Math.max(0, ticks); }

    @Override public int getWarningDistance() { return this.warningDistance; }

    @Override public void setWarningDistance(int distance) { this.warningDistance = Math.max(0, distance); }

    @Override public boolean isInside(Location location) {
        if (location == null) {
            return false;
        }
        double half = this.size / 2.0;
        return Math.abs(location.getX() - this.centerX) < half
                && Math.abs(location.getZ() - this.centerZ) < half;
    }

    @Override public double getMaxSize() { return MAX_SIZE; }

    @Override public double getMaxCenterCoordinate() { return MAX_SIZE / 2.0; }

    private static double clampSize(double value) {
        return Math.max(1.0, Math.min(MAX_SIZE, value));
    }

    private static double clampCenter(double value) {
        double half = MAX_SIZE / 2.0;
        return Math.max(-half, Math.min(half, value));
    }
}
