package net.minecraft.network;

/** NMS {@code ProtocolInfo}. */
public interface ProtocolInfo<T extends PacketListener> {
    ConnectionProtocol id();

    net.minecraft.network.protocol.PacketFlow flow();

    net.minecraft.network.codec.StreamCodec<?> codec();

    Object bundlerInfo();

    DetailsProvider detailsProvider();

    /** NMS {@code ProtocolInfo.DetailsProvider}. */
    interface DetailsProvider {
        Details details();
    }

    /** NMS {@code ProtocolInfo.Details}. */
    interface Details {
        ConnectionProtocol id();

        net.minecraft.network.protocol.PacketFlow flow();

        void listPackets(PacketVisitor visitor);

        /** NMS {@code ProtocolInfo.Details.PacketVisitor}. */
        interface PacketVisitor {
            void visit(Object packetType, int id);
        }
    }
}
