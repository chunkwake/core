package net.minecraft.world.entity;

/** NMS {@code LivingEntity}: в 26.3 это подтип {@link Avatar}. */
public class LivingEntity extends Avatar {
    /** {@code LivingEntity.getEffect(MobEffect)} (CraftEngine). */
    public net.minecraft.world.effect.MobEffectInstance getEffect(
            net.minecraft.world.effect.MobEffect effect) {
        return null;
    }

    public net.minecraft.world.effect.MobEffectInstance getEffect(
            net.minecraft.core.Holder<net.minecraft.world.effect.MobEffect> effect) {
        return null;
    }

    public net.minecraft.world.entity.item.ItemEntity drop(
            net.minecraft.world.item.ItemStack stack, boolean dropAround,
            net.minecraft.util.Prediction prediction) {
        return null;
    }

    public net.minecraft.world.entity.item.ItemEntity drop(
            net.minecraft.world.item.ItemStack stack, boolean dropAround,
            net.minecraft.util.Prediction prediction, boolean includeNonPlayerKills,
            java.util.function.Consumer<org.bukkit.entity.Item> dropConsumer) {
        return null;
    }

    public net.minecraft.world.entity.item.ItemEntity drop(
            net.minecraft.world.item.ItemStack stack, boolean dropAround,
            net.minecraft.util.Prediction prediction, boolean includeNonPlayerKills,
            boolean suppressDropAnimation,
            java.util.function.Consumer<org.bukkit.entity.Item> dropConsumer) {
        return null;
    }

    public net.minecraft.world.damagesource.DamageSource getLastDamageSource() {
        return null;
    }

    public net.minecraft.world.damagesource.DamageSource getLastDamageSource(int maxAge) {
        return null;
    }

    public net.minecraft.world.phys.AABB getLocalBoundsForPose(Pose pose) {
        return getBoundingBox();
    }

    public final float getScale() {
        return 1.0F;
    }

    public void startUsingItem(net.minecraft.world.InteractionHand hand) {
    }

    public void startUsingItem(net.minecraft.world.InteractionHand hand, boolean force) {
    }

    public net.minecraft.world.entity.player.Player getLastHurtByPlayer() {
        return this.lastHurtByPlayer;
    }

    public boolean swing(net.minecraft.world.InteractionHand hand,
            net.minecraft.world.item.component.SwingAnimation animation, boolean force) {
        return false;
    }

    public boolean addEffect(net.minecraft.world.effect.MobEffectInstance effect) {
        return false;
    }

    public boolean removeEffect(
            net.minecraft.core.Holder<net.minecraft.world.effect.MobEffect> effect) {
        return false;
    }

    public boolean removeAllEffects() {
        return false;
    }

    public net.minecraft.world.item.ItemStack getItemBySlot(
            net.minecraft.world.entity.EquipmentSlot slot) {
        return net.minecraft.world.item.ItemStack.EMPTY;
    }

    public void setItemSlot(net.minecraft.world.entity.EquipmentSlot slot,
            net.minecraft.world.item.ItemStack stack) {
    }

    public net.minecraft.world.entity.player.Player lastHurtByPlayer;
    public net.minecraft.world.damagesource.DamageSource lastDamageSource;

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
