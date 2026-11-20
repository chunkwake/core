package net.minecraft.server;

/** NMS {@code ServerTickRateManager}. */
public class ServerTickRateManager {
    /** Один тик в наносекундах при 20 TPS. */
    public long nanosecondsPerTick() {
        return 50_000_000L;
    }

    public boolean isSprinting() {
        return false;
    }

    public boolean isFrozen() {
        return false;
    }
}
