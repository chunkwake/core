package net.minecraft.network.chat;

/** Минимальный NMS {@code TextColor}. */
public final class TextColor {
    private final int rgb;

    private TextColor(int rgb) {
        this.rgb = rgb & 0xFFFFFF;
    }

    public static TextColor fromRgb(int rgb) {
        return new TextColor(rgb);
    }

    public int getValue() {
        return this.rgb;
    }
}
