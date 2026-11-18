package net.minecraft.network.chat;

import com.mojang.serialization.Codec;
import com.mojang.serialization.SimpleCodec;

/** Минимальный NMS-стиль текста для ProtocolLib. */
public class Style {
    public static class Serializer {
        @SuppressWarnings({"rawtypes", "unchecked"})
        public static final Codec<Style> CODEC = (Codec) SimpleCodec.INSTANCE;

        private Serializer() {
        }
    }
}
