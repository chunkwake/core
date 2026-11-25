package dev.chunkwake.bridge;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Duration;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;
import java.util.Date;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.Locale;
import java.util.Map;
import java.util.Set;

/** {@code /ban} и {@code /ban-ip}: файлы Paper в корне сервера. */
final class BanListImpl implements org.bukkit.BanList {
    private static final DateTimeFormatter DATE = DateTimeFormatter
            .ofPattern("yyyy-MM-dd HH:mm:ss Z", Locale.ROOT).withZone(ZoneOffset.UTC);
    private static final DateTimeFormatter ISO = DateTimeFormatter.ISO_INSTANT;

    private final ChunkwakeServer server;
    private final org.bukkit.BanList.Type type;
    private final Map<String, BanEntryImpl> entries = new LinkedHashMap<>();

    BanListImpl(ChunkwakeServer server, org.bukkit.BanList.Type type) {
        this.server = server;
        this.type = type;
        load();
    }

    private Path file() {
        Path root = server.levelDirectory() == null ? null : server.levelDirectory().getParent();
        if (root == null) {
            return null;
        }
        return root.resolve(type == org.bukkit.BanList.Type.IP
                ? "banned-ips.json" : "banned-players.json");
    }

    private String key(String target) {
        return type == org.bukkit.BanList.Type.IP
                ? target : target.toLowerCase(Locale.ROOT);
    }

    private static String date(Date date) {
        return date == null ? "forever" : DATE.format(date.toInstant());
    }

    private static Date parseDate(String text) {
        if (text == null || text.isBlank() || "forever".equalsIgnoreCase(text)) {
            return null;
        }
        try {
            return Date.from(OffsetDateTime.parse(text).toInstant());
        } catch (RuntimeException ignored) {
            // дата Paper: yyyy-MM-dd HH:mm:ss Z
        }
        try {
            return Date.from(ISO.parse(text, Instant::from));
        } catch (RuntimeException ignored) {
            return null;
        }
    }

    private synchronized void load() {
        entries.clear();
        Path file = file();
        if (file == null || !Files.isRegularFile(file)) {
            return;
        }
        try {
            JsonElement parsed = JsonParser.parseString(Files.readString(file, StandardCharsets.UTF_8));
            if (!parsed.isJsonArray()) {
                return;
            }
            for (JsonElement element : parsed.getAsJsonArray()) {
                if (!element.isJsonObject()) {
                    continue;
                }
                JsonObject object = element.getAsJsonObject();
                String target = type == org.bukkit.BanList.Type.IP
                        ? string(object, "ip")
                        : (object.has("name") ? string(object, "name") : string(object, "uuid"));
                if (target == null || target.isBlank()) {
                    continue;
                }
                Date created = parseDate(string(object, "created"));
                Date expires = parseDate(string(object, "expires"));
                entries.put(key(target), new BanEntryImpl(this, target,
                        created == null ? new Date() : created,
                        string(object, "source"), expires, string(object, "reason")));
            }
        } catch (Exception ignored) {
            // битый файл — пустой список
        }
    }

    private static String string(JsonObject object, String name) {
        return object.has(name) && object.get(name).isJsonPrimitive()
                ? object.get(name).getAsString() : null;
    }

    synchronized void save() {
        Path file = file();
        if (file == null) {
            return;
        }
        try {
            JsonArray out = new JsonArray();
            for (BanEntryImpl entry : entries.values()) {
                JsonObject object = new JsonObject();
                object.addProperty(type == org.bukkit.BanList.Type.IP ? "ip" : "name", entry.target);
                object.addProperty("created", date(entry.getCreated()));
                object.addProperty("source", entry.getSource());
                object.addProperty("expires", date(entry.getExpiration()));
                object.addProperty("reason", entry.getReason());
                out.add(object);
            }
            Files.createDirectories(file.getParent());
            Files.writeString(file, out.toString(), StandardCharsets.UTF_8);
        } catch (Exception ignored) {
            // как Bukkit: ошибка записи не ломает вызов плагина
        }
    }

    private synchronized BanEntryImpl entry(String target) {
        return target == null ? null : entries.get(key(target));
    }

    @Override public org.bukkit.BanEntry getBanEntry(String target) { return entry(target); }

    @Override public org.bukkit.BanEntry getBanEntry(Object target) {
        return target == null ? null : entry(String.valueOf(target));
    }

    @Override public org.bukkit.BanEntry addBan(String target, String reason,
            Date expires, String source) {
        if (target == null || target.isBlank()) {
            throw new IllegalArgumentException("target cannot be empty");
        }
        synchronized (this) {
            BanEntryImpl entry = entries.computeIfAbsent(key(target),
                    ignored -> new BanEntryImpl(this, target, new Date(), source, expires, reason));
            entry.setCreated(new Date());
            entry.setSource(source == null ? "Chunkwake" : source);
            entry.setExpiration(expires);
            entry.setReason(reason == null ? "Banned by an operator." : reason);
            save();
            return entry;
        }
    }

    @Override public org.bukkit.BanEntry addBan(Object target, String reason,
            Date expires, String source) {
        return addBan(target == null ? null : String.valueOf(target), reason, expires, source);
    }

    @Override public org.bukkit.BanEntry addBan(Object target, String reason,
            Instant expires, String source) {
        return addBan(target, reason, expires == null ? null : Date.from(expires), source);
    }

    @Override public org.bukkit.BanEntry addBan(Object target, String reason,
            Duration duration, String source) {
        return addBan(target, reason,
                duration == null ? null : Date.from(Instant.now().plus(duration)), source);
    }

    @Override public synchronized Set<org.bukkit.BanEntry> getBanEntries() {
        return new LinkedHashSet<>(entries.values());
    }

    @Override public synchronized Set getEntries() {
        return getBanEntries();
    }

    org.bukkit.BanList.Type type() { return this.type; }

    @Override public synchronized boolean isBanned(Object target) {
        return isBanned(target == null ? null : String.valueOf(target));
    }

    @Override public synchronized boolean isBanned(String target) {
        BanEntryImpl entry = entry(target);
        if (entry == null) {
            return false;
        }
        Date expiry = entry.getExpiration();
        return expiry == null || expiry.after(new Date());
    }

    @Override public synchronized void pardon(Object target) {
        pardon(target == null ? null : String.valueOf(target));
    }

    @Override public synchronized void pardon(String target) {
        if (target != null && entries.remove(key(target)) != null) {
            save();
        }
    }

    @Override public String toString() {
        return "BanList{" + type + ", entries=" + entries.size() + "}";
    }
}
