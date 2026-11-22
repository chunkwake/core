package net.minecraft.server.dedicated;

/** NMS {@code DedicatedServerSettings} (CraftEngine). */
public class DedicatedServerSettings {
    /** Поле, которое ищет CraftEngine (getProperties). */
    private final DedicatedServerProperties properties = new DedicatedServerProperties();

    public DedicatedServerSettings() {
    }

    public DedicatedServerProperties getProperties() {
        return this.properties;
    }
}
