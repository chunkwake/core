package net.minecraft.world.entity.projectile;

import net.minecraft.world.entity.Entity;

/**
 * NMS {@code Projectile}: базовая сущность-снаряд.
 *
 * <p>Нужен только сигнатуре {@code BlockBehaviour.onProjectileHit}.
 */
public class Projectile extends Entity {
    public Projectile() {
        super();
    }
}
