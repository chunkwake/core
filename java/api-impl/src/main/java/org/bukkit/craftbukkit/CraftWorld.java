package org.bukkit.craftbukkit;

/**
 * Заглушка CraftBukkit {@code CraftWorld}: плагины (squaremap и др.)
 * кастуют {@code org.bukkit.World} к этому классу и берут NMS
 * {@code ServerLevel} через {@code getHandle()}.
 *
 * <p>Наши миры ({@code WorldImpl}) наследуются от этого класса, поэтому
 * каст проходит. NMS-часть — заглушка {@code ServerLevel}; она не умеет
 * отдавать настоящие чанки, поэтому плагинам, которым нужен реальный
 * уровень (рендер карт), этого недостаточно — такие места падают уже на
 * методах ServerLevel.
 */
public class CraftWorld {
    private final net.minecraft.server.level.ServerLevel handle =
            new net.minecraft.server.level.ServerLevel();
    /**
     * Алиас, который CraftEngine (CraftWorldProxy) находит по имени:
     * getWorld(World) читает именно поле world.
     */
    public net.minecraft.server.level.ServerLevel world = this.handle;
    /** Алиас для CraftEngine (CraftWorldProxy#getWorldBorder/setWorldBorder). */
    public final net.minecraft.world.level.border.WorldBorder worldBorder =
            new net.minecraft.world.level.border.WorldBorder();

    public CraftWorld() {
    }

    public net.minecraft.server.level.ServerLevel getHandle() {
        return this.handle;
    }

    /** `CraftWorld.getName()`: squaremap читает имя мира из NMS-хендла. */
    public String getName() {
        return "world";
    }

    /** Высота мира для CraftEngine/WorldBorder. */
    public int getMinHeight() {
        return -64;
    }

    public int getMaxHeight() {
        return 320;
    }

    public int getHeight() {
        return this.getMaxHeight() - this.getMinHeight();
    }
}
