package net.minecraft.world.item.crafting;

/** NMS {@code PotionIngredient} (CraftEngine brewing). */
public final class PotionIngredient implements java.util.function.Predicate<net.minecraft.world.item.ItemStack> {
    public static final com.mojang.serialization.MapCodec MAP_CODEC = null;
    public static final net.minecraft.network.codec.StreamCodec STREAM_CODEC = null;

    private final Ingredient ingredient;
    private final java.util.Optional<?> potions;

    public PotionIngredient(Ingredient ingredient, java.util.Optional<?> potions) {
        this.ingredient = ingredient;
        this.potions = potions;
    }

    public Ingredient ingredient() {
        return this.ingredient;
    }

    public java.util.Optional<?> potions() {
        return this.potions;
    }

    @Override
    public boolean test(net.minecraft.world.item.ItemStack stack) {
        return false;
    }
}
