package net.minecraft;

/**
 * NMS {@code SharedConstants}. AxGraves (AxAPI) берёт отсюда номер
 * сетевого протокола и по нему выбирает NMS-handler для 26.3.
 */
public final class SharedConstants {
    public static final String VERSION = "26.3";
    public static final int WORLD_VERSION = 5023;
    public static final String SERIES = "main";
    public static final int RELEASE_NETWORK_PROTOCOL_VERSION = 777;
    public static final int SNAPSHOT_NETWORK_PROTOCOL_VERSION = 338;

    private SharedConstants() {
    }

    public static net.minecraft.WorldVersion getCurrentVersion() {
        return net.minecraft.WorldVersion.Current.INSTANCE;
    }

    public static int getProtocolVersion() {
        return RELEASE_NETWORK_PROTOCOL_VERSION;
    }

    /** Старое (обфусцированное) имя метода в 26.3. */
    public static int c() {
        return RELEASE_NETWORK_PROTOCOL_VERSION;
    }
}
