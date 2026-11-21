package net.minecraft.world.inventory;

/** Минимальный NMS {@code AbstractContainerMenu} (InvSee++). */
public class AbstractContainerMenu {
    /** {@code AbstractContainerMenu.getSlot(int)} (CraftEngine). */
    public Slot getSlot(int index) {
        return new Slot();
    }

}
