package net.minecraft.world.entity;

import net.minecraft.world.entity.player.Player;

/** NMS {@code LivingEntity}. */
public class LivingEntity extends Entity {
    private float health = 20.0F;

    public LivingEntity() {
    }

    public LivingEntity(EntityType<?> type, net.minecraft.world.level.Level level) {
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
}
