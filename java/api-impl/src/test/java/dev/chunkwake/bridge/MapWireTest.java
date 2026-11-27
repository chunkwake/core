package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.nio.file.Path;
import java.util.UUID;
import org.bukkit.map.MapView;
import org.junit.jupiter.api.Test;

/** `Player.sendMap`: запрос полного кадра карты для одного игрока. */
final class MapWireTest {
    private final ChunkwakeServer server = new ChunkwakeServer(Path.of("world"));

    MapWireTest() {
        if (org.bukkit.Bukkit.getServer() == null) {
            org.bukkit.Bukkit.setServer(server.api);
        }
    }

    @Test
    void buildsSendMapRequest() {
        MapView view = Stub.of(MapView.class, new Object() {
            public int getId() {
                return 42;
            }
        });
        assertEquals("sendmap 42", MapWire.command(view));
    }

    @Test
    void rejectsNullMapBeforeSending() {
        PlayerImpl player = new PlayerImpl(server, UUID.randomUUID(), "Steve");
        assertThrows(IllegalArgumentException.class, () -> player.sendMap(null));
    }
}
