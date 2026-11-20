package net.minecraft.world.entity.item;

import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.level.Level;

/** NMS {@code PrimedTnt} (MythicMobs). */
public class PrimedTnt extends Entity {
    private int fuse = 80;
    private float explosionPower = 4.0F;

    public PrimedTnt(EntityType<?> type, Level level) {
        super(type, level);
    }

    public int getFuse() {
        return this.fuse;
    }

    public void setFuse(int fuse) {
        this.fuse = fuse;
    }

    public float getExplosionPower() {
        return this.explosionPower;
    }

    public void setExplosionPower(float power) {
        this.explosionPower = power;
    }
}
