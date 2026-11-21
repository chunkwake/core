package net.minecraft.world.item;

/** NMS {@code DyeColor}. */
public enum DyeColor {
    WHITE(0, "white"),
    ORANGE(1, "orange"),
    MAGENTA(2, "magenta"),
    LIGHT_BLUE(3, "light_blue"),
    YELLOW(4, "yellow"),
    LIME(5, "lime"),
    PINK(6, "pink"),
    GRAY(7, "gray"),
    LIGHT_GRAY(8, "light_gray"),
    CYAN(9, "cyan"),
    PURPLE(10, "purple"),
    BLUE(11, "blue"),
    BROWN(12, "brown"),
    GREEN(13, "green"),
    RED(14, "red"),
    BLACK(15, "black");

    /** Поля, которые ищет CraftEngine (textureDiffuseColor/fireworkColor). */
    private int textureDiffuseColor;
    private final int[] textureDiffuseColors = new int[3];
    private int fireworkColor;

    private final int id;
    private final String name;

    DyeColor(int id, String name) {
        this.id = id;
        this.name = name;
    }

    public int getId() {
        return this.id;
    }

    public String getName() {
        return this.name;
    }

    public static DyeColor byId(int id) {
        for (DyeColor color : values()) {
            if (color.id == id) {
                return color;
            }
        }
        return WHITE;
    }
}
