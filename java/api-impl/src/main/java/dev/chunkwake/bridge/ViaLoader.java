package dev.chunkwake.bridge;

import java.io.File;
import java.net.URISyntaxException;
import java.net.URL;
import java.net.URLClassLoader;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.logging.Level;
import java.util.logging.Logger;
import org.bukkit.Server;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.PluginDescriptionFile;

/**
 * ViaVersion, ViaBackwards и ViaRewind из {@code plugins/} — как библиотека.
 *
 * <p>Их Bukkit-платформа внедряется в Netty Java-сервера
 * ({@code MinecraftServer.getServer()}, каналы игроков), а сеть у нас на Rust.
 * Поэтому jar-ы не включаются как плагины: мост грузит их одним загрузчиком
 * вместе с {@code via/chunkwake-via.jar} (своя платформа Via поверх
 * {@code UserConnectionViaVersionPlatform}) и переводит через неё пакеты
 * клиентов других версий.
 *
 * <p>Для остальных плагинов Via выглядит как обычно: в {@code PluginManager}
 * стоят плагины с теми же именем и версией, {@code /viaversion} работает, а
 * классы Via (API {@code Via.getAPI()}) видны из загрузчиков плагинов —
 * загрузчик Via стоит у них родителем.
 */
final class ViaLoader {
    /** Порядок важен: ViaBackwards и ViaRewind включаются поверх ViaVersion. */
    static final List<String> NAMES = List.of("ViaVersion", "ViaBackwards", "ViaRewind");

    private final Map<String, File> jars = new LinkedHashMap<>();
    private final Map<String, PluginDescriptionFile> descriptions = new LinkedHashMap<>();
    private URLClassLoader loader;
    ViaHost host;

    /** Отбирает jar-ы Via из списка; остальные возвращает. */
    List<File> claim(File[] all, Logger logger) {
        List<File> rest = new ArrayList<>();
        for (File jar : all) {
            String name = null;
            try {
                PluginDescriptionFile desc = PluginLoader.describe(jar);
                if (NAMES.contains(desc.getName())) {
                    name = desc.getName();
                    if (jars.containsKey(name)) {
                        logger.warning("Duplicate plugin " + name + " in " + jar.getName() + ", keeping "
                                + jars.get(name).getName());
                        continue;
                    }
                    jars.put(name, jar);
                    descriptions.put(name, desc);
                }
            } catch (Exception e) {
                // Не плагин или битый plugin.yml: пусть разбирается обычная загрузка.
            }
            if (name == null) {
                rest.add(jar);
            }
        }
        return rest;
    }

    /** Загрузчик классов Via для плагинов; без Via — {@code fallback}. */
    ClassLoader parent(ClassLoader fallback) {
        return loader != null ? loader : fallback;
    }

    /**
     * Поднимает Via. Без ViaVersion (только ViaBackwards) — предупреждение:
     * они без неё не работают, как и на Paper.
     */
    void start(File pluginsDir, ChunkwakeServer server, Logger logger) {
        if (jars.isEmpty()) {
            return;
        }
        if (!jars.containsKey("ViaVersion")) {
            logger.warning("ViaVersion: " + String.join(", ", jars.keySet())
                    + " need ViaVersion in plugins/; skipped");
            return;
        }
        File impl = implJar();
        if (impl == null) {
            logger.warning("ViaVersion: via/chunkwake-via.jar is missing next to the plugin API; "
                    + "clients of other versions are not translated");
            return;
        }
        long started = System.nanoTime();
        try {
            List<URL> urls = new ArrayList<>();
            for (String name : NAMES) {
                File jar = jars.get(name);
                if (jar != null) {
                    urls.add(jar.toURI().toURL());
                }
            }
            urls.add(impl.toURI().toURL());
            loader = new URLClassLoader("ViaVersion", urls.toArray(URL[]::new), getClass().getClassLoader());
            String version = descriptions.get("ViaVersion").getVersion();
            host = (ViaHost) Class.forName("dev.chunkwake.via.ChunkwakeVia", true, loader)
                    .getConstructor(File.class, String.class, boolean.class, boolean.class)
                    .newInstance(pluginsDir, version, jars.containsKey("ViaBackwards"),
                            jars.containsKey("ViaRewind"));
            Bridge.installVia(host);
            for (var e : descriptions.entrySet()) {
                server.plugins.plugins.put(e.getKey(), Stub.of(Plugin.class,
                        new Shadow(e.getValue(), new File(pluginsDir, e.getKey()), server)));
            }
            logger.info("plugins: " + String.join(", ", jars.keySet()) + " loaded as a library in "
                    + (System.nanoTime() - started) / 1_000_000 + " ms");
        } catch (Throwable t) {
            host = null;
            logger.log(Level.SEVERE, "ViaVersion: startup failed; clients of other versions are not translated",
                    t instanceof java.lang.reflect.InvocationTargetException ite && ite.getCause() != null
                            ? ite.getCause() : t);
        }
    }

    /** {@code /viaversion} с алиасами Bukkit-плагина. */
    Command command() {
        ViaHost via = host;
        return new Command("viaversion", "ViaVersion", "/viaversion help",
                List.of("viaver", "vvbukkit")) {
            @Override
            public boolean execute(CommandSender sender, String label, String[] args) {
                return via.command(sender, args);
            }

            @Override
            public List<String> tabComplete(CommandSender sender, String alias, String[] args) {
                return via.complete(sender, args);
            }
        };
    }

    void stop(Logger logger) {
        if (host == null) {
            return;
        }
        try {
            host.disable();
        } catch (Throwable t) {
            logger.log(Level.WARNING, "ViaVersion: shutdown failed", t);
        }
    }

    /** {@code via/chunkwake-via.jar} рядом с {@code chunkwake-api.jar}. */
    private File implJar() {
        try {
            Path api = Path.of(Bridge.class.getProtectionDomain().getCodeSource().getLocation().toURI());
            File jar = api.resolveSibling("via").resolve("chunkwake-via.jar").toFile();
            return jar.isFile() ? jar : null;
        } catch (URISyntaxException | RuntimeException e) {
            return null;
        }
    }

    /** Плагин Via в {@code PluginManager}: имя, версия, каталог данных — без кода. */
    static final class Shadow {
        private final PluginDescriptionFile description;
        private final File dataFolder;
        private final ChunkwakeServer server;

        Shadow(PluginDescriptionFile description, File dataFolder, ChunkwakeServer server) {
            this.description = description;
            this.dataFolder = dataFolder;
            this.server = server;
        }

        public String getName() {
            return description.getName();
        }

        public PluginDescriptionFile getDescription() {
            return description;
        }

        public io.papermc.paper.plugin.configuration.PluginMeta getPluginMeta() {
            return description;
        }

        public File getDataFolder() {
            return dataFolder;
        }

        public boolean isEnabled() {
            return true;
        }

        public Server getServer() {
            return server.api;
        }

        public Logger getLogger() {
            return Logger.getLogger(description.getName());
        }

        @Override
        public String toString() {
            return description.getFullName();
        }
    }
}
