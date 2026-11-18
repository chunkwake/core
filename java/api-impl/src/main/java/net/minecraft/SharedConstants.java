package net.minecraft;

/** NMS {@code SharedConstants}: версия и протокол. */
public final class SharedConstants {
    public static final String VERSION = "26.3";
    private static final int PROTOCOL = 800;

    private SharedConstants() {
    }

    public static int getProtocolVersion() {
        return PROTOCOL;
    }

    public static String getCurrentVersion() {
        return VERSION;
    }

    public static String getVersionName() {
        return VERSION;
    }
}
