package net.minecraft.network.codec;

/** NMS {@code StreamMemberEncoder} (Pl3xMap). */
public interface StreamMemberEncoder<O, T> {
    void encode(T value, O output);
}
