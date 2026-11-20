package net.minecraft.world.entity;

import net.minecraft.world.entity.ai.control.LookControl;
import net.minecraft.world.entity.ai.control.MoveControl;
import net.minecraft.world.entity.ai.goal.GoalSelector;
import net.minecraft.world.entity.ai.navigation.PathNavigation;

/** NMS {@code Mob}: база для мобов MythicMobs. */
public abstract class Mob extends LivingEntity {
    public final GoalSelector goalSelector = new GoalSelector();
    public final GoalSelector targetSelector = new GoalSelector();
    /** Поле, которое MythicMobs ищет рефлексией (как в ванильном Mob). */
    public PathNavigation navigation;
    private boolean noAi;
    private LivingEntity target;

    public Mob(EntityType<?> type, net.minecraft.world.level.Level level) {
        super(type, level);
    }

    public GoalSelector getGoalSelector() {
        return this.goalSelector;
    }

    public GoalSelector getTargetSelector() {
        return this.targetSelector;
    }

    public PathNavigation getNavigation() {
        return this.navigation;
    }

    public LookControl getLookControl() {
        return null;
    }

    public MoveControl getMoveControl() {
        return null;
    }

    public LivingEntity getTarget() {
        return this.target;
    }

    public void setTarget(LivingEntity target) {
        this.target = target;
    }

    public boolean isNoAi() {
        return this.noAi;
    }

    public void setNoAi(boolean noAi) {
        this.noAi = noAi;
    }

    public boolean isPersistenceRequired() {
        return false;
    }

    public void setPersistenceRequired() {
    }
}
