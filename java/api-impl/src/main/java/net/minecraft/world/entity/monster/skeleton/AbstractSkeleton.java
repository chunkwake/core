package net.minecraft.world.entity.monster.skeleton;

import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.monster.Monster;

/** NMS {@code AbstractSkeleton}. */
public abstract class AbstractSkeleton extends Monster {
    /** Поля, которые MythicMobs меняет рефлексией. */
    public net.minecraft.world.entity.ai.goal.Goal bowGoal;
    public net.minecraft.world.entity.ai.goal.Goal meleeGoal;
    public AbstractSkeleton(EntityType<?> type, net.minecraft.world.level.Level level) {
        super(type, level);
    }

    public AbstractSkeleton() {
        super();
    }
}
