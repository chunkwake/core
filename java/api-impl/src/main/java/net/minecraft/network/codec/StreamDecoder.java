package net.minecraft.network.codec;

/** NMS {@code StreamDecoder} (Paper 26.x: buffer + value). */
public interface StreamDecoder<B, T> {
    T decode(B input);
}
