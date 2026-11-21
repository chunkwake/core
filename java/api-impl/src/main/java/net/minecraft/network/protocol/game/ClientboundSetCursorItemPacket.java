package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;
import net.minecraft.world.item.ItemStack;

/** NMS {@code ClientboundSetCursorItemPacket} (CraftEngine). */
public class ClientboundSetCursorItemPacket implements Packet {
    private ItemStack contents = ItemStack.EMPTY;

    public ClientboundSetCursorItemPacket() {
    }

    public ClientboundSetCursorItemPacket(ItemStack contents) {
        this.contents = contents == null ? ItemStack.EMPTY : contents;
    }

    public ItemStack getContents() {
        return this.contents;
    }
}
