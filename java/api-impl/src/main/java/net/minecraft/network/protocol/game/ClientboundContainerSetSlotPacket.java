package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;
import net.minecraft.world.item.ItemStack;

/** NMS {@code ClientboundContainerSetSlotPacket} (CraftEngine). */
public class ClientboundContainerSetSlotPacket implements Packet {
    private int containerId;
    private int stateId;
    private int slot;
    private ItemStack itemStack = ItemStack.EMPTY;

    public ClientboundContainerSetSlotPacket() {
    }

    public ClientboundContainerSetSlotPacket(int containerId, int stateId, int slot,
            ItemStack itemStack) {
        this.containerId = containerId;
        this.stateId = stateId;
        this.slot = slot;
        this.itemStack = itemStack == null ? ItemStack.EMPTY : itemStack;
    }

    public int getContainerId() {
        return this.containerId;
    }

    public int getStateId() {
        return this.stateId;
    }

    public int getSlot() {
        return this.slot;
    }

    public ItemStack getItemStack() {
        return this.itemStack;
    }
}
