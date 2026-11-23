package org.bukkit.craftbukkit.inventory;

/** CraftBukkit {@code CraftInventoryAnvil} (CraftEngine). */
public class CraftInventoryAnvil {
    /** CraftEngine {@code FieldGetter(name="container")}. */
    public Object container;

    public CraftInventoryAnvil() {
    }

    public net.minecraft.world.inventory.AbstractContainerMenu getContainer() {
        return this.container instanceof net.minecraft.world.inventory.AbstractContainerMenu menu ? menu : null;
    }
}
