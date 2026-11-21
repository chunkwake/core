package net.minecraft.network.protocol.game;

import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundSetPassengersPacket}. */
public class ClientboundSetPassengersPacket implements Packet {
    /** Нужен DecentHolograms: кодек декодирует байты из FriendlyByteBuf. */
    public static final StreamCodec<ClientboundSetPassengersPacket> STREAM_CODEC =
            new StreamCodec<ClientboundSetPassengersPacket>() {
                @Override
                public ClientboundSetPassengersPacket decode(Object buffer) {
                    FriendlyByteBuf buf = (FriendlyByteBuf) buffer;
                    int vehicle = buf.readVarInt();
                    int count = buf.readVarInt();
                    int[] passengers = new int[count];
                    for (int i = 0; i < count; i++) {
                        passengers[i] = buf.readVarInt();
                    }
                    return new ClientboundSetPassengersPacket(vehicle, passengers);
                }
            };

    private int vehicle;
    private int[] passengers;

    public ClientboundSetPassengersPacket(net.minecraft.world.entity.Entity vehicle) {
        this(vehicle == null ? 0 : vehicle.getId(), new int[0]);
    }

    public ClientboundSetPassengersPacket(int vehicle, int[] passengers) {
        this.vehicle = vehicle;
        this.passengers = passengers == null ? new int[0] : passengers.clone();
    }

    public int getVehicle() {
        return this.vehicle;
    }

    public int[] getPassengers() {
        return this.passengers;
    }

    public void setVehicle(int vehicle) {
        this.vehicle = vehicle;
    }

    public void setPassengers(int[] passengers) {
        this.passengers = passengers == null ? new int[0] : passengers.clone();
    }
}
