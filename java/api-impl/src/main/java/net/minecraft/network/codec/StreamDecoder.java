package net.minecraft.network.codec;

/** NMS {@code StreamDecoder}: PacketEvents/GrimAC ищут этот класс. */
public interface StreamDecoder<T> {
    T decode(Object input);
}
