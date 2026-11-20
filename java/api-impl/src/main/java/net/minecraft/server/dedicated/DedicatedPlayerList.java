package net.minecraft.server.dedicated;

/**
 * NMS {@code DedicatedPlayerList}: нужен InvSee++ через
 * {@code CraftServer.getHandle().playerIo}.
 */
public class DedicatedPlayerList extends net.minecraft.server.players.PlayerList {
    public net.minecraft.world.level.storage.PlayerDataStorage playerIo =
            new net.minecraft.world.level.storage.PlayerDataStorage();

    /** {@code DedicatedPlayerList.getServer()} (MythicMobs). */
    public DedicatedServer getServer() {
        return DedicatedServer.create();
    }
}
