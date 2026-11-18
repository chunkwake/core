package org.bukkit.craftbukkit.entity;

import net.minecraft.server.level.ServerPlayer;

/**
 * CraftBukkit-совместимый класс игрока: наследник {@link CraftHumanEntity}
 * с {@code getHandle()}, который ищут DecentHolograms, TAB и другие плагины.
 */
public class CraftPlayer extends CraftHumanEntity {
    private final ServerPlayer handle = new ServerPlayer();

    public CraftPlayer(Object impl) {
        super(impl);
    }

    public ServerPlayer getHandle() {
        return this.handle;
    }
}
