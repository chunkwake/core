package net.minecraft.world.level.saveddata.maps;

/** NMS {@code MapItemSavedData} (минимально). */
public class MapItemSavedData {
    /** Поле, которое ищет CraftEngine (getScale). */
    private int scale;
    /** Поле, которое ищет CraftEngine (getColors). */
    public byte[] colors = new byte[0];
    /** Поле, которое ищет CraftEngine (getLocked). */
    public boolean locked;

    public static final class MapPatch {
    }
}
