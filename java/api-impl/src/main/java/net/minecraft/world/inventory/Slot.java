package net.minecraft.world.inventory;

/** NMS {@code Slot} (CraftEngine). */
public class Slot {
    /** Поле, которое ищет CraftEngine (SlotProxy#getContainer). */
    private net.minecraft.world.Container container;

    public Slot() {
    }

    public net.minecraft.world.Container getContainer() {
        return this.container;
    }

    /** {@code Slot.getItem()} (CraftEngine). */
    public net.minecraft.world.item.ItemStack getItem() {
        return net.minecraft.world.item.ItemStack.EMPTY;
    }
}
