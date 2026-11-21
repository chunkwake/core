package net.minecraft.world.entity;

/** NMS {@code LivingEntity}: в 26.3 это подтип {@link Avatar}. */
public class LivingEntity extends Avatar {
    /** {@code LivingEntity.getEffect(MobEffect)} (CraftEngine). */
    public net.minecraft.world.effect.MobEffectInstance getEffect(
            net.minecraft.world.effect.MobEffect effect) {
        return null;
    }

    /** {@code LivingEntity.getAttribute(Holder)} (CraftEngine). */
    public net.minecraft.world.entity.ai.attributes.AttributeInstance getAttribute(
            net.minecraft.core.Holder<net.minecraft.world.entity.ai.attributes.Attribute> attribute) {
        return null;
    }

    protected LivingEntity(java.util.UUID uuid) {
        super(uuid);
    }

    public LivingEntity() {
        super();
    }

    public LivingEntity(EntityType<?> type, net.minecraft.world.level.Level level) {
        super(type, level);
    }
}
