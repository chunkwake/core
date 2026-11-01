package dev.chunkwake.bridge;

import io.papermc.paper.plugin.configuration.PluginMeta;
import io.papermc.paper.plugin.provider.classloader.ConfiguredPluginClassLoader;
import io.papermc.paper.plugin.provider.classloader.PluginClassLoaderGroup;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.net.URL;
import java.net.URLClassLoader;
import java.util.List;
import java.util.jar.JarFile;
import java.util.logging.Logger;
import org.bukkit.plugin.InvalidDescriptionException;
import org.bukkit.plugin.PluginDescriptionFile;
import org.bukkit.plugin.java.JavaPlugin;

/**
 * Загрузчик одного плагина. Paper требует, чтобы {@link JavaPlugin} создавался
 * загрузчиком {@link ConfiguredPluginClassLoader}: конструктор плагина вызывает
 * {@link #init(JavaPlugin)}, где мы передаём плагину сервер и описание.
 */
final class PluginLoader extends URLClassLoader implements ConfiguredPluginClassLoader {
    private final ChunkwakeServer server;
    final PluginDescriptionFile description;
    private final File file;
    private final File dataFolder;
    private JavaPlugin plugin;

    PluginLoader(ChunkwakeServer server, File file, PluginDescriptionFile description, ClassLoader parent)
            throws IOException {
        super(file.getName(), new URL[] {file.toURI().toURL()}, parent);
        this.server = server;
        this.file = file;
        this.description = description;
        this.dataFolder = new File(file.getParentFile(), description.getName());
    }

    /**
     * Добавляет в classpath плагина библиотеки из `libraries:` его `plugin.yml`.
     *
     * Paper делает это до создания класса плагина; у нас тоже до {@link #create()},
     * иначе main-класс, который их использует, не загрузится.
     */
    void addDeclaredLibraries(Logger log) {
        for (URL url : Libraries.resolve(file, description.getLibraries(), log)) {
            addURL(url);
        }
    }

    /** Сколько библиотек плагин объявил в `libraries:` — для журнала до загрузки. */
    int declaredLibraryCount() {
        List<String> declared = description.getLibraries();
        return declared == null ? 0 : declared.size();
    }

    /** `plugin.yml` (или `paper-plugin.yml`) из jar. */
    static PluginDescriptionFile describe(File jar) throws IOException, InvalidDescriptionException {
        try (JarFile jf = new JarFile(jar)) {
            var entry = jf.getEntry("plugin.yml");
            if (entry == null) {
                entry = jf.getEntry("paper-plugin.yml");
            }
            if (entry == null) {
                throw new InvalidDescriptionException("no plugin.yml");
            }
            try (InputStream in = jf.getInputStream(entry)) {
                return new PluginDescriptionFile(in);
            }
        }
    }

    JavaPlugin create() throws ReflectiveOperationException {
        Class<?> main = Class.forName(description.getMain(), true, this);
        return (JavaPlugin) main.getDeclaredConstructor().newInstance();
    }

    @Override
    public void init(JavaPlugin plugin) {
        this.plugin = plugin;
        Logger logger = Logger.getLogger(description.getName());
        plugin.init(server.api, description, dataFolder, file, this, description, logger);
    }

    @Override
    public JavaPlugin getPlugin() {
        return plugin;
    }

    @Override
    public PluginMeta getConfiguration() {
        return description;
    }

    @Override
    public Class<?> loadClass(String name, boolean resolve, boolean checkGlobal, boolean checkLibraries)
            throws ClassNotFoundException {
        return loadClass(name, resolve);
    }

    @Override
    public PluginClassLoaderGroup getGroup() {
        return null;
    }
}
