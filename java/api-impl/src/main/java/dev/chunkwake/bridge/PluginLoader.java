package dev.chunkwake.bridge;

import io.papermc.paper.plugin.configuration.PluginMeta;
import io.papermc.paper.plugin.provider.classloader.ConfiguredPluginClassLoader;
import io.papermc.paper.plugin.provider.classloader.PluginClassLoaderGroup;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.net.URL;
import java.net.URLClassLoader;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
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
    static {
        // Загрузчики ищут классы друг у друга: с замком на весь загрузчик два
        // плагина, ссылающиеся друг на друга из разных потоков, встали бы.
        ClassLoader.registerAsParallelCapable();
    }

    private final ChunkwakeServer server;
    private final PluginClassGroup group;
    final PluginDescriptionFile description;
    private final File file;
    private final File dataFolder;
    private JavaPlugin plugin;
    /** `loader:` и `bootstrapper:` из `paper-plugin.yml`; `null` — их нет. */
    private final String paperLoader;
    private final String bootstrapper;
    private io.papermc.paper.plugin.bootstrap.PluginBootstrap bootstrap;

    PluginLoader(ChunkwakeServer server, File file, PluginDescriptionFile description, ClassLoader parent,
            PluginClassGroup group) throws IOException {
        super(file.getName(), new URL[] {file.toURI().toURL()}, parent);
        this.server = server;
        this.group = group;
        this.file = file;
        this.description = description;
        this.dataFolder = new File(file.getParentFile(), description.getName());
        Map<String, Object> paper = paperYaml(file);
        this.paperLoader = paper.get("loader") instanceof String s ? s : null;
        this.bootstrapper = paper.get("bootstrapper") instanceof String s ? s : null;
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

    /**
     * `loader:` из `paper-plugin.yml` (`io.papermc.paper.plugin.loader.PluginLoader`):
     * плагин сам добавляет библиотеки в classpath, обычно через
     * {@link io.papermc.paper.plugin.loader.library.impl.MavenLibraryResolver}.
     * Paper зовёт его до создания главного класса; без этого Veinminer падал на
     * `NoClassDefFoundError: de/miraculixx/kpaper/main/KPaper`.
     */
    void runPaperLoader(Logger log) throws ReflectiveOperationException {
        if (paperLoader == null) {
            return;
        }
        var loader = (io.papermc.paper.plugin.loader.PluginLoader)
                Class.forName(paperLoader, true, this).getDeclaredConstructor().newInstance();
        List<io.papermc.paper.plugin.loader.library.ClassPathLibrary> libraries = new ArrayList<>();
        var context = context();
        loader.classloader(new io.papermc.paper.plugin.loader.PluginClasspathBuilder() {
            @Override
            public io.papermc.paper.plugin.loader.PluginClasspathBuilder addLibrary(
                    io.papermc.paper.plugin.loader.library.ClassPathLibrary library) {
                libraries.add(library);
                return this;
            }

            @Override
            public io.papermc.paper.plugin.bootstrap.PluginProviderContext getContext() {
                return context;
            }
        });
        int before = getURLs().length;
        for (var library : libraries) {
            library.register(path -> {
                try {
                    addURL(path.toUri().toURL());
                } catch (IOException e) {
                    throw new io.papermc.paper.plugin.loader.library.LibraryLoadingException(
                            "library " + path + ": " + e, e);
                }
            });
        }
        int added = getURLs().length - before;
        log.fine(() -> "plugin " + file.getName() + ": " + added + " library jar(s) from " + paperLoader);
    }

    /**
     * `bootstrapper:` из `paper-plugin.yml`: `bootstrap` до создания плагина
     * (регистрация команд и реестров через менеджер жизненного цикла), затем
     * `createPlugin` создаёт главный класс.
     */
    void runBootstrapper() throws ReflectiveOperationException {
        if (bootstrapper == null) {
            return;
        }
        bootstrap = (io.papermc.paper.plugin.bootstrap.PluginBootstrap)
                Class.forName(bootstrapper, true, this).getDeclaredConstructor().newInstance();
        bootstrap.bootstrap(context());
    }

    private io.papermc.paper.plugin.bootstrap.BootstrapContext context() {
        String name = description.getName();
        var lifecycle = Lifecycle.<io.papermc.paper.plugin.bootstrap.BootstrapContext>manager(name);
        return new io.papermc.paper.plugin.bootstrap.BootstrapContext() {
            @Override
            public PluginMeta getConfiguration() {
                return description;
            }

            @Override
            public PluginMeta getPluginMeta() {
                return description;
            }

            @Override
            public Path getDataDirectory() {
                return dataFolder.toPath();
            }

            @Override
            public net.kyori.adventure.text.logger.slf4j.ComponentLogger getLogger() {
                return net.kyori.adventure.text.logger.slf4j.ComponentLogger.logger(name);
            }

            @Override
            public Path getPluginSource() {
                return file.toPath();
            }

            @Override
            public io.papermc.paper.plugin.lifecycle.event.LifecycleEventManager<
                            io.papermc.paper.plugin.bootstrap.BootstrapContext>
                    getLifecycleManager() {
                return lifecycle;
            }
        };
    }

    /** Ключи `paper-plugin.yml`, которых нет в {@link PluginDescriptionFile}. */
    @SuppressWarnings("unchecked")
    private static Map<String, Object> paperYaml(File jar) throws IOException {
        try (JarFile jf = new JarFile(jar)) {
            var entry = jf.getEntry("paper-plugin.yml");
            if (entry == null) {
                return Map.of();
            }
            try (InputStream in = jf.getInputStream(entry)) {
                Object root = new org.yaml.snakeyaml.Yaml().load(in);
                return root instanceof Map<?, ?> m ? (Map<String, Object>) m : Map.of();
            }
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
        if (bootstrap != null) {
            return bootstrap.createPlugin(context());
        }
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
    protected Class<?> loadClass(String name, boolean resolve) throws ClassNotFoundException {
        return loadClass(name, resolve, true, true);
    }

    /**
     * Родитель (API, Via), затем свой jar и библиотеки, затем — при
     * {@code checkGlobal} — jar других плагинов ({@link PluginClassGroup}).
     * Библиотеки лежат в том же classpath, что и jar, поэтому
     * {@code checkLibraries} их не отделяет.
     */
    @Override
    public Class<?> loadClass(String name, boolean resolve, boolean checkGlobal, boolean checkLibraries)
            throws ClassNotFoundException {
        synchronized (getClassLoadingLock(name)) {
            Class<?> c = findLoadedClass(name);
            if (c == null) {
                try {
                    c = getParent().loadClass(name);
                } catch (ClassNotFoundException ignored) {
                    // Нет у родителя — ищем у себя.
                }
            }
            if (c == null) {
                try {
                    c = findClass(name);
                } catch (ClassNotFoundException ignored) {
                    // Нет в своём jar — ищем у других плагинов.
                }
            }
            if (c == null && checkGlobal && group != null) {
                c = group.getClassByName(name, resolve, this);
            }
            if (c == null) {
                throw new ClassNotFoundException(name);
            }
            if (resolve) {
                resolveClass(c);
            }
            return c;
        }
    }

    @Override
    public PluginClassLoaderGroup getGroup() {
        return group;
    }

    @Override
    public void close() throws IOException {
        if (group != null) {
            group.remove(this);
        }
        super.close();
    }
}
