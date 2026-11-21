package net.minecraft.world.inventory;

/** NMS {@code InventoryMenu} (CraftEngine). */
public class InventoryMenu {
    public InventoryMenu() {
    }

    /** {@code InventoryMenu.slotsChanged(Container)} (CraftEngine). */
    public void slotsChanged(net.minecraft.world.Container container) {
    }

    /** {@code InventoryMenu.getCraftSlots()} (CraftEngine). */
    public CraftingContainer getCraftSlots() {
        return new CraftingContainer();
    }
}
