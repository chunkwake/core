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

    /** {@code Slot.safeTake(int, int, Player)} (CraftEngine). */
    public net.minecraft.world.item.ItemStack safeTake(int amount, int maxAmount,
            net.minecraft.world.entity.player.Player player) {
        return net.minecraft.world.item.ItemStack.EMPTY;
    }

    /** {@code Slot.getItem()} (CraftEngine). */
    public net.minecraft.world.item.ItemStack getItem() {
        return net.minecraft.world.item.ItemStack.EMPTY;
    }
}
