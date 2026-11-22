package net.minecraft.world.item.crafting;

/** NMS {@code RecipeInput}: вход рецепта. */
public interface RecipeInput {
    net.minecraft.world.item.ItemStack getItem(int index);

    int size();

    default boolean isEmpty() {
        return size() == 0;
    }
}
