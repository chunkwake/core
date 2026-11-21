package net.minecraft.world.inventory;

/** NMS {@code CraftingContainer} (CraftEngine). */
public class CraftingContainer {
    public CraftingContainer() {
    }

    public void setItem(int slot, net.minecraft.world.item.ItemStack item) {
    }

    public net.minecraft.world.item.ItemStack removeItem(int slot, int amount) {
        return net.minecraft.world.item.ItemStack.EMPTY;
    }

    public void setChanged() {
    }

    public net.minecraft.world.item.crafting.RecipeHolder getCurrentRecipe() {
        return null;
    }
}
