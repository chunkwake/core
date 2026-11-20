package io.papermc.paper.network;

import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import net.kyori.adventure.key.Key;

/** Paper API: реестр слушателей каналов (Emotecraft). */
public final class ChannelInitializeListenerHolder {
    private static final Map<Key, ChannelInitializeListener> LISTENERS =
            new ConcurrentHashMap<>();

    private ChannelInitializeListenerHolder() {
    }

    public static void addListener(Key key, ChannelInitializeListener listener) {
        LISTENERS.put(key, listener);
    }

    public static void removeListener(Key key) {
        LISTENERS.remove(key);
    }

    public static Map<Key, ChannelInitializeListener> listeners() {
        return Map.copyOf(LISTENERS);
    }
}
