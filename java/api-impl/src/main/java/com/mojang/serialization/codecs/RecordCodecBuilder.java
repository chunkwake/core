package com.mojang.serialization.codecs;

/** Минимальный DFU {@code RecordCodecBuilder} (VoiceMessages). */
public final class RecordCodecBuilder {
    private RecordCodecBuilder() {
    }

    public interface Instance {
    }

    public static <A> com.mojang.serialization.Codec<A> create(
            java.util.function.Function<Instance, ?> builder) {
        @SuppressWarnings({"rawtypes", "unchecked"})
        com.mojang.serialization.Codec<A> codec =
                (com.mojang.serialization.Codec) com.mojang.serialization.SimpleCodec.INSTANCE;
        return codec;
    }
}
