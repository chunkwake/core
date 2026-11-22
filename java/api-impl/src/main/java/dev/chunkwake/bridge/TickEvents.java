package dev.chunkwake.bridge;

import com.destroystokyo.paper.event.server.ServerTickEndEvent;
import com.destroystokyo.paper.event.server.ServerTickStartEvent;
import org.bukkit.plugin.PluginManager;

/** Paper {@code ServerTickStartEvent}/{@code ServerTickEndEvent} поверх тика Rust. */
final class TickEvents {
    private static int tick;

    private TickEvents() {
    }

    /** Начало Java-части тика; возвращает номер, который нужно передать в {@link #end}. */
    static int start(PluginManager plugins) {
        tick++;
        if (ServerTickStartEvent.getHandlerList().getRegisteredListeners().length != 0) {
            plugins.callEvent(new ServerTickStartEvent(tick));
        }
        return tick;
    }

    static void end(PluginManager plugins, int tickNumber, double durationMs) {
        if (ServerTickEndEvent.getHandlerList().getRegisteredListeners().length == 0) {
            return;
        }
        // Как в Paper: остаток до следующего тика, отрицательный при переработке.
        long remaining = (long) (50_000_000.0 - durationMs * 1_000_000.0);
        plugins.callEvent(new ServerTickEndEvent(tickNumber, durationMs, remaining));
    }
}
