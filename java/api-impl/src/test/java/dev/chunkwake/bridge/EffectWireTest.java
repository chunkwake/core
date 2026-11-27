package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.nio.file.Path;
import java.util.UUID;
import org.bukkit.Effect;
import org.bukkit.Location;
import org.junit.jupiter.api.Test;

/** `Player.playEffect`: строка для Rust-пакета `level_event`. */
final class EffectWireTest {
    private final ChunkwakeServer server = new ChunkwakeServer(Path.of("world"));

    EffectWireTest() {
        if (org.bukkit.Bukkit.getServer() == null) {
            org.bukkit.Bukkit.setServer(server.api);
        }
    }

    @Test
    void buildsLevelEventRequest() {
        Effect effect = Effect.valueOf("DOOR_TOGGLE");
        Location at = new Location(null, 1.5, 64.0, -2.5);
        assertEquals(
                "effect " + effect.getId() + " 1.5 64.0 -2.5 7",
                EffectWire.command(at, effect, 7));
    }

    @Test
    void rejectsBadArgumentsBeforeSending() {
        PlayerImpl player = new PlayerImpl(server, UUID.randomUUID(), "Steve");
        Effect effect = Effect.valueOf("DOOR_TOGGLE");
        Location at = new Location(null, 0.0, 64.0, 0.0);
        assertThrows(IllegalArgumentException.class, () -> player.playEffect(at, null, 1));
        assertThrows(IllegalArgumentException.class, () -> player.playEffect(null, effect, 1));
        assertThrows(IllegalArgumentException.class,
                () -> player.playEffect(at, effect, null));
        assertThrows(IllegalArgumentException.class,
                () -> player.playEffect(at, effect, "not-an-int"));
    }
}
