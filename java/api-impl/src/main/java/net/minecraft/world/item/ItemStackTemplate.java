package net.minecraft.world.item;

/** NMS-заглушка {@code ItemStackTemplate} (CommandAPI проверяет наличие). */
public class ItemStackTemplate {
    private final net.minecraft.core.Holder<Item> item;
    private final int count;
    private final net.minecraft.core.component.DataComponentPatch components;

    public ItemStackTemplate() {
        this(null, 1, null);
    }

    public ItemStackTemplate(net.minecraft.core.Holder<Item> item, int count,
            net.minecraft.core.component.DataComponentPatch components) {
        this.item = item;
        this.count = count;
        this.components = components;
    }

    public net.minecraft.core.Holder<Item> item() {
        return this.item;
    }

    public int count() {
        return this.count;
    }

    public net.minecraft.core.component.DataComponentPatch components() {
        return this.components;
    }

    public static final Object STREAM_CODEC = null;

    public ItemStack create() {
        return ItemStack.EMPTY;
    }

    public static ItemStackTemplate fromNonEmptyStack(ItemStack stack) {
        return new ItemStackTemplate();
    }
}
