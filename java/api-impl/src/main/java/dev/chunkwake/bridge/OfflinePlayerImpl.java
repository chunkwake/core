package dev.chunkwake.bridge;

import java.util.UUID;
import org.bukkit.Location;
import org.bukkit.OfflinePlayer;
import org.bukkit.entity.Player;

/**
 * {@link OfflinePlayer}: игрок, которого нет в сети (или есть — тогда
 * проксирует к {@link PlayerImpl}). Имя берётся из кэша входов.
 */
final class OfflinePlayerImpl {
    private final ChunkwakeServer server;
    private final UUID id;
    private String name;
    final OfflinePlayer api = Stub.of(OfflinePlayer.class, this);

    OfflinePlayerImpl(ChunkwakeServer server, UUID id, String name) {
        this.server = server;
        this.id = id;
        this.name = name;
    }

    public UUID getUniqueId() { return this.id; }

    public String getName() {
        if (this.name != null) {
            return this.name;
        }
        Player online = this.server.getPlayer(this.id);
        if (online != null) {
            return online.getName();
        }
        this.name = this.server.nameOf(this.id);
        return this.name;
    }

    public boolean isOnline() {
        return this.server.getPlayer(this.id) != null;
    }

    public boolean isConnected() { return isOnline(); }

    public Player getPlayer() {
        return this.server.getPlayer(this.id);
    }

    public boolean hasPlayedBefore() {
        return this.server.hasPlayed(this.id);
    }

    public Location getBedSpawnLocation() { return null; }

    public Location getRespawnLocation(boolean loadLocationAndValidate) { return null; }

    public long getFirstPlayed() { return 0L; }

    public long getLastPlayed() {
        return this.server.lastSeen(this.id);
    }

    public long getLastSeen() {
        return this.server.lastSeen(this.id);
    }

    public boolean isBanned() { return false; }
    public boolean isWhitelisted() { return false; }
    public void setWhitelisted(boolean value) {}
    public boolean isOp() { return Bridge.opLevel(this.id) > 0; }
    public void setOp(boolean value) { Bridge.setOp(this.id, value); }

    public com.destroystokyo.paper.profile.PlayerProfile getPlayerProfile() {
        return this.server.profile(this.id, getName());
    }

    public org.bukkit.profile.PlayerProfile getPlayerProfile(boolean copy) {
        return this.server.bukkitProfile(this.id, getName());
    }

    @Override public String toString() {
        return "OfflinePlayer{name=" + getName() + ", uuid=" + this.id + "}";
    }
}
