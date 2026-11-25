package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.UUID;
import org.junit.jupiter.api.Test;

/** whitelist.json/banned-players.json для OfflinePlayer. */
final class AccessFilesTest {
    @Test
    void whitelistRoundTripAndBanExpiry() throws Exception {
        Path root = Files.createTempDirectory("chunkwake-access");
        try {
            ChunkwakeServer server = new ChunkwakeServer(root.resolve("world"));
            UUID id = UUID.fromString("11111111-2222-3333-4444-555555555555");
            OfflinePlayerImpl player = new OfflinePlayerImpl(server, id, "Tester");

            assertFalse(player.isWhitelisted());
            player.setWhitelisted(true);
            assertTrue(player.isWhitelisted());
            assertTrue(Files.readString(root.resolve("whitelist.json")).contains(id.toString()));
            assertTrue(server.getWhitelistedPlayers().stream()
                    .anyMatch(p -> id.equals(p.getUniqueId())));
            player.setWhitelisted(false);
            assertFalse(player.isWhitelisted());

            Files.writeString(root.resolve("ops.json"),
                    "[{\"uuid\":\"" + id + "\",\"name\":\"Tester\",\"level\":4},"
                            + "{\"uuid\":\"33333333-4444-5555-6666-777777777777\","
                            + "\"name\":\"NoOp\",\"level\":0}]");
            assertEquals(1, server.getOperators().size());
            assertTrue(server.getOperators().iterator().next().getUniqueId().equals(id));

            UUID banned = UUID.fromString("22222222-3333-4444-5555-666666666666");
            Files.writeString(root.resolve("banned-players.json"),
                    "[{\"uuid\":\"" + banned + "\",\"name\":\"Banned\",\"expires\":\"forever\"}]");
            assertTrue(new OfflinePlayerImpl(server, banned, "Banned").isBanned());

            Files.writeString(root.resolve("banned-players.json"),
                    "[{\"uuid\":\"" + banned + "\",\"name\":\"Banned\","
                            + "\"expires\":\"2000-01-01T00:00:00Z\"}]");
            assertFalse(new OfflinePlayerImpl(server, banned, "Banned").isBanned());
        } finally {
            try (var paths = Files.walk(root)) {
                paths.sorted(java.util.Comparator.reverseOrder()).forEach(path -> {
                    try {
                        Files.deleteIfExists(path);
                    } catch (java.io.IOException ignored) {
                        // временный каталог
                    }
                });
            }
        }
    }
}
