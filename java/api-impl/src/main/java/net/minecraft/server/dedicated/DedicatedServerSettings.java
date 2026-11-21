package net.minecraft.server.dedicated;

/** NMS {@code DedicatedServerSettings} (CraftEngine). */
public class DedicatedServerSettings {
    /** Поле, которое ищет CraftEngine (getProperties). */
    private final java.util.Properties properties = new java.util.Properties();

    public DedicatedServerSettings() {
    }

    public java.util.Properties getProperties() {
        return this.properties;
    }
}
