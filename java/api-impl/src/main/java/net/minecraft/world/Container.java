package net.minecraft.world;

/** NMS {@code Container} (CraftEngine). */
public interface Container {
    default void setItem(int slot, net.minecraft.world.item.ItemStack item) {
    }

    default net.minecraft.world.item.ItemStack removeItem(int slot, int amount) {
        return net.minecraft.world.item.ItemStack.EMPTY;
    }

    default void setChanged() {
    }

    default net.minecraft.world.item.crafting.RecipeHolder getCurrentRecipe() {
        return null;
    }

    default net.minecraft.world.item.ItemStack getItem(int slot) {
        return net.minecraft.world.item.ItemStack.EMPTY;
    }

    int getContainerSize();

    boolean isEmpty();
}
