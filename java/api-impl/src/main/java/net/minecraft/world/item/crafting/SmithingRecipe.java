package net.minecraft.world.item.crafting;

/** Автоматически сгенерированный NMS-стаб для net/minecraft/world/item/crafting/SmithingRecipe; не редактировать вручную. */
public interface SmithingRecipe extends net.minecraft.world.item.crafting.Recipe {

    public default net.minecraft.world.item.crafting.RecipeType getType() {
        return null;
    }
    public default net.minecraft.world.item.crafting.RecipeSerializer getSerializer() {
        return null;
    }
    public default boolean matches(net.minecraft.world.item.crafting.SmithingRecipeInput p0, net.minecraft.world.level.Level p1) {
        return false;
    }
    public default java.util.Optional templateIngredient() {
        return null;
    }
    public default net.minecraft.world.item.crafting.Ingredient baseIngredient() {
        return null;
    }
    public default java.util.Optional additionIngredient() {
        return null;
    }
    public default java.lang.Object recipeBookCategory() {
        return null;
    }
    public default boolean matches(net.minecraft.world.item.crafting.RecipeInput p0, net.minecraft.world.level.Level p1) {
        return false;
    }
}
