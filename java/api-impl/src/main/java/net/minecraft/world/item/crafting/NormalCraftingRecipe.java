package net.minecraft.world.item.crafting;

/** Автоматически сгенерированный NMS-стаб для net/minecraft/world/item/crafting/NormalCraftingRecipe; не редактировать вручную. */
public abstract class NormalCraftingRecipe implements net.minecraft.world.item.crafting.CraftingRecipe {
    public net.minecraft.world.item.crafting.Recipe$CommonInfo commonInfo;
    public net.minecraft.world.item.crafting.CraftingRecipe$CraftingBookInfo bookInfo;

    protected NormalCraftingRecipe() {
        this(null, null);
    }

    protected NormalCraftingRecipe(net.minecraft.world.item.crafting.Recipe$CommonInfo commonInfo,
            net.minecraft.world.item.crafting.CraftingRecipe$CraftingBookInfo bookInfo) {
        this.commonInfo = commonInfo;
        this.bookInfo = bookInfo;
    }

    public abstract net.minecraft.world.item.crafting.RecipeSerializer getSerializer();
    public java.lang.String group() {
        return null;
    }
    public net.minecraft.world.item.crafting.CraftingBookCategory category() {
        return null;
    }
    public boolean showNotification() {
        return false;
    }
    public java.lang.Object placementInfo() {
        return null;
    }
}
