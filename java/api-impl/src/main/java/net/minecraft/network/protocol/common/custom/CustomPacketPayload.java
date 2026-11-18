package net.minecraft.network.protocol.common.custom;

import net.minecraft.network.codec.StreamCodec;
import net.minecraft.resources.Identifier;

/** NMS {@code CustomPacketPayload}. */
public interface CustomPacketPayload {
    Type<? extends CustomPacketPayload> type();

    default Identifier id() {
        return this.type().id();
    }

    /** Тип payload'а: id + кодек. */
    record Type<T extends CustomPacketPayload>(Identifier id, StreamCodec<T> codec) {
        public static <T extends CustomPacketPayload> Type<T> create(Identifier id,
                StreamCodec<T> codec) {
            return new Type<>(id, codec);
        }
    }
}
