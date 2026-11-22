package net.minecraft.util;

/** NMS {@code Prediction}. */
public enum Prediction {
    PREDICTED,
    SERVER_ONLY;

    public static Prediction getServerOnly() {
        return SERVER_ONLY;
    }
}
