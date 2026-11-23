package org.bukkit.craftbukkit;

/**
 * Минимальный CraftWorldBorder: CraftEngine наследует его для
 * {@code InjectedWorldBorder}, чтобы получить доступ к мирам через
 * Bukkit-объект. Полная реализация WorldBorder живёт в Rust-ядре.
 */
public class CraftWorldBorder extends net.minecraft.world.level.border.WorldBorder {
    protected final CraftWorld world;

    public CraftWorldBorder(CraftWorld world) {
        super();
        this.world = world;
    }

    public CraftWorld getWorld() {
        return this.world;
    }
}
