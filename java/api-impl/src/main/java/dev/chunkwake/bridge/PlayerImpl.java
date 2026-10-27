package dev.chunkwake.bridge;

import java.util.UUID;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.ComponentLike;
import net.kyori.adventure.text.serializer.gson.GsonComponentSerializer;
import org.bukkit.Server;
import org.bukkit.World;
import org.bukkit.entity.Player;
import org.bukkit.permissions.Permission;

/** {@link Player}: онлайн-игрок Rust-сервера (MVP — всё, что нужно для чата и событий). */
final class PlayerImpl {
    final Player api = Stub.of(Player.class, this);
    private final ChunkwakeServer server;
    final UUID id;
    final String name;
    volatile boolean online = true;

    PlayerImpl(ChunkwakeServer server, UUID id, String name) {
        this.server = server;
        this.id = id;
        this.name = name;
    }

    public String getName() { return name; }
    public UUID getUniqueId() { return id; }
    public Server getServer() { return server.api; }
    public World getWorld() { return server.world; }
    public boolean isOnline() { return online; }
    public boolean isConnected() { return online; }
    // MVP: все игроки — операторы, пока нет прав.
    public boolean isOp() { return true; }
    public boolean hasPermission(String p) { return true; }
    public boolean hasPermission(Permission p) { return true; }
    public boolean isPermissionSet(String p) { return true; }
    public Component name() { return Component.text(name); }
    public Component displayName() { return Component.text(name); }
    public String getDisplayName() { return name; }

    public void sendMessage(String message) {
        sendMessage(ChunkwakeServer.legacy(message));
    }

    public void sendMessage(String... messages) {
        for (String m : messages) sendMessage(m);
    }

    public void sendMessage(Component message) {
        if (online) {
            Bridge.sendMessage(id, GsonComponentSerializer.gson().serialize(message));
        }
    }

    public void sendMessage(ComponentLike message) {
        sendMessage(message.asComponent());
    }

    @Override
    public String toString() {
        return "ChunkwakePlayer{name=" + name + "}";
    }
}
