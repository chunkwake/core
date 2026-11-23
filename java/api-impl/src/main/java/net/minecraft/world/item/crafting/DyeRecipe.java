package net.minecraft.world.item.crafting;

/** NMS {@code DyeRecipe} (CraftEngine). */
public class DyeRecipe extends NormalCraftingRecipe {
    public static final com.mojang.serialization.MapCodec MAP_CODEC = null;
    public static final net.minecraft.network.codec.StreamCodec STREAM_CODEC = null;
    public static final RecipeSerializer SERIALIZER = null;

    private final Ingredient target;
    private final Ingredient dye;
    private final net.minecraft.world.item.ItemStackTemplate result;

    public DyeRecipe(Recipe$CommonInfo commonInfo, CraftingRecipe$CraftingBookInfo bookInfo,
            Ingredient target, Ingredient dye, net.minecraft.world.item.ItemStackTemplate result) {
        super(commonInfo, bookInfo);
        this.target = target;
        this.dye = dye;
        this.result = result;
    }

    @Override
    public RecipeSerializer getSerializer() {
        return SERIALIZER;
    }

    public boolean matches(CraftingInput input, net.minecraft.world.level.Level level) {
        return false;
    }

    public net.minecraft.world.item.ItemStack assemble(CraftingInput input) {
        return null;
    }
}
