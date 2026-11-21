package net.minecraft.world.item.crafting;

/** NMS-заглушка {@code RecipeHolder} (CommandAPI проверяет наличие). */
public class RecipeHolder {
    /** Поле, которое ищет CraftEngine (getValue). */
    private Object value;
    /** Поле, которое ищет CraftEngine (getId). */
    private Object id;

    public RecipeHolder() {
    }
}
