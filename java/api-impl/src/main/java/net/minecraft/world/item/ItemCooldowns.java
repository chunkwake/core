package net.minecraft.world.item;

import java.util.HashMap;
import java.util.Map;

/** NMS {@code ItemCooldowns}. */
public class ItemCooldowns {
    private final Map<Item, Integer> cooldowns = new HashMap<>();

    public ItemCooldowns() {
    }

    public boolean isOnCooldown(Item item) {
        return this.cooldowns.containsKey(item);
    }

    public void addCooldown(Item item, int ticks) {
        this.cooldowns.put(item, ticks);
    }

    public void removeCooldown(Item item) {
        this.cooldowns.remove(item);
    }

    public void tick() {
    }
}
