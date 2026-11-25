package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.StringReader;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import org.bukkit.NamespacedKey;
import org.bukkit.World;
import org.bukkit.event.EventHandler;
import org.bukkit.event.HandlerList;
import org.bukkit.event.Listener;
import org.bukkit.event.world.WorldLoadEvent;
import org.bukkit.event.world.WorldUnloadEvent;
import org.bukkit.metadata.FixedMetadataValue;
import org.bukkit.persistence.PersistentDataType;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.PluginDescriptionFile;
import org.junit.jupiter.api.Test;

/** Миры по измерениям, Metadatable и PDC с записью при изменении — как в Paper 26.3. */
class WorldsTest {
    private final ChunkwakeServer server = new ChunkwakeServer(Path.of("world"));
    private final Plugin plugin;

    WorldsTest() throws Exception {
        plugin = Stub.of(Plugin.class, new PaperCommandsTest.FakePlugin(
                new PluginDescriptionFile(new StringReader("name: T\nversion: 1\nmain: t.T\n")), true));
    }

    @Test
    void namesKeysAndPathsFollowPaper() {
        WorldImpl over = new WorldImpl(server, WorldImpl.OVERWORLD);
        WorldImpl nether = new WorldImpl(server, WorldImpl.NETHER);
        WorldImpl end = new WorldImpl(server, WorldImpl.END);
        assertEquals("world", over.api.getName());
        assertEquals("world_nether", nether.api.getName());
        assertEquals("world_the_end", end.api.getName());
        assertEquals(World.Environment.NETHER, nether.api.getEnvironment());
        assertEquals(NamespacedKey.minecraft("the_end"), end.api.getKey());
        assertEquals(Path.of("world", "dimensions", "minecraft", "the_nether"), nether.getWorldPath());
        assertEquals(-64, over.api.getMinHeight());
        assertEquals(0, end.api.getMinHeight());
    }

    @Test
    void metadataIsPerWorldAndClearedOnDisable() {
        World w = server.overworld.api;
        w.setMetadata("k", new FixedMetadataValue(plugin, 7));
        assertTrue(w.hasMetadata("k"));
        assertEquals(7, w.getMetadata("k").get(0).asInt());
        server.metadata().removeAll(plugin);
        assertFalse(w.hasMetadata("k"));
    }

    @Test
    void pdcReportsEveryChange() {
        Map<String, Object>[] last = new Map[1];
        PdcImpl pdc = new PdcImpl(new LinkedHashMap<>(), m -> last[0] = m);
        NamespacedKey key = new NamespacedKey("myplugin", "coins");
        pdc.api.set(key, PersistentDataType.INTEGER, 42);
        assertEquals(1, last[0].size());
        assertEquals(42, pdc.api.get(key, PersistentDataType.INTEGER));
        pdc.api.remove(key);
        assertTrue(last[0].isEmpty());
    }

    /** `WorldLoadEvent` после создания и отменяемый `WorldUnloadEvent` до выгрузки. */
    @Test
    void worldLifecycleEventsFollowPaper() {
        WorldImpl dynamic = new WorldImpl(server, "minecraft:eventworld");
        List<String> seen = new ArrayList<>();
        Listener listener = new Listener() {
            @EventHandler
            public void onLoad(WorldLoadEvent e) {
                seen.add("load:" + e.getWorld().getName());
            }

            @EventHandler
            public void onUnload(WorldUnloadEvent e) {
                seen.add("unload:" + e.getWorld().getName());
                e.setCancelled(true);
            }
        };
        server.plugins.registerEvents(listener, plugin);
        try {
            server.fireWorldLoad(dynamic);
            assertEquals(List.of("load:eventworld"), seen);

            // Отмена оставляет мир загруженным: Rust unloadworld не вызывается.
            assertFalse(server.unloadWorld(dynamic.api, true));
            assertEquals(List.of("load:eventworld", "unload:eventworld"), seen);
            assertFalse(dynamic.isUnloaded());

            // Ванильные измерения не выгружаются и событие не получают.
            assertFalse(server.unloadWorld(server.world, true));
            assertEquals(2, seen.size());
        } finally {
            HandlerList.unregisterAll(plugin);
        }
    }
}
