package net.minecraft.network.protocol;

/** NMS {@code BundlePacket} (CraftEngine). */
public abstract class BundlePacket implements Packet {
    private Iterable<?> packets;

    protected BundlePacket(Iterable<?> packets) {
        this.packets = packets;
    }

    public Iterable<?> getPackets() {
        return this.packets;
    }

    public void setPackets(Iterable<?> packets) {
        this.packets = packets;
    }
}
