package net.minecraft.world.item.crafting;

/** NMS {@code CustomRecipe}: рецепт без данных. */
public abstract class CustomRecipe implements Recipe {
    @Override
    public net.minecraft.resources.Identifier getId() {
        return null;
    }

    @Override
    public RecipeType<?> getType() {
        return null;
    }
}
