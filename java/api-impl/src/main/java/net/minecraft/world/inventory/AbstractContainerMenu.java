package net.minecraft.world.inventory;

/** Минимальный NMS {@code AbstractContainerMenu} (InvSee++). */
public class AbstractContainerMenu {
    /** {@code AbstractContainerMenu.incrementStateId()} (CraftEngine). */
    public int incrementStateId() {
        return 0;
    }

    /** {@code AbstractContainerMenu.getSlot(int)} (CraftEngine). */
    public Slot getSlot(int index) {
        return new Slot();
    }

}
