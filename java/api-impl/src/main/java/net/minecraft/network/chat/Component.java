package net.minecraft.network.chat;

import com.mojang.serialization.Codec;
import com.mojang.serialization.SimpleCodec;

/** Минимальный NMS-компонент текста для ProtocolLib. */
public class Component {
    public static class Serializer {
        @SuppressWarnings({"rawtypes", "unchecked"})
        public static final Codec<Component> CODEC = (Codec) SimpleCodec.INSTANCE;

        private Serializer() {
        }
    }
}
