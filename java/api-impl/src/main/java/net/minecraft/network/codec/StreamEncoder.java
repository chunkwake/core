package net.minecraft.network.codec;

/** NMS {@code StreamEncoder}: PacketEvents/GrimAC ищут этот класс. */
public interface StreamEncoder<T> {
    void encode(Object output, T value);
}
