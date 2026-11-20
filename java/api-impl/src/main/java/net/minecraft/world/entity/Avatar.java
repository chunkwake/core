package net.minecraft.world.entity;

import net.minecraft.world.entity.player.Player;

/** NMS {@code Avatar}: базовая живая сущность (26.3). */
public class Avatar extends Entity {
    private float health = 20.0F;

    protected Avatar(java.util.UUID uuid) {
        super(uuid);
    }

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
        if (this instanceof net.minecraft.server.level.ServerPlayer) {
            org.bukkit.entity.Player player = org.bukkit.Bukkit.getPlayer(this.getUUID());
            if (player instanceof org.bukkit.craftbukkit.entity.CraftLivingEntity living) {
                return living;
            }
        }
        return new org.bukkit.craftbukkit.entity.CraftLivingEntity(this);
    }
}
