package net.minecraft.world.entity.ai.goal;

/** NMS {@code WrappedGoal}: цель + приоритет. */
public class WrappedGoal implements Comparable<WrappedGoal> {
    private final int priority;
    private final Goal goal;

    public WrappedGoal(int priority, Goal goal) {
        this.priority = priority;
        this.goal = goal;
    }

    public int getPriority() {
        return this.priority;
    }

    public Goal getGoal() {
        return this.goal;
    }

    public boolean isRunning() {
        return false;
    }

    @Override
    public int compareTo(WrappedGoal other) {
        return Integer.compare(this.priority, other.priority);
    }
}
