package net.minecraft.world.item.crafting;

/** NMS {@code FireworkStarFadeRecipe}: минимальный стаб для CraftEngine. */
public class FireworkStarFadeRecipe extends CustomRecipe {
    public static final FireworkStarFadeRecipe INSTANCE =
            new FireworkStarFadeRecipe(null, null, null);

    private final Ingredient target;
    private final Ingredient dye;
    private final net.minecraft.world.item.ItemStackTemplate result;

    public FireworkStarFadeRecipe(Ingredient target, Ingredient dye,
            net.minecraft.world.item.ItemStackTemplate result) {
        this.target = target;
        this.dye = dye;
        this.result = result;
    }
}
