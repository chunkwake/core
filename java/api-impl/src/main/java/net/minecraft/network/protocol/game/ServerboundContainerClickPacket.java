package net.minecraft.network.protocol.game;

import it.unimi.dsi.fastutil.ints.Int2ObjectMap;
import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import net.minecraft.network.protocol.Packet;
import net.minecraft.world.item.ItemStack;

/** NMS {@code ServerboundContainerClickPacket} (CraftEngine). */
public class ServerboundContainerClickPacket implements Packet {
    private ItemStack carriedItem = ItemStack.EMPTY;
    private Int2ObjectMap<ItemStack> changedSlots = new Int2ObjectOpenHashMap<>();

    public ServerboundContainerClickPacket() {
    }

    public ItemStack getCarriedItem() {
        return this.carriedItem;
    }

    public void setCarriedItem(ItemStack carriedItem) {
        this.carriedItem = carriedItem == null ? ItemStack.EMPTY : carriedItem;
    }

    public Int2ObjectMap<ItemStack> getChangedSlots() {
        return this.changedSlots;
    }

    public void setChangedSlots(Int2ObjectMap<?> changedSlots) {
        Int2ObjectMap<ItemStack> copy = new Int2ObjectOpenHashMap<>();
        if (changedSlots != null) {
            for (Int2ObjectMap.Entry<?> entry : changedSlots.int2ObjectEntrySet()) {
                copy.put(entry.getIntKey(), (ItemStack) entry.getValue());
            }
        }
        this.changedSlots = copy;
    }
}
