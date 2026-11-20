package net.minecraft.world.entity;

import net.minecraft.world.entity.player.Player;

/** NMS {@code Avatar}: базовая живая сущность (26.3). */
public class Avatar extends Entity {
    private float health = 20.0F;

    public Avatar() {
    }

    public Avatar(EntityType<?> type, net.minecraft.world.level.Level level) {
        super(type, level);
    }

    public float getHealth() {
        return this.health;
    }

    public void setHealth(float health) {
        this.health = health;
    }

    public boolean isAlive() {
        return !this.isRemoved() && this.health > 0.0F;
    }

    public boolean isDeadOrDying() {
        return this.health <= 0.0F;
    }

    public Player getLastHurtByPlayer() {
        return null;
    }

    /**
     * {@code Avatar.getBukkitEntity()}: в 26.3 живая сущность возвращает
     * {@code CraftLivingEntity} (Emotecraft relies on the exact type).
     */
    @Override
    public org.bukkit.craftbukkit.entity.CraftLivingEntity getBukkitEntity() {
        return new org.bukkit.craftbukkit.entity.CraftLivingEntity(this);
    }
}
