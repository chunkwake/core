package net.minecraft.world.item.crafting;

/** NMS {@code BrewingInput} (CraftEngine). */
public final class BrewingInput implements RecipeInput {
    private final net.minecraft.world.item.ItemStack input;
    private final net.minecraft.world.item.ItemStack reagent;

    public BrewingInput(net.minecraft.world.item.ItemStack input, net.minecraft.world.item.ItemStack reagent) {
        this.input = input;
        this.reagent = reagent;
    }

    public net.minecraft.world.item.ItemStack input() {
        return this.input;
    }

    public net.minecraft.world.item.ItemStack reagent() {
        return this.reagent;
    }

    @Override
    public net.minecraft.world.item.ItemStack getItem(int index) {
        return index == 0 ? this.input : this.reagent;
    }

    @Override
    public int size() {
        return 2;
    }
}
