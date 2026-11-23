package org.bukkit.craftbukkit.inventory;

/** CraftBukkit {@code CraftInventoryView} (CraftEngine). */
public class CraftInventoryView {
    /** CraftEngine {@code FieldGetter(name="container")}. */
    public Object container;

    public CraftInventoryView() {
    }

    public net.minecraft.world.inventory.AbstractContainerMenu getContainer() {
        return this.container instanceof net.minecraft.world.inventory.AbstractContainerMenu menu ? menu : null;
    }
}
