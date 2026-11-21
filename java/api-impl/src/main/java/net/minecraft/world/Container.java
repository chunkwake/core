package net.minecraft.world;

/** NMS {@code Container} (CraftEngine). */
public interface Container {
    default net.minecraft.world.item.crafting.RecipeHolder getCurrentRecipe() {
        return null;
    }

    default net.minecraft.world.item.ItemStack getItem(int slot) {
        return net.minecraft.world.item.ItemStack.EMPTY;
    }

    int getContainerSize();

    boolean isEmpty();
}
