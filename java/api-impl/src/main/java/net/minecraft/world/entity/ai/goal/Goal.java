package net.minecraft.world.entity.ai.goal;

import java.util.EnumSet;

/** NMS {@code Goal}; MythicMobs создаёт свои цели через GoalSelector. */
public abstract class Goal {
    public enum Flag {
        MOVE,
        LOOK,
        JUMP,
        TARGET
    }

    private EnumSet<Flag> flags = EnumSet.noneOf(Flag.class);

    public abstract boolean canUse();

    public boolean canContinueToUse() {
        return this.canUse();
    }

    public boolean isInterruptable() {
        return true;
    }

    public void start() {
    }

    public void stop() {
    }

    public boolean requiresUpdateEveryTick() {
        return false;
    }

    public void tick() {
    }

    public void setFlags(EnumSet<Flag> flags) {
        this.flags = flags;
    }

    public EnumSet<Flag> getFlags() {
        return this.flags;
    }

    protected int adjustedTickDelay(int ticks) {
        return this.requiresUpdateEveryTick() ? ticks : reducedTickDelay(ticks);
    }

    protected static int reducedTickDelay(int ticks) {
        return (int) Math.ceil(ticks / 2.0D);
    }
}
