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

    public CraftWorld() {
    }

    public net.minecraft.server.level.ServerLevel getHandle() {
        return this.handle;
    }

    /** `CraftWorld.getName()`: squaremap читает имя мира из NMS-хендла. */
    public String getName() {
        return "world";
    }
}
