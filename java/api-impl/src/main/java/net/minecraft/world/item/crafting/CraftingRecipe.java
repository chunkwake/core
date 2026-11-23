package net.minecraft.world.item.crafting;

/** Автоматически сгенерированный NMS-стаб для net/minecraft/world/item/crafting/CraftingRecipe; не редактировать вручную. */
public interface CraftingRecipe extends net.minecraft.world.item.crafting.Recipe {

    public default net.minecraft.world.item.crafting.RecipeType getType() {
        return null;
    }
    public default net.minecraft.world.item.crafting.RecipeSerializer getSerializer() {
        return null;
    }
    public default net.minecraft.world.item.crafting.CraftingBookCategory category() {
        return null;
    }
    public default net.minecraft.core.NonNullList getRemainingItems(net.minecraft.world.item.crafting.CraftingInput p0) {
        return null;
    }
    public static net.minecraft.core.NonNullList defaultCraftingReminder(net.minecraft.world.item.crafting.CraftingInput p0) {
        return null;
    }
    public default java.lang.Object recipeBookCategory() {
        return null;
    }
}
