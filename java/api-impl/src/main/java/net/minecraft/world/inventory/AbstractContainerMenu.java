package net.minecraft.world.inventory;

/** Минимальный NMS {@code AbstractContainerMenu} (InvSee++). */
public class AbstractContainerMenu {
    /** Поле, которое ищет CraftEngine (getCarried). */
    private net.minecraft.world.item.ItemStack carried = net.minecraft.world.item.ItemStack.EMPTY;

    /** Поле, которое ищет CraftEngine (getContainerId). */
    private int containerId;

    /** Поле, которое ищет CraftEngine (getSlots). */
    public final net.minecraft.core.NonNullList<Slot> slots = new net.minecraft.core.NonNullList<>();

    /** Поле, которое ищет CraftEngine (getCheckReachable). */
    private boolean checkReachable = true;

    /** {@code AbstractContainerMenu.getDataSlots()} (CraftEngine). */
    public java.util.List<DataSlot> getDataSlots() {
        return java.util.List.of();
    }

    /** {@code AbstractContainerMenu.getMenuType()} (CraftEngine). */
    public net.minecraft.world.inventory.MenuType<?> getMenuType() {
        return null;
    }

    /** {@code AbstractContainerMenu.getCarried()} (CraftEngine). */
    public net.minecraft.world.item.ItemStack getCarried() {
        return this.carried;
    }

    /** {@code AbstractContainerMenu.broadcastChanges()} (CraftEngine). */
    public void broadcastChanges() {
    }

    /** {@code AbstractContainerMenu.broadcastFullState()} (CraftEngine). */
    public void broadcastFullState() {
    }

    /** {@code AbstractContainerMenu.quickMoveStack(Player, int)} (CraftEngine). */
    public net.minecraft.world.item.ItemStack quickMoveStack(
            net.minecraft.world.entity.player.Player player, int index) {
        return net.minecraft.world.item.ItemStack.EMPTY;
    }

    /** {@code AbstractContainerMenu.incrementStateId()} (CraftEngine). */
    public int incrementStateId() {
        return 0;
    }

    /** {@code AbstractContainerMenu.getSlot(int)} (CraftEngine). */
    public Slot getSlot(int index) {
        return new Slot();
    }

}
