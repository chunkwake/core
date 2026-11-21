package net.minecraft.network.protocol.game;

import java.util.List;
import net.minecraft.network.protocol.Packet;
import net.minecraft.world.item.ItemStack;

/** NMS {@code ClientboundContainerSetContentPacket} (CraftEngine). */
public class ClientboundContainerSetContentPacket implements Packet {
    private int containerId;
    private int stateId;
    private List<ItemStack> items = List.of();
    private ItemStack carriedItem = ItemStack.EMPTY;

    public ClientboundContainerSetContentPacket() {
    }

    public ClientboundContainerSetContentPacket(int containerId, int stateId,
            List<ItemStack> items, ItemStack carriedItem) {
        this.containerId = containerId;
        this.stateId = stateId;
        this.items = items == null ? List.of() : items;
        this.carriedItem = carriedItem == null ? ItemStack.EMPTY : carriedItem;
    }

    public ClientboundContainerSetContentPacket(int containerId, int stateId,
            ItemStack carriedItem, ItemStack ignored) {
        this(containerId, stateId, List.of(), carriedItem);
    }

    public int getContainerId() {
        return this.containerId;
    }

    public int getStateId() {
        return this.stateId;
    }

    public List<ItemStack> getItems() {
        return this.items;
    }

    public ItemStack getCarriedItem() {
        return this.carriedItem;
    }
}
