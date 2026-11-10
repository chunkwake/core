package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import io.papermc.paper.event.connection.PlayerConnectionValidateLoginEvent;
import java.io.StringReader;
import java.util.UUID;
import java.util.function.Consumer;
import net.kyori.adventure.text.Component;
import org.bukkit.event.Event;
import org.bukkit.event.EventPriority;
import org.bukkit.event.HandlerList;
import org.bukkit.event.Listener;
import org.bukkit.event.player.AsyncPlayerPreLoginEvent;
import org.bukkit.event.player.PlayerKickEvent;
import org.bukkit.event.player.PlayerLoginEvent;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.PluginDescriptionFile;
import org.bukkit.plugin.RegisteredListener;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;

/** Вход и кик в порядке Paper: аргументы из Rust → события → ответ. */
class LoginEventsTest {
    private static final UUID ID = UUID.fromString("0b8f6d4c-0000-4000-8000-000000000001");
    private static final String HEAD = "Steve " + ID + " 127.0.0.1 51234";

    private final ChunkwakeServer server = new ChunkwakeServer(java.nio.file.Path.of("."));
    private final Plugin plugin;

    LoginEventsTest() throws Exception {
        plugin = Stub.of(Plugin.class, new PaperCommandsTest.FakePlugin(
                new PluginDescriptionFile(new StringReader("name: T\nversion: 1\nmain: t.T\n")), true));
    }

    @AfterEach
    void unregister() {
        HandlerList.unregisterAll(plugin);
    }

    private <E extends Event> void on(Class<E> type, HandlerList handlers, Consumer<E> body) {
        handlers.register(new RegisteredListener(new Listener() {}, (l, e) -> {
            if (type.isInstance(e)) {
                body.accept(type.cast(e));
            }
        }, EventPriority.NORMAL, plugin, false));
    }

    @Test
    void preLoginSeesProfileAndCanDeny() {
        String[] seen = new String[1];
        on(AsyncPlayerPreLoginEvent.class, AsyncPlayerPreLoginEvent.getHandlerList(), e -> {
            seen[0] = e.getName() + " " + e.getAddress().getHostAddress() + " "
                    + e.getPlayerProfile().getProperties().iterator().next().getValue()
                    + " async=" + e.isAsynchronous();
            e.disallow(AsyncPlayerPreLoginEvent.Result.KICK_BANNED, Component.text("nope"));
        });
        String reply = LoginEvents.preLogin(server, ID, HEAD + "\ntextures abc sig");
        assertEquals("Steve 127.0.0.1 abc async=true", seen[0]);
        assertEquals("k \"nope\"", reply);
    }

    @Test
    void validateCanOverrideVanillaDenial() {
        Component[] vanilla = new Component[1];
        on(PlayerConnectionValidateLoginEvent.class, PlayerConnectionValidateLoginEvent.getHandlerList(), e -> {
            vanilla[0] = e.getKickMessage();
            e.allow();
        });
        String reply = LoginEvents.login(server, ID,
                HEAD + "\nKICK_WHITELIST {\"text\":\"You are not white-listed on this server!\"}\ntextures abc");
        assertEquals("a", reply);
        assertEquals(Component.text("You are not white-listed on this server!"), vanilla[0]);
        PlayerImpl p = server.logins.get(ID);
        assertNotNull(p);
        assertFalse(p.online);
        assertEquals(51234, p.getAddress().getPort());
        assertTrue(p.getPlayerProfile().hasProperty("textures"));
    }

    @Test
    @SuppressWarnings("deprecation")
    void legacyLoginEventSeesResultAndCanKick() {
        PlayerLoginEvent.Result[] seen = new PlayerLoginEvent.Result[1];
        on(PlayerLoginEvent.class, PlayerLoginEvent.getHandlerList(), e -> {
            seen[0] = e.getResult();
            assertEquals("Steve", e.getPlayer().getName());
            e.disallow(PlayerLoginEvent.Result.KICK_OTHER, Component.text("later"));
        });
        assertEquals("k \"later\"", LoginEvents.login(server, ID, HEAD + "\n"));
        assertEquals(PlayerLoginEvent.Result.ALLOWED, seen[0]);
        assertNull(server.logins.get(ID));
    }

    @Test
    void kickEventChangesReasonOrCancels() {
        PlayerImpl p = new PlayerImpl(server, ID, "Steve");
        on(PlayerKickEvent.class, PlayerKickEvent.getHandlerList(), e -> {
            assertEquals(PlayerKickEvent.Cause.KICKED, e.getCause());
            e.reason(Component.text("bye"));
        });
        assertEquals("\"bye\"",
                p.kickEvent("KICKED\n{\"translate\":\"multiplayer.disconnect.kicked\"}"));
        assertTrue(p.kicked);

        HandlerList.unregisterAll(plugin);
        PlayerImpl q = new PlayerImpl(server, ID, "Steve");
        on(PlayerKickEvent.class, PlayerKickEvent.getHandlerList(), e -> e.setCancelled(true));
        assertEquals("c", q.kickEvent("BANNED\n{\"text\":\"x\"}"));
        assertFalse(q.kicked);
    }
}
