package net.minecraft.network.protocol.game;

import com.mojang.datafixers.util.Pair;
import java.util.List;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.item.ItemStack;

/** NMS ClientboundSetEquipmentPacket: entity + список (slot, stack). */
public class ClientboundSetEquipmentPacket {
    private int entity;
    private List<Pair<EquipmentSlot, ItemStack>> slots;

    public ClientboundSetEquipmentPacket(int entity, List<Pair<EquipmentSlot, ItemStack>> slots) {
        this.entity = entity;
        this.slots = slots;
    }

    public int getEntity() {
        return this.entity;
    }

    public List<Pair<EquipmentSlot, ItemStack>> getSlots() {
        return this.slots;
    }
}
