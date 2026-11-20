package dev.chunkwake.bridge;

import java.util.concurrent.ConcurrentLinkedQueue;
import org.bukkit.event.world.ChunkLoadEvent;
import org.bukkit.plugin.PluginManager;

/**
 * {@link ChunkLoadEvent} из Rust.
 *
 * <p>Rust присылает координаты загруженного чанка в event-канал (kind 52);
 * здесь они складываются в очередь и вызываются на тике сервера, то есть
 * в главном потоке, как в Paper. Так squaremap и другие плагины получают
 * событие загрузки чанка и могут отрисовать/обновить карту.
 */
final class ChunkEvents {
    private record Load(WorldImpl world, int x, int z, boolean isNew) {}

    private static final ConcurrentLinkedQueue<Load> QUEUE = new ConcurrentLinkedQueue<>();
    /** Уже отправленные (мир, x, z): событие — один раз на загрузку. */
    private static final java.util.Set<String> SEEN =
            java.util.concurrent.ConcurrentHashMap.newKeySet();

    private ChunkEvents() {}

    static void queue(WorldImpl world, int x, int z, boolean isNew) {
        if (world != null && SEEN.add(world.dim + ":" + x + ":" + z)) {
            QUEUE.add(new Load(world, x, z, isNew));
        }
    }

    /** Вызывается из {@link Bridge#tick()} (главный поток сервера). */
    static void tick(PluginManager plugins) {
        if (ChunkLoadEvent.getHandlerList().getRegisteredListeners().length == 0) {
            QUEUE.clear();
            return;
        }
        int fired = 0;
        Load load;
        while (fired < 512 && (load = QUEUE.poll()) != null) {
            plugins.callEvent(new ChunkLoadEvent(
                    ChunkImpl.api(load.world, load.x, load.z), load.isNew));
            fired++;
        }
    }
}
