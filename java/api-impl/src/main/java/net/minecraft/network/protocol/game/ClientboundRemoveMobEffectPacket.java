package net.minecraft.network.protocol.game;

import net.minecraft.core.Holder;
import net.minecraft.network.protocol.Packet;
import net.minecraft.world.effect.MobEffect;

/** NMS {@code ClientboundRemoveMobEffectPacket} (CraftEngine). */
public class ClientboundRemoveMobEffectPacket implements Packet {
    private int entityId;
    private Holder<MobEffect> effect;

    public ClientboundRemoveMobEffectPacket() {
    }

    public ClientboundRemoveMobEffectPacket(int entityId, Holder<MobEffect> effect) {
        this.entityId = entityId;
        this.effect = effect;
    }

    public ClientboundRemoveMobEffectPacket(int entityId, MobEffect effect) {
        this(entityId, (Holder<MobEffect>) null);
    }
}
