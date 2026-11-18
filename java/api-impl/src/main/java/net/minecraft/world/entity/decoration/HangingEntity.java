package net.minecraft.world.entity.decoration;

import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.Level;

/** NMS {@code HangingEntity}. */
public class HangingEntity extends Entity {
    public HangingEntity() {
    }

    public HangingEntity(net.minecraft.world.entity.EntityType<?> type, Level level) {
        super(type, level);
    }
}
