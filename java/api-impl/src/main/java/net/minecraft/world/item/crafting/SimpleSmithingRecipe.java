package net.minecraft.world.item.crafting;

/** Автоматически сгенерированный NMS-стаб для net/minecraft/world/item/crafting/SimpleSmithingRecipe; не редактировать вручную. */
public abstract class SimpleSmithingRecipe implements net.minecraft.world.item.crafting.SmithingRecipe {
    public net.minecraft.world.item.crafting.Recipe$CommonInfo commonInfo;

    public abstract net.minecraft.world.item.crafting.RecipeSerializer getSerializer();
    public boolean matches(net.minecraft.world.item.crafting.SmithingRecipeInput input,
            net.minecraft.world.level.Level level) {
        return false;
    }
    public java.lang.Object placementInfo() {
        return null;
    }
    public java.lang.String group() {
        return null;
    }
    public boolean showNotification() {
        return false;
    }
}
