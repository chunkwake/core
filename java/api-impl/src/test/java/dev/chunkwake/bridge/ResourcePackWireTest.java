package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;

import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.util.UUID;
import org.junit.jupiter.api.Test;

/** `Player.setResourcePack`/`addResourcePack`: провод push/pop. */
final class ResourcePackWireTest {
    private final ChunkwakeServer server = new ChunkwakeServer(Path.of("world"));

    ResourcePackWireTest() {
        if (org.bukkit.Bukkit.getServer() == null) {
            org.bukkit.Bukkit.setServer(server.api);
        }
    }

    @Test
    void pushEncodesUrlHashAndPrompt() {
        UUID pack = UUID.fromString("0b8f6d4c-0000-4000-8000-000000000042");
        String prompt = "{\"text\":\"Need it\"}";
        String line = ResourcePackWire.push(
                pack,
                "https://example.test/p.zip",
                new byte[] {0x01, (byte) 0xab},
                prompt,
                true);
        assertEquals(
                "rpackpush " + pack
                        + " " + Hex.encode("https://example.test/p.zip".getBytes(StandardCharsets.UTF_8))
                        + " 01ab 1 "
                        + Hex.encode(prompt.getBytes(StandardCharsets.UTF_8)),
                line);
    }

    @Test
    void popAndPopAll() {
        UUID pack = UUID.fromString("0b8f6d4c-0000-4000-8000-000000000042");
        assertEquals("rpackpop " + pack, ResourcePackWire.pop(pack));
        assertEquals("rpackpopall", ResourcePackWire.popAll());
    }

    @Test
    void emptyHashAndPromptAreDashes() {
        UUID pack = UUID.fromString("0b8f6d4c-0000-4000-8000-000000000042");
        String url = "https://example.test/p.zip";
        assertEquals(
                "rpackpush " + pack + " " + Hex.encode(url.getBytes(StandardCharsets.UTF_8)) + " - 0 -",
                ResourcePackWire.push(pack, url, new byte[0], null, false));
    }

    @Test
    void freshPlayerHasNoResourcePack() {
        PlayerImpl player = new PlayerImpl(server, UUID.randomUUID(), "Steve");
        assertFalse(player.hasResourcePack());
        assertNull(player.getResourcePackHash());
        assertNull(player.getResourcePackStatus());
    }

    @Test
    void tracksLastResourcePackStatus() {
        PlayerImpl player = new PlayerImpl(server, UUID.randomUUID(), "Steve");
        player.resourcePackStatus(
                UUID.randomUUID(),
                org.bukkit.event.player.PlayerResourcePackStatusEvent.Status.ACCEPTED);
        assertEquals(
                org.bukkit.event.player.PlayerResourcePackStatusEvent.Status.ACCEPTED,
                player.getResourcePackStatus());
        player.resourcePackStatus(
                UUID.randomUUID(),
                org.bukkit.event.player.PlayerResourcePackStatusEvent.Status.SUCCESSFULLY_LOADED);
        assertEquals(
                org.bukkit.event.player.PlayerResourcePackStatusEvent.Status.SUCCESSFULLY_LOADED,
                player.getResourcePackStatus());
    }
}
