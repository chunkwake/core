package org.bukkit.craftbukkit.entity;

import dev.chunkwake.bridge.PlayerCalls;
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

    /** CraftBukkit-методы каналов, которые ищет Simple Voice Chat. */
    public void addChannel(String channel) {
        PlayerCalls.call(this.handle, "addChannel", new Class<?>[]{ String.class },
                new Object[]{ channel }, void.class);
    }

    public void removeChannel(String channel) {
        PlayerCalls.call(this.handle, "removeChannel", new Class<?>[]{ String.class },
                new Object[]{ channel }, void.class);
    }
}
