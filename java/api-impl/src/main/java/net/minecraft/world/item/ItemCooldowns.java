package net.minecraft.world.item;

import java.util.HashMap;
import java.util.Map;

/** NMS {@code ItemCooldowns}. */
public class ItemCooldowns {
    /** В 26.3 ключ — id предмета (Identifier), значение — таймеры. */
    private final Map<Object, CooldownInstance> cooldowns = new HashMap<>();
    public int tickCount;

    public ItemCooldowns() {
    }

    /** {@code ItemCooldowns.addCooldown(ItemStack, int)} (CraftEngine). */
    public void addCooldown(net.minecraft.world.item.ItemStack item, int ticks) {
    }

    public boolean isOnCooldown(Item item) {
        return this.cooldowns.containsKey(item);
    }

    public void addCooldown(Item item, int ticks) {
        this.cooldowns.put(item, new CooldownInstance(this.tickCount, this.tickCount + ticks));
    }

    public void removeCooldown(Item item) {
        this.cooldowns.remove(item);
    }

    public void tick() {
        this.tickCount++;
    }

    /** NMS {@code ItemCooldowns.CooldownInstance}: MythicMobs читает startTime/endTime. */
    public static final class CooldownInstance {
        public int startTime;
        public int endTime;

        public CooldownInstance(int startTime, int endTime) {
            this.startTime = startTime;
            this.endTime = endTime;
        }

        public int remainingTicks() {
            return this.endTime - this.startTime;
        }
    }
}
