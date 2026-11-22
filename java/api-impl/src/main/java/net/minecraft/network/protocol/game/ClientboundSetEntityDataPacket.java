package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundSetEntityDataPacket} (id + opaque metadata). */
public class ClientboundSetEntityDataPacket implements Packet {
    private int id;
    private byte[] packedItems = new byte[0];
    private java.util.List<net.minecraft.network.syncher.SynchedEntityData.DataValue<?>> values =
            java.util.List.of();

    public ClientboundSetEntityDataPacket() {
    }

    public ClientboundSetEntityDataPacket(int id,
            java.util.List<net.minecraft.network.syncher.SynchedEntityData.DataValue<?>> values) {
        this.id = id;
        this.values = values == null ? java.util.List.of() : values;
    }

    public java.util.List<net.minecraft.network.syncher.SynchedEntityData.DataValue<?>> values() {
        return this.values;
    }

    public ClientboundSetEntityDataPacket(int id, byte[] packedItems) {
        this.id = id;
        this.packedItems = packedItems == null ? new byte[0] : packedItems;
    }

    public int id() {
        return this.id;
    }

    public byte[] packedItems() {
        return this.packedItems;
    }

    public static ClientboundSetEntityDataPacket newInstance(int id, java.util.List<?> values) {
        java.util.List<net.minecraft.network.syncher.SynchedEntityData.DataValue<?>> list =
                new java.util.ArrayList<>();
        for (Object value : values) {
            if (value instanceof net.minecraft.network.syncher.SynchedEntityData.DataValue<?> data) {
                list.add(data);
            }
        }
        return new ClientboundSetEntityDataPacket(id, list);
    }

    public int getId() {
        return this.id;
    }

    public java.util.List<?> getPackedItems() {
        return this.values;
    }

    public static void pack(java.util.List<?> values,
            net.minecraft.network.FriendlyByteBuf buffer) {
    }

    public static void pack(java.util.List<?> values,
            net.minecraft.network.RegistryFriendlyByteBuf buffer) {
    }

    public static java.util.List<?> unpack(net.minecraft.network.FriendlyByteBuf buffer) {
        return java.util.List.of();
    }

    public static java.util.List<?> unpack(net.minecraft.network.RegistryFriendlyByteBuf buffer) {
        return java.util.List.of();
    }
}
