package net.minecraft.world.item.crafting;

import net.minecraft.world.item.ItemStack;

/** NMS {@code RepairItemRecipe}. */
public class RepairItemRecipe extends CustomRecipe {
    public static final RepairItemRecipe INSTANCE = new RepairItemRecipe();
    public static final com.mojang.serialization.MapCodec<RepairItemRecipe> MAP_CODEC = null;
    public static final Object STREAM_CODEC = null;
    public static final RecipeSerializer<RepairItemRecipe> SERIALIZER = null;

    public RepairItemRecipe() {
    }

    public boolean matches(CraftingInput input, net.minecraft.world.level.Level level) {
        return false;
    }

    public ItemStack assemble(CraftingInput input) {
        return ItemStack.EMPTY;
    }

    public RecipeSerializer<RepairItemRecipe> getSerializer() {
        return SERIALIZER;
    }
}
