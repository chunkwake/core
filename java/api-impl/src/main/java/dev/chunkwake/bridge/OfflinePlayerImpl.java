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

    /** Корень сервера, где лежат whitelist.json/banned-players.json. */
    private java.nio.file.Path accessFile(String name) {
        java.nio.file.Path level = server.levelDirectory();
        java.nio.file.Path root = level == null ? null : level.getParent();
        return root == null ? null : root.resolve(name);
    }

    private com.google.gson.JsonArray readAccess(String name) {
        java.nio.file.Path file = accessFile(name);
        if (file == null || !java.nio.file.Files.isRegularFile(file)) {
            return new com.google.gson.JsonArray();
        }
        try {
            String text = java.nio.file.Files.readString(
                    file, java.nio.charset.StandardCharsets.UTF_8);
            com.google.gson.JsonElement parsed = com.google.gson.JsonParser.parseString(text);
            return parsed.isJsonArray() ? parsed.getAsJsonArray() : new com.google.gson.JsonArray();
        } catch (Exception e) {
            return new com.google.gson.JsonArray();
        }
    }

    private void writeAccess(String name, com.google.gson.JsonArray entries) {
        java.nio.file.Path file = accessFile(name);
        if (file == null) {
            return;
        }
        try {
            java.nio.file.Files.createDirectories(file.getParent());
            java.nio.file.Files.writeString(file,
                    new com.google.gson.Gson().toJson(entries),
                    java.nio.charset.StandardCharsets.UTF_8);
        } catch (Exception ignored) {
            // как Bukkit: ошибка записи не ломает вызов плагина
        }
    }

    private static boolean sameEntry(com.google.gson.JsonObject entry, UUID id, String name) {
        if (entry.has("uuid") && entry.get("uuid").isJsonPrimitive()
                && id.toString().equalsIgnoreCase(entry.get("uuid").getAsString())) {
            return true;
        }
        return name != null && entry.has("name") && entry.get("name").isJsonPrimitive()
                && name.equalsIgnoreCase(entry.get("name").getAsString());
    }

    private boolean listed(String file) {
        String name = this.name != null ? this.name : getName();
        for (com.google.gson.JsonElement e : readAccess(file)) {
            if (e.isJsonObject() && sameEntry(e.getAsJsonObject(), id, name)) {
                return true;
            }
        }
        return false;
    }

    public boolean isBanned() {
        String name = this.name != null ? this.name : getName();
        for (com.google.gson.JsonElement e : readAccess("banned-players.json")) {
            if (!e.isJsonObject()) {
                continue;
            }
            com.google.gson.JsonObject entry = e.getAsJsonObject();
            if (!sameEntry(entry, id, name)) {
                continue;
            }
            String expires = entry.has("expires") ? entry.get("expires").getAsString() : "forever";
            if ("forever".equalsIgnoreCase(expires)) {
                return true;
            }
            try {
                java.time.Instant until = java.time.OffsetDateTime.parse(expires).toInstant();
                return until.isAfter(java.time.Instant.now());
            } catch (RuntimeException ignored) {
                return true;
            }
        }
        return false;
    }

    public boolean isWhitelisted() {
        return listed("whitelist.json");
    }

    public void setWhitelisted(boolean value) {
        String name = this.name != null ? this.name : getName();
        com.google.gson.JsonArray entries = readAccess("whitelist.json");
        boolean found = false;
        for (int i = entries.size() - 1; i >= 0; i--) {
            com.google.gson.JsonElement e = entries.get(i);
            if (!e.isJsonObject() || !sameEntry(e.getAsJsonObject(), id, name)) {
                continue;
            }
            if (value) {
                found = true;
            } else {
                entries.remove(i);
            }
        }
        if (value && !found) {
            com.google.gson.JsonObject entry = new com.google.gson.JsonObject();
            entry.addProperty("uuid", id.toString());
            entry.addProperty("name", name);
            entries.add(entry);
        }
        writeAccess("whitelist.json", entries);
    }
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
