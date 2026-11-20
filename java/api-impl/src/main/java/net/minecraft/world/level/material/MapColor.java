package net.minecraft.world.level.material;

/** NMS {@code MapColor}: цвет блока на карте (squaremap читает поле col). */
public class MapColor {
    public static final MapColor NONE = new MapColor(0, 0);
    public static final MapColor PLANT = new MapColor(7, 0x7FB238);
    public static final MapColor WATER = new MapColor(12, 0x3F76E4);

    /** Упакованный ARGB/RGB цвет; squaremap читает это поле напрямую. */
    public final int col;

    private final int id;

    public MapColor(int id, int col) {
        this.id = id;
        this.col = 0xFF000000 | (col & 0xFFFFFF);
    }

    public int id() {
        return this.id;
    }

    public int calculateRGBColor(Brightness brightness) {
        return this.col;
    }

    public enum Brightness {
        LOW,
        NORMAL,
        HIGH
    }
}
