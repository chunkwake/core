package org.bukkit.craftbukkit.inventory;

/** CraftBukkit {@code CraftInventory} (CraftEngine). */
public class CraftInventory {
    private Object inventory;

    public CraftInventory() {
    }

    public static org.bukkit.inventory.Inventory newInstance(Object inventory) {
        return null;
    }

    public Object getInventory() {
        return this.inventory;
    }

    public void setInventory(Object inventory) {
        this.inventory = inventory;
    }
}
