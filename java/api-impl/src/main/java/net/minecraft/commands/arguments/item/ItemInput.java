package net.minecraft.commands.arguments.item;

/** NMS ItemInput: поля item/tag и createItemStack нужны Incendo Cloud. */
public class ItemInput {
    public Object item;
    public Object tag;

    public ItemInput() {
    }

    public net.minecraft.world.item.ItemStack createItemStack(int count, boolean allowOversized) {
        return new net.minecraft.world.item.ItemStack("minecraft:air", count);
    }
}
