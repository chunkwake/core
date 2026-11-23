package net.minecraft.world.item.crafting;

/** NMS-заглушка {@code RecipeHolder} (CommandAPI/CraftEngine). */
public class RecipeHolder {
    /** Поле, которое ищет CraftEngine (getValue). */
    private Object value;
    /** Поле, которое ищет CraftEngine (getId). */
    private Object id;

    public RecipeHolder() {
    }

    public RecipeHolder(net.minecraft.resources.ResourceKey<?> id, Recipe recipe) {
        this.id = id;
        this.value = recipe;
    }
}
