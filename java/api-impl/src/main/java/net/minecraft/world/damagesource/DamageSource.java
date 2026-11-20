package net.minecraft.world.damagesource;

import net.minecraft.core.Holder;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.phys.Vec3;

/** NMS {@code DamageSource}. */
public class DamageSource {
    private final Holder<DamageType> type;
    private final Entity causingEntity;
    private final Entity directEntity;
    private final Vec3 position;

    public DamageSource(Holder<DamageType> type) {
        this(type, null, null, null);
    }

    public DamageSource(Holder<DamageType> type, Entity entity) {
        this(type, entity, entity, null);
    }

    public DamageSource(Holder<DamageType> type, Entity causing, Entity direct) {
        this(type, causing, direct, null);
    }

    public DamageSource(Holder<DamageType> type, Vec3 position) {
        this(type, null, null, position);
    }

    private DamageSource(Holder<DamageType> type, Entity causing, Entity direct, Vec3 position) {
        this.type = type;
        this.causingEntity = causing;
        this.directEntity = direct;
        this.position = position;
    }

    public Entity getEntity() {
        return this.causingEntity;
    }

    public Entity getDirectEntity() {
        return this.directEntity;
    }

    public ItemStack getWeaponItem() {
        return null;
    }

    public Component getLocalizedDeathMessage(LivingEntity entity) {
        return null;
    }

    public String getMsgId() {
        return "generic";
    }

    public boolean isCreativePlayer() {
        return false;
    }

    public Vec3 getSourcePosition() {
        return this.position;
    }

    public Vec3 sourcePositionRaw() {
        return this.position;
    }

    public boolean is(net.minecraft.resources.ResourceKey<DamageType> key) {
        return false;
    }

    public boolean is(net.minecraft.tags.TagKey<DamageType> tag) {
        return false;
    }

    public DamageType type() {
        return this.type == null ? null : this.type.value();
    }

    public Holder<DamageType> typeHolder() {
        return this.type;
    }

    public boolean scalesWithDifficulty() {
        return false;
    }

    public float getFoodExhaustion() {
        return 0.1F;
    }

    @Override
    public String toString() {
        return "DamageSource[" + this.type + "]";
    }
}
