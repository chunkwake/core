package net.minecraft.network.chat.numbers;

/** NMS {@code BlankFormat}: пустой формат чисел (EliteMobs). */
public final class BlankFormat implements NumberFormat {
    public static final BlankFormat INSTANCE = new BlankFormat();

    private BlankFormat() {
    }
}
