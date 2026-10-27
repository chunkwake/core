package dev.chunkwake.bridge;

import java.io.File;
import java.io.IOException;
import java.util.Map;
import java.util.Set;
import java.util.regex.Pattern;
import org.bukkit.Bukkit;
import org.bukkit.event.Event;
import org.bukkit.event.Listener;
import org.bukkit.plugin.InvalidDescriptionException;
import org.bukkit.plugin.InvalidPluginException;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.PluginDescriptionFile;
import org.bukkit.plugin.PluginLoader;
import org.bukkit.plugin.RegisteredListener;

/**
 * {@link PluginLoader} для {@code JavaPlugin.getPluginLoader()} (ServiceLoader).
 * Плагины грузит {@link ChunkwakeServer}; здесь — только делегирование.
 */
public final class LegacyPluginLoader implements PluginLoader {
    @Override
    public Plugin loadPlugin(File file) throws InvalidPluginException {
        throw new InvalidPluginException("chunkwake: plugins are loaded from the plugins directory at startup");
    }

    @Override
    public PluginDescriptionFile getPluginDescription(File file) throws InvalidDescriptionException {
        try {
            return dev.chunkwake.bridge.PluginLoader.describe(file);
        } catch (IOException e) {
            throw new InvalidDescriptionException(e);
        }
    }

    @Override
    public Pattern[] getPluginFileFilters() {
        return new Pattern[] {Pattern.compile("\\.jar$")};
    }

    @Override
    public Map<Class<? extends Event>, Set<RegisteredListener>> createRegisteredListeners(Listener l, Plugin p) {
        throw new UnsupportedOperationException("chunkwake: use PluginManager.registerEvents");
    }

    @Override
    public void enablePlugin(Plugin plugin) {
        Bukkit.getPluginManager().enablePlugin(plugin);
    }

    @Override
    public void disablePlugin(Plugin plugin) {
        Bukkit.getPluginManager().disablePlugin(plugin);
    }
}
