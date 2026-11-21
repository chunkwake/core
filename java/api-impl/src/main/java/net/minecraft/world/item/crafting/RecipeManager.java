package net.minecraft.world.item.crafting;

/** NMS-заглушка {@code RecipeManager} (CommandAPI проверяет наличие). */
public class RecipeManager {
    /** Поле, которое ищет CraftEngine (getByName). */
    private final java.util.Map<Object, Object> byName = new java.util.HashMap<>();
    /** Поле, которое ищет CraftEngine (getRecipes). */
    private final java.util.Collection<Object> recipes = new java.util.ArrayList<>();

    public RecipeManager() {
    }
}
