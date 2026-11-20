package net.minecraft.world.entity;

/** NMS {@code LivingEntity}: в 26.3 это подтип {@link Avatar}. */
public class LivingEntity extends Avatar {
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
