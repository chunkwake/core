package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;
import net.minecraft.world.item.ItemStack;

/** NMS {@code ServerboundSetCreativeModeSlotPacket} (CraftEngine). */
public class ServerboundSetCreativeModeSlotPacket implements Packet {
    private short slotNum;
    private ItemStack itemStack = ItemStack.EMPTY;

    public ServerboundSetCreativeModeSlotPacket() {
    }

    public ServerboundSetCreativeModeSlotPacket(short slotNum, ItemStack itemStack) {
        this.slotNum = slotNum;
        this.itemStack = itemStack == null ? ItemStack.EMPTY : itemStack;
    }

    public ServerboundSetCreativeModeSlotPacket(int slotNum, ItemStack itemStack) {
        this((short) slotNum, itemStack);
    }

    public short getSlotNum() { return this.slotNum; }
    public int getSlotNum$legacy() { return this.slotNum; }
    public ItemStack getItemStack() { return this.itemStack; }
}
