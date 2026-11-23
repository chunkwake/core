package net.minecraft.world.item.crafting;

/** NMS {@code BrewingRecipe} (CraftEngine). */
public class BrewingRecipe implements Recipe {
    public static final com.mojang.serialization.MapCodec MAP_CODEC = null;
    public static final net.minecraft.network.codec.StreamCodec STREAM_CODEC = null;
    public static final RecipeSerializer SERIALIZER = null;

    private final PotionIngredient input;
    private final PotionIngredient reagent;
    private final net.minecraft.world.item.ItemStackTemplate output;

    public BrewingRecipe(PotionIngredient input, PotionIngredient reagent,
            net.minecraft.world.item.ItemStackTemplate output) {
        this.input = input;
        this.reagent = reagent;
        this.output = output;
    }

    public PotionIngredient getInput() {
        return this.input;
    }

    public PotionIngredient getReagent() {
        return this.reagent;
    }

    public net.minecraft.world.item.ItemStackTemplate getOutput() {
        return this.output;
    }

    public boolean matches(BrewingInput input) {
        return false;
    }

    public boolean matches(BrewingInput input, net.minecraft.world.level.Level level) {
        return false;
    }

    public net.minecraft.world.item.ItemStack assemble(BrewingInput input) {
        return null;
    }

    public RecipeType getType() {
        return null;
    }

    public RecipeSerializer getSerializer() {
        return SERIALIZER;
    }
}
