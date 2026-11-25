package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.util.Date;
import org.bukkit.BanList;
import org.junit.jupiter.api.Test;

/** BanList поверх banned-players.json/banned-ips.json. */
final class BanListTest {
    @Test
    @SuppressWarnings({"rawtypes", "unchecked"})
    void nameAndIpBansRoundTripThroughFiles() throws Exception {
        Path root = Files.createTempDirectory("chunkwake-bans");
        try {
            ChunkwakeServer server = new ChunkwakeServer(root.resolve("world"));
            BanList names = (BanList) server.getBanList(BanList.Type.NAME);
            assertNotNull(names);
            var entry = names.addBan("Tester", "griefing",
                    Date.from(Instant.now().plusSeconds(3600)), "console");
            assertNotNull(entry);
            assertEquals("griefing", entry.getReason());
            assertEquals("console", entry.getSource());
            assertTrue(names.isBanned("Tester"));
            assertTrue(server.getBannedPlayers().stream()
                    .anyMatch(p -> "Tester".equals(p.getName())));
            assertTrue(Files.readString(root.resolve("banned-players.json")).contains("Tester"));

            names.pardon("Tester");
            assertFalse(names.isBanned("Tester"));
            assertNull(names.getBanEntry("Tester"));

            BanList ips = (BanList) server.getBanList(BanList.Type.IP);
            ips.addBan("192.0.2.7", "proxy", Date.from(Instant.now().plusSeconds(60)), "console");
            assertTrue(ips.isBanned("192.0.2.7"));
            assertTrue(Files.readString(root.resolve("banned-ips.json")).contains("192.0.2.7"));
            ips.pardon("192.0.2.7");
            assertFalse(ips.isBanned("192.0.2.7"));

            // Просроченная запись остаётся в файле, но уже не банит.
            names.addBan("Expired", "old", Date.from(Instant.now().minusSeconds(60)), "console");
            assertFalse(names.isBanned("Expired"));
            names.pardon("Expired");
            assertEquals(BanList.Type.NAME, ((BanListImpl) names).type());
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
