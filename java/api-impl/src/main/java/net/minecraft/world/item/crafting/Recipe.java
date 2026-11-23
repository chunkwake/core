package net.minecraft.world.item.crafting;

/** NMS {@code Recipe} (CraftEngine). */
public interface Recipe {
    /** {@code Recipe.getId()} (CraftEngine). */
    default net.minecraft.resources.Identifier getId() {
        return null;
    }

    /** {@code Recipe.getType()} (CraftEngine). */
    default RecipeType<?> getType() {
        return null;
    }
}
