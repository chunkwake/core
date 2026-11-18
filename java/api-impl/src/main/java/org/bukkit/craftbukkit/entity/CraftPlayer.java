package org.bukkit.craftbukkit.entity;

import dev.chunkwake.bridge.PlayerCalls;
import net.minecraft.server.level.ServerPlayer;

/**
 * CraftBukkit-совместимый класс игрока: наследник {@link CraftHumanEntity}
 * с {@code getHandle()}, который ищут DecentHolograms, TAB и другие плагины.
 */
public class CraftPlayer extends CraftHumanEntity {
    private final ServerPlayer fallback = new ServerPlayer();

    public CraftPlayer(Object impl) {
        super(impl);
    }

    /** Конструктор как у CraftBukkit: `CraftPlayer(CraftServer, ServerPlayer)`. */
    public CraftPlayer(org.bukkit.craftbukkit.CraftServer server, ServerPlayer entity) {
        super(server, entity);
    }

    public ServerPlayer getHandle() {
        return this.entity != null ? this.entity : this.fallback;
    }

    /** CraftBukkit-методы каналов, которые ищет Simple Voice Chat. */
    public void addChannel(String channel) {
        PlayerCalls.call(getHandle(), "addChannel", new Class<?>[]{ String.class },
                new Object[]{ channel }, void.class);
    }

    public void removeChannel(String channel) {
        PlayerCalls.call(getHandle(), "removeChannel", new Class<?>[]{ String.class },
                new Object[]{ channel }, void.class);
    }
}
