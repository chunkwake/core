package net.minecraft.world.entity.ai.goal;

import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;
import java.util.function.Predicate;

/** NMS {@code GoalSelector}: реестр целей моба. */
public class GoalSelector {
    private final List<WrappedGoal> goals = new ArrayList<>();
    /** Поля, которые MythicMobs ищет рефлексией. */
    public final java.util.Map<Goal.Flag, WrappedGoal> lockedFlags = new java.util.EnumMap<>(Goal.Flag.class);
    public final Set<WrappedGoal> availableGoals = new LinkedHashSet<>();

    public GoalSelector() {
    }

    public void addGoal(int priority, Goal goal) {
        WrappedGoal wrapped = new WrappedGoal(priority, goal);
        this.goals.add(wrapped);
        this.availableGoals.add(wrapped);
    }

    public void removeGoal(Goal goal) {
        this.goals.removeIf(wrapped -> wrapped.getGoal() == goal);
    }

    public void removeAllGoals(Predicate<Goal> predicate) {
        this.goals.removeIf(wrapped -> predicate.test(wrapped.getGoal()));
    }

    public void tick() {
    }

    public void tickRunningGoals(boolean tickAll) {
    }

    public Set<WrappedGoal> getAvailableGoals() {
        return new LinkedHashSet<>(this.goals);
    }

    public void disableControlFlag(Goal.Flag flag) {
    }

    public void enableControlFlag(Goal.Flag flag) {
    }

    public void setControlFlag(Goal.Flag flag, boolean enabled) {
    }
}
