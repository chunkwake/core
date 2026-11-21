package net.minecraft.world.item.crafting;

/** NMS {@code Recipe} (CraftEngine). */
public interface Recipe {
    /** {@code Recipe.getId()} (CraftEngine). */
    net.minecraft.resources.Identifier getId();

    /** {@code Recipe.getType()} (CraftEngine). */
    RecipeType<?> getType();
}
