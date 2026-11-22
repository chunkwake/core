package net.minecraft.world.item.crafting;

import java.util.List;
import net.minecraft.world.item.ItemStack;

/** NMS {@code CraftingInput}: сетка крафта. */
public class CraftingInput implements RecipeInput {
    public static final CraftingInput EMPTY = new CraftingInput(0, 0, List.of());

    private final int width;
    private final int height;
    private final List<ItemStack> items;
    private final Object stackedContents;

    private CraftingInput(int width, int height, List<ItemStack> items) {
        this.width = width;
        this.height = height;
        this.items = items == null ? List.of() : items;
        this.stackedContents = null;
    }

    public static CraftingInput of(int width, int height, List<ItemStack> items) {
        return new CraftingInput(width, height, items);
    }

    public static CraftingInput ofPositioned(int width, int height, List<ItemStack> items) {
        return new CraftingInput(width, height, items);
    }

    @Override
    public ItemStack getItem(int index) {
        return index >= 0 && index < this.items.size() ? this.items.get(index)
                : ItemStack.EMPTY;
    }

    public ItemStack getItem(int x, int y) {
        return getItem(y * this.width + x);
    }

    @Override
    public int size() {
        return this.items.size();
    }

    @Override
    public boolean isEmpty() {
        return this.items.isEmpty();
    }

    public Object stackedContents() {
        return this.stackedContents;
    }

    public List<ItemStack> items() {
        return this.items;
    }

    public int ingredientCount() {
        return (int) this.items.stream().filter(stack -> !stack.isEmpty()).count();
    }

    public int width() {
        return this.width;
    }

    public int height() {
        return this.height;
    }

    /** NMS {@code CraftingInput.Positioned}. */
    public static class Positioned {
        private final CraftingInput input;
        private final int left;
        private final int top;

        public Positioned(CraftingInput input, int left, int top) {
            this.input = input;
            this.left = left;
            this.top = top;
        }

        public CraftingInput input() {
            return this.input;
        }

        public int left() {
            return this.left;
        }

        public int top() {
            return this.top;
        }
    }
}
