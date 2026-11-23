package net.minecraft.server.level;

/**
 * NMS {@code ServerLevel$EntityCallbacks} (CraftEngine entity world tracking).
 * CE reflects a non-static {@link ServerLevel} field and hooks tracking methods.
 */
public class ServerLevel$EntityCallbacks {
    public final ServerLevel level;

    public ServerLevel$EntityCallbacks(ServerLevel level) {
        this.level = level;
    }

    public void onTrackingStart(net.minecraft.world.entity.Entity entity) {
    }

    public void onTrackingEnd(net.minecraft.world.entity.Entity entity) {
    }
}
