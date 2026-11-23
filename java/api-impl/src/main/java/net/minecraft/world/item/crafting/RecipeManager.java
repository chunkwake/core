package net.minecraft.world.item.crafting;

/** NMS-заглушка {@code RecipeManager} (CommandAPI проверяет наличие). */
public class RecipeManager {
    /** Поля, которые CraftEngine читает через FieldGetter. */
    private final net.minecraft.world.flag.FeatureFlagSet enabledFlags = null;
    private final java.util.Map<Object, Object> byType = new java.util.HashMap<>();
    /** Поле, которое ищет CraftEngine (getByName). */
    private final java.util.Map<Object, Object> byName = new java.util.HashMap<>();
    /** Paper 26.3 хранит рецепты в RecipeMap; CE читает это поле. */
    private final RecipeMap recipes = new RecipeMap();

    public RecipeManager() {
    }

    public net.minecraft.world.flag.FeatureFlagSet getEnabledFlags() {
        return null;
    }

    /** {@code RecipeManager.finalizeRecipeLoading(FeatureFlagSet)} (CraftEngine). */
    public void finalizeRecipeLoading(net.minecraft.world.flag.FeatureFlagSet flags) {
    }

    /** {@code RecipeManager.addRecipe(RecipeHolder)} (CraftEngine). */
    public void addRecipe(RecipeHolder recipe) {
    }

    /** {@code RecipeManager.removeRecipe(ResourceKey)} (CraftEngine). */
    public boolean removeRecipe(net.minecraft.resources.ResourceKey<?> key) {
        return false;
    }

    /** {@code RecipeManager.getRecipeFor(RecipeType, RecipeInput, Level, ResourceKey)} (CraftEngine). */
    public java.util.Optional<RecipeHolder> getRecipeFor(RecipeType<?> type, RecipeInput input,
            net.minecraft.world.level.Level level, net.minecraft.resources.ResourceKey<?> key) {
        return java.util.Optional.empty();
    }
}
