package net.minecraft.network.protocol.game;

import net.minecraft.core.Holder;
import net.minecraft.network.protocol.Packet;
import net.minecraft.world.effect.MobEffect;
import net.minecraft.world.effect.MobEffectInstance;

/** NMS {@code ClientboundUpdateMobEffectPacket} (CraftEngine). */
public class ClientboundUpdateMobEffectPacket implements Packet {
    private int entityId;
    private Holder<MobEffect> effect;
    private int effectAmplifier;
    private int effectDurationTicks;
    private byte flags;

    public ClientboundUpdateMobEffectPacket() {
    }

    public ClientboundUpdateMobEffectPacket(int entityId, MobEffectInstance instance, boolean blink) {
        this.entityId = entityId;
        if (instance != null) {
            this.effectAmplifier = instance.getAmplifier();
            this.effectDurationTicks = instance.getDuration();
        }
    }

    public ClientboundUpdateMobEffectPacket(int entityId, Holder<MobEffect> effect) {
        this.entityId = entityId;
        this.effect = effect;
    }

    public void setEntityId(int entityId) { this.entityId = entityId; }
    public void setEffectAmplifier(int amplifier) { this.effectAmplifier = amplifier; }
    public void setEffectAmplifier$legacy(byte amplifier) { this.effectAmplifier = amplifier; }
    public void setEffectDurationTicks(int ticks) { this.effectDurationTicks = ticks; }
    public void setEffect(Holder<MobEffect> effect) { this.effect = effect; }
    public void setEffect$legacy(MobEffect effect) { this.effect = null; }
    public void setFlags(byte flags) { this.flags = flags; }
}
