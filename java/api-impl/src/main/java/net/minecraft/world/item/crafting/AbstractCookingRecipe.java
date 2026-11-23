package net.minecraft.world.item.crafting;

/** Автоматически сгенерированный NMS-стаб для net/minecraft/world/item/crafting/AbstractCookingRecipe; не редактировать вручную. */
public abstract class AbstractCookingRecipe extends net.minecraft.world.item.crafting.SingleItemRecipe {
    public net.minecraft.world.item.crafting.AbstractCookingRecipe$CookingBookInfo bookInfo;

    public AbstractCookingRecipe() {
    }
    public AbstractCookingRecipe(net.minecraft.world.item.crafting.Recipe$CommonInfo p0, net.minecraft.world.item.crafting.AbstractCookingRecipe$CookingBookInfo p1, net.minecraft.world.item.crafting.Ingredient p2, net.minecraft.world.item.ItemStackTemplate p3, float p4, int p5) {
    }
    public abstract net.minecraft.world.item.crafting.RecipeSerializer getSerializer();
    public abstract net.minecraft.world.item.crafting.RecipeType getType();
    public float experience() {
        return 0;
    }
    public int cookingTime() {
        return 0;
    }
    public net.minecraft.world.item.crafting.CookingBookCategory category() {
        return null;
    }
    public java.lang.String group() {
        return null;
    }
    public java.util.List display() {
        return null;
    }
    public static com.mojang.serialization.MapCodec cookingMapCodec(java.lang.Object p0) {
        return null;
    }
    public static net.minecraft.network.codec.StreamCodec cookingStreamCodec(java.lang.Object p0) {
        return null;
    }
}
