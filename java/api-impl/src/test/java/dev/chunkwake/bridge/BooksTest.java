package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.StringReader;
import java.nio.file.Path;
import java.util.List;
import java.util.UUID;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;
import org.bukkit.Material;
import org.bukkit.event.EventPriority;
import org.bukkit.event.HandlerList;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerEditBookEvent;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.BookMeta;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.PluginDescriptionFile;
import org.bukkit.plugin.RegisteredListener;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;

/** Книги: {@code BookMeta} над компонентом книги и {@code PlayerEditBookEvent}. */
class BooksTest {
    private static final UUID ID = UUID.fromString("0b8f6d4c-0000-4000-8000-000000000003");
    private static final String WRITABLE = "minecraft:writable_book 1 {\"book\":{\"written\":false,\"pages\":[\"one\",\"two\"]}}";
    private static final String WRITTEN = "minecraft:written_book 1 {\"book\":{\"written\":true,\"pages\":[\"one\",\"two\"],"
            + "\"title\":\"T\",\"author\":\"Steve\",\"generation\":0,\"resolved\":true}}";

    private final ChunkwakeServer server = new ChunkwakeServer(Path.of("world"));
    private final Plugin plugin;
    private final PlayerImpl player = new PlayerImpl(server, ID, "Steve");

    BooksTest() throws Exception {
        if (org.bukkit.Bukkit.getServer() == null) {
            org.bukkit.Bukkit.setServer(server.api);
        }
        plugin = Stub.of(Plugin.class, new PaperCommandsTest.FakePlugin(
                new PluginDescriptionFile(new StringReader("name: T\nversion: 1\nmain: t.T\n")), true));
    }

    @AfterEach
    void unregister() {
        HandlerList.unregisterAll(plugin);
    }

    @Test
    void writableAndWrittenMeta() {
        ItemStack draft = StackImpl.parse(WRITABLE);
        BookMeta meta = assertInstanceOf(BookMeta.class, draft.getItemMeta());
        assertEquals(List.of("one", "two"), meta.getPages());
        assertFalse(meta.hasTitle());
        assertFalse(meta.setTitle("nope"));
        meta.addPage("three");
        meta.setPage(1, "uno");
        assertThrows(IllegalArgumentException.class, () -> meta.getPage(4));
        draft.setItemMeta(meta);
        assertTrue(StackImpl.line(draft).contains("[\"uno\",\"two\",\"three\"]"), StackImpl.line(draft));

        ItemStack book = StackImpl.parse(WRITTEN);
        BookMeta signed = (BookMeta) book.getItemMeta();
        assertEquals("T", signed.getTitle());
        assertEquals("Steve", signed.getAuthor());
        assertEquals(BookMeta.Generation.ORIGINAL, signed.getGeneration());
        assertEquals(Component.text("one"), signed.page(1));
        signed.page(2, Component.text("red", NamedTextColor.RED));
        assertEquals("§cred", signed.getPage(2));
        assertFalse(signed.setTitle("x".repeat(33)));
        BookMeta renamed = signed.title(Component.text("New"));
        assertEquals("New", renamed.getTitle());
        assertEquals("T", signed.getTitle());
    }

    @Test
    void editEventCanCancelRewriteAndUnsign() {
        int[] mode = {0};
        PlayerEditBookEvent.getHandlerList().register(new RegisteredListener(new Listener() {}, (l, ev) -> {
            if (!(ev instanceof PlayerEditBookEvent e)) return;
            assertEquals(3, e.getSlot());
            if (mode[0] == 1) e.setCancelled(true);
            if (mode[0] == 2) {
                BookMeta m = e.getNewBookMeta();
                m.setPage(1, "censored");
                e.setNewBookMeta(m);
                e.setSigning(false);
            }
        }, EventPriority.NORMAL, plugin, false));
        String arg = "3 1\n" + WRITABLE + "\n" + WRITTEN;
        String plain = Events.editBook(server.getPluginManager(), player.api, arg);
        assertTrue(plain.startsWith("s minecraft:written_book 1 "), plain);

        mode[0] = 1;
        assertEquals("c", Events.editBook(server.getPluginManager(), player.api, arg));

        mode[0] = 2;
        String out = Events.editBook(server.getPluginManager(), player.api, arg);
        assertTrue(out.startsWith("w minecraft:writable_book 1 "), out);
        assertTrue(out.contains("\"censored\""), out);
        assertEquals(Material.WRITABLE_BOOK, StackImpl.parse(out.substring(2)).getType());
    }
}
