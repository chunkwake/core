package dev.chunkwake.bridge;

import com.destroystokyo.paper.profile.ProfileProperty;
import io.papermc.paper.connection.PlayerLoginConnection;
import io.papermc.paper.event.connection.PlayerConnectionValidateLoginEvent;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ConcurrentHashMap;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.serializer.gson.GsonComponentSerializer;
import org.bukkit.event.player.AsyncPlayerPreLoginEvent;
import org.bukkit.event.player.PlayerLoginEvent;
import org.bukkit.event.player.PlayerPreLoginEvent;

/**
 * События входа в порядке Paper после авторизации:
 * <ol>
 * <li>{@code AsyncPlayerPreLoginEvent} (и устаревший {@code PlayerPreLoginEvent},
 * если его слушают) — вне главного потока, до проверок сервера;</li>
 * <li>проверки ванили {@code PlayerList.canPlayerLogin} (бан, белый список,
 * места) — в Rust;</li>
 * <li>{@code PlayerConnectionValidateLoginEvent} с причиной отказа ванили и
 * {@code PlayerLoginEvent} поверх него, как {@code HorriblePlayerLoginEventHack}.</li>
 * </ol>
 */
final class LoginEvents {
    private LoginEvents() {}

    /** Профиль после {@code AsyncPlayerPreLoginEvent} (плагин мог его заменить). */
    private static final Map<UUID, ProfileImpl> PROFILES = new ConcurrentHashMap<>();

    private static final java.util.concurrent.atomic.AtomicInteger AUTHENTICATORS =
            new java.util.concurrent.atomic.AtomicInteger();

    private static final GsonComponentSerializer GSON = GsonComponentSerializer.gson();

    /** Голова аргумента: {@code имя uuid ip порт}. */
    private record Head(String name, UUID id, InetSocketAddress address) {
        static Head parse(String line) {
            String[] f = line.split(" ");
            InetAddress ip;
            try {
                ip = InetAddress.getByName(f[2]);
            } catch (java.net.UnknownHostException e) {
                ip = InetAddress.getLoopbackAddress();
            }
            return new Head(f[0], UUID.fromString(f[1]), new InetSocketAddress(ip, Integer.parseInt(f[3])));
        }
    }

    /** Профиль из строк {@code имя значение [подпись]}, начиная с {@code from}. */
    private static ProfileImpl profile(Head h, String[] lines, int from) {
        ProfileImpl p = new ProfileImpl(h.id(), h.name());
        for (int i = from; i < lines.length; i++) {
            if (lines[i].isEmpty()) {
                continue;
            }
            String[] f = lines[i].split(" ", 3);
            p.setProperty(new ProfileProperty(f[0], f.length > 1 ? f[1] : "", f.length > 2 ? f[2] : null));
        }
        return p;
    }

    /**
     * {@code AsyncPlayerPreLoginEvent}: {@code имя uuid ip порт\nсвойства…} →
     * {@code k <json>} (отказ) или пусто. Зовётся из своего потока, не из потока плагинов.
     */
    @SuppressWarnings("deprecation")
    static String preLogin(ChunkwakeServer server, UUID id, String arg) {
        // Поток свой на каждый вход, как у `ServerLoginPacketListenerImpl`.
        Thread.currentThread().setName("User Authenticator #" + AUTHENTICATORS.incrementAndGet());
        String[] lines = arg.split("\n", -1);
        Head h = Head.parse(lines[0]);
        ProfileImpl profile = profile(h, lines, 1);
        var plugins = server.api.getPluginManager();
        var e = new AsyncPlayerPreLoginEvent(h.name(), h.address().getAddress(), h.address(), h.id(), false,
                profile, "", connection(profile, h.address()));
        plugins.callEvent(e);
        // `callPlayerPreLoginEvents`: устаревшее событие получает итог нового и может его сменить.
        if (PlayerPreLoginEvent.getHandlerList().getRegisteredListeners().length > 0) {
            var legacy = new PlayerPreLoginEvent(h.name(), h.address().getAddress(), h.id());
            if (e.getResult() != PlayerPreLoginEvent.Result.ALLOWED) {
                legacy.disallow(e.getResult(), e.kickMessage());
            }
            plugins.callEvent(legacy);
            if (legacy.getResult() != PlayerPreLoginEvent.Result.ALLOWED) {
                e.disallow(legacy.getResult(), legacy.kickMessage());
            } else if (e.getLoginResult() != AsyncPlayerPreLoginEvent.Result.ALLOWED) {
                e.allow();
            }
        }
        if (e.getLoginResult() != AsyncPlayerPreLoginEvent.Result.ALLOWED) {
            PROFILES.remove(h.id());
            return "k " + GSON.serialize(e.kickMessage());
        }
        if (e.getPlayerProfile() instanceof ProfileImpl p) {
            PROFILES.put(h.id(), p.clone());
        }
        return "";
    }

    /**
     * Проверка входа после ванили:
     * {@code имя uuid ip порт\n[ВИД <json>]\nсвойства…} → {@code a}, {@code k <json>}.
     * Игрок события уходит в {@code join}.
     */
    static String login(ChunkwakeServer server, UUID id, String arg) {
        String[] lines = arg.split("\n", -1);
        Head h = Head.parse(lines[0]);
        String vanilla = lines.length > 1 ? lines[1] : "";
        PlayerLoginEvent.Result kind = PlayerLoginEvent.Result.ALLOWED;
        Component denial = null;
        if (!vanilla.isEmpty()) {
            int sp = vanilla.indexOf(' ');
            kind = PlayerLoginEvent.Result.valueOf(vanilla.substring(0, sp));
            denial = GSON.deserialize(vanilla.substring(sp + 1));
        }
        ProfileImpl stored = PROFILES.remove(h.id());
        ProfileImpl profile = stored != null ? stored : profile(h, lines, 2);
        PlayerImpl p = new PlayerImpl(server, h.id(), h.name());
        p.online = false;
        p.profile = profile;
        p.address = h.address();

        var plugins = server.api.getPluginManager();
        var validate = new PlayerConnectionValidateLoginEvent(connection(profile, h.address()), denial);
        plugins.callEvent(validate);
        Component result = validate.isAllowed() ? null : validate.getKickMessage();
        if (result != null && kind == PlayerLoginEvent.Result.ALLOWED) {
            kind = PlayerLoginEvent.Result.KICK_OTHER;
        }
        // `HorriblePlayerLoginEventHack`: устаревшее событие только при слушателях.
        if (PlayerLoginEvent.getHandlerList().getRegisteredListeners().length > 0) {
            @SuppressWarnings("deprecation")
            var e = new PlayerLoginEvent(p.api, "", h.address().getAddress(), h.address().getAddress());
            if (result != null) {
                e.disallow(kind, result);
            }
            plugins.callEvent(e);
            result = e.getResult() == PlayerLoginEvent.Result.ALLOWED ? null : e.kickMessage();
        }
        if (result != null) {
            server.logins.remove(h.id());
            return "k " + GSON.serialize(result);
        }
        server.logins.put(h.id(), p);
        return "a";
    }

    /** Соединение входа ({@code PaperPlayerLoginConnection}): профиль и адрес. */
    private static PlayerLoginConnection connection(ProfileImpl profile, InetSocketAddress address) {
        return Stub.of(PlayerLoginConnection.class, new Object() {
            public com.destroystokyo.paper.profile.PlayerProfile getAuthenticatedProfile() { return profile; }
            public com.destroystokyo.paper.profile.PlayerProfile getUnsafeProfile() { return profile; }
            public java.net.SocketAddress getAddress() { return address; }
            public InetSocketAddress getClientAddress() { return address; }
            public InetSocketAddress getVirtualHost() { return null; }
            public InetSocketAddress getHAProxyAddress() { return null; }
            public boolean isConnected() { return true; }
            public boolean isTransferred() { return false; }
            public void disconnect(Component reason) {}
            public CompletableFuture<byte[]> retrieveCookie(org.bukkit.NamespacedKey key) {
                return CompletableFuture.completedFuture(null);
            }
        });
    }
}
