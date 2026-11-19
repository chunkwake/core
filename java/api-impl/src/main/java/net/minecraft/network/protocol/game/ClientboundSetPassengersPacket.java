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

    private final int vehicle;
    private final int[] passengers;

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
}
