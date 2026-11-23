package net.minecraft.world.item.crafting;

/** NMS {@code Ingredient}: набор входных предметов. */
public class Ingredient {
    public Ingredient() {
    }

    /** {@code Ingredient.ofStacks(List)} (CraftEngine). */
    public static Ingredient ofStacks(java.util.List<net.minecraft.world.item.ItemStack> stacks) {
        return new Ingredient();
    }

    /** {@code Ingredient.of(Stream)} (CraftEngine). */
    public static Ingredient of(java.util.stream.Stream<net.minecraft.world.item.ItemStack> stacks) {
        return new Ingredient();
    }
}
