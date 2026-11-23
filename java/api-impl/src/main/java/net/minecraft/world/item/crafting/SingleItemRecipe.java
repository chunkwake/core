package net.minecraft.world.item.crafting;

/** Автоматически сгенерированный NMS-стаб для net/minecraft/world/item/crafting/SingleItemRecipe; не редактировать вручную. */
public abstract class SingleItemRecipe implements net.minecraft.world.item.crafting.Recipe {
    public net.minecraft.world.item.crafting.Recipe$CommonInfo commonInfo;

    public SingleItemRecipe() {
    }
    public SingleItemRecipe(net.minecraft.world.item.crafting.Recipe$CommonInfo p0, net.minecraft.world.item.crafting.Ingredient p1, net.minecraft.world.item.ItemStackTemplate p2) {
    }
    public abstract net.minecraft.world.item.crafting.RecipeSerializer getSerializer();
    public abstract net.minecraft.world.item.crafting.RecipeType getType();
    public boolean matches(net.minecraft.world.item.crafting.SingleRecipeInput p0, net.minecraft.world.level.Level p1) {
        return false;
    }
    public boolean showNotification() {
        return false;
    }
    public net.minecraft.world.item.crafting.Ingredient input() {
        return null;
    }
    public java.lang.Object placementInfo() {
        return null;
    }
    public net.minecraft.world.item.ItemStack assemble(net.minecraft.world.item.crafting.SingleRecipeInput p0) {
        return null;
    }
    public static com.mojang.serialization.MapCodec simpleMapCodec(java.lang.Object p0) {
        return null;
    }
    public static net.minecraft.network.codec.StreamCodec simpleStreamCodec(java.lang.Object p0) {
        return null;
    }
    public net.minecraft.world.item.ItemStack assemble(net.minecraft.world.item.crafting.RecipeInput p0) {
        return null;
    }
    public boolean matches(net.minecraft.world.item.crafting.RecipeInput p0, net.minecraft.world.level.Level p1) {
        return false;
    }
}
