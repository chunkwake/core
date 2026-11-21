package net.minecraft.world.inventory;

/** NMS {@code InventoryMenu} (CraftEngine). */
public class InventoryMenu {
    public InventoryMenu() {
    }

    /** {@code InventoryMenu.getCraftSlots()} (CraftEngine). */
    public CraftingContainer getCraftSlots() {
        return new CraftingContainer();
    }
}
