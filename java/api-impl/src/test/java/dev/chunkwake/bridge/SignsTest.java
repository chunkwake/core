package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertTrue;

import io.papermc.paper.event.packet.UncheckedSignChangeEvent;
import java.io.StringReader;
import java.nio.file.Path;
import java.util.UUID;
import java.util.function.Consumer;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;
import org.bukkit.DyeColor;
import org.bukkit.block.Block;
import org.bukkit.block.HangingSign;
import org.bukkit.block.Sign;
import org.bukkit.block.sign.Side;
import org.bukkit.event.Event;
import org.bukkit.event.EventPriority;
import org.bukkit.event.HandlerList;
import org.bukkit.event.Listener;
import org.bukkit.event.block.SignChangeEvent;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.PluginDescriptionFile;
import org.bukkit.plugin.RegisteredListener;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;

/** Таблички: события правки как в Paper и снимок {@code Sign} из ответа Rust. */
class SignsTest {
    private static final UUID ID = UUID.fromString("0b8f6d4c-0000-4000-8000-000000000002");
    private static final String LINES = "1 64 2 FRONT\n\"Hello\"\n\"\"\n{\"text\":\"red\",\"color\":\"red\"}\n\"\"";

    private final ChunkwakeServer server = new ChunkwakeServer(Path.of("world"));
    private final Plugin plugin;
    private final PlayerImpl player = new PlayerImpl(server, ID, "Steve");

    SignsTest() throws Exception {
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
    void signChangeSeesLinesAndCanRewriteOrCancel() {
        String[] seen = new String[1];
        int[] mode = {0};
        on(SignChangeEvent.class, SignChangeEvent.getHandlerList(), e -> {
            seen[0] = e.getSide() + " " + e.getBlock().getX() + " " + e.getLine(0) + " " + e.line(2);
            if (mode[0] == 0) e.line(1, Component.text("[ok]", NamedTextColor.GREEN));
            if (mode[0] == 1) e.setCancelled(true);
        });
        String reply = Events.signChange(server.getPluginManager(), player.api, server.overworld.api, LINES);
        assertTrue(seen[0].startsWith("FRONT 1 Hello "), seen[0]);
        String[] out = reply.split("\n");
        assertEquals(4, out.length);
        assertEquals("\"Hello\"", out[0]);
        assertEquals("{\"color\":\"green\",\"text\":\"[ok]\"}", out[1]);

        mode[0] = 1;
        assertEquals("c", Events.signChange(server.getPluginManager(), player.api, server.overworld.api, LINES));

        mode[0] = 2;
        assertEquals("", Events.signChange(server.getPluginManager(), player.api, server.overworld.api, LINES));
    }

    @Test
    void uncheckedEventGetsRawText() {
        String[] seen = new String[1];
        on(UncheckedSignChangeEvent.class, UncheckedSignChangeEvent.getHandlerList(), e -> {
            seen[0] = e.getEditedBlockPosition().blockX() + " " + e.getSide() + " " + e.lines().get(0);
            e.setCancelled(true);
        });
        String reply = Events.signUnchecked(server.getPluginManager(), player, "5 70 -3 BACK\n\"a {b}\"\n\"\"\n\"\"\n\"\"");
        assertEquals("c", reply);
        assertEquals("5 BACK " + Component.text("a {b}"), seen[0]);
    }

    @Test
    void signSnapshotFromJson() {
        Block block = new BlockImpl(server.overworld.api, 1, 64, 2, "minecraft:oak_sign").api;
        String json = "{\"hanging\":false,\"waxed\":true,\"editor\":null,"
                + "\"front\":{\"color\":\"light_blue\",\"glowing\":true,\"lines\":[\"Hi\",\"\",{\"text\":\"x\",\"bold\":true},\"\"]},"
                + "\"back\":{\"color\":\"black\",\"glowing\":false,\"lines\":[\"\",\"\",\"\",\"\"]}}";
        Sign sign = SignImpl.of(block, json);
        assertEquals("Hi", sign.getLine(0));
        assertEquals(DyeColor.LIGHT_BLUE, sign.getColor());
        assertTrue(sign.isGlowingText());
        assertTrue(sign.isWaxed());
        assertEquals("§lx", sign.getSide(Side.FRONT).getLine(2));
        sign.getSide(Side.BACK).setLine(0, "§cback");
        assertEquals(Component.text("back", NamedTextColor.RED), sign.getSide(Side.BACK).line(0));
        assertInstanceOf(HangingSign.class, SignImpl.of(block, json.replace("\"hanging\":false", "\"hanging\":true")));
    }
}
