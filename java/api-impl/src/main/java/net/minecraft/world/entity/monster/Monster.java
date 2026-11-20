package net.minecraft.world.entity.monster;

/** NMS {@code Monster}. */
public abstract class Monster extends net.minecraft.world.entity.PathfinderMob {
    public Monster(net.minecraft.world.entity.EntityType<?> type,
            net.minecraft.world.level.Level level) {
        super(type, level);
    }

    public Monster() {
        super();
    }
}
