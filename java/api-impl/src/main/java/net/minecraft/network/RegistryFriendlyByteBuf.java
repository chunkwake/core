package net.minecraft.network;

import io.netty.buffer.ByteBuf;
import net.minecraft.core.RegistryAccess;

/** NMS {@code RegistryFriendlyByteBuf}: PacketEvents/GrimAC ищут класс. */
public class RegistryFriendlyByteBuf extends FriendlyByteBuf {
    private final RegistryAccess registries;

    public RegistryFriendlyByteBuf(ByteBuf buf) {
        this(buf, RegistryAccess.empty());
    }

    public RegistryFriendlyByteBuf(ByteBuf buf, RegistryAccess registries) {
        super(buf);
        this.registries = registries;
    }

    public RegistryAccess registryAccess() {
        return this.registries;
    }
}
