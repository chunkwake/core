package dev.chunkwake.bridge;

import io.papermc.paper.plugin.configuration.PluginMeta;
import io.papermc.paper.plugin.provider.classloader.ClassLoaderAccess;
import io.papermc.paper.plugin.provider.classloader.ConfiguredPluginClassLoader;
import io.papermc.paper.plugin.provider.classloader.PluginClassLoaderGroup;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.logging.Logger;

/**
 * Общие классы плагинов, как глобальная группа Paper для плагинов с
 * `plugin.yml`: класс, которого нет ни у родителя, ни в jar самого плагина,
 * ищется в jar остальных плагинов. Без этого плагин с `depend: [Vault]` падал
 * на `NoClassDefFoundError: net/milkbowl/vault/...`.
 *
 * <p>Сначала проверяются плагины из `depend`/`softdepend`, потом все прочие.
 * Как и Paper, доступ к плагину не из зависимостей разрешён, но записывается в
 * журнал один раз на пару плагинов: так авторы видят забытый `depend`.
 */
final class PluginClassGroup implements PluginClassLoaderGroup {
    private final Logger logger;
    private final List<ConfiguredPluginClassLoader> loaders = new CopyOnWriteArrayList<>();
    private final Set<String> warned = ConcurrentHashMap.newKeySet();

    PluginClassGroup(Logger logger) {
        this.logger = logger;
    }

    @Override
    public Class<?> getClassByName(String name, boolean resolve, ConfiguredPluginClassLoader requester) {
        PluginMeta meta = requester.getConfiguration();
        List<ConfiguredPluginClassLoader> ordered = new ArrayList<>(loaders.size());
        // Paper-like resolution: declared depend/softdepend order decides which
        // provider wins when two plugins expose the same class (WorldEdit/FAWE).
        for (String dependency : meta.getPluginDependencies()) {
            addMatching(dependency, ordered, requester);
        }
        for (String dependency : meta.getPluginSoftDependencies()) {
            addMatching(dependency, ordered, requester);
        }
        for (ConfiguredPluginClassLoader loader : loaders) {
            if (loader != requester && !ordered.contains(loader)) {
                ordered.add(loader);
            }
        }
        for (ConfiguredPluginClassLoader loader : ordered) {
            Class<?> c = tryLoad(loader, name, resolve);
            if (c != null) {
                if (!dependsOn(meta, loader.getConfiguration())) {
                    String key = meta.getName() + "->" + loader.getConfiguration().getName();
                    if (warned.add(key)) {
                        logger.warning("Loaded class " + name + " from " + loader.getConfiguration().getDisplayName()
                                + " which is not a depend or softdepend of " + meta.getDisplayName()
                                + " (this may cause issues if it is enabled later)");
                    }
                }
                return c;
            }
        }
        return null;
    }

    private void addMatching(String dependency, List<ConfiguredPluginClassLoader> ordered,
            ConfiguredPluginClassLoader requester) {
        // Exact plugin name first. Only then providers/aliases: FastAsyncWorldEdit
        // provides "WorldEdit", but WorldEdit's own loader must win when both
        // are declared dependencies.
        for (ConfiguredPluginClassLoader loader : loaders) {
            if (loader == requester || ordered.contains(loader)) {
                continue;
            }
            if (loader.getConfiguration().getName().equals(dependency)) {
                ordered.add(loader);
            }
        }
        for (ConfiguredPluginClassLoader loader : loaders) {
            if (loader == requester || ordered.contains(loader)) {
                continue;
            }
            if (loader.getConfiguration().getProvidedPlugins().contains(dependency)) {
                ordered.add(loader);
            }
        }
    }

    private static Class<?> tryLoad(ConfiguredPluginClassLoader loader, String name, boolean resolve) {
        try {
            // Без глобального поиска: иначе два плагина искали бы друг в друге.
            return loader.loadClass(name, resolve, false, true);
        } catch (ClassNotFoundException | NoClassDefFoundError e) {
            return null;
        }
    }

    /** `requester` объявил `target` в `depend`/`softdepend` (по имени или `provides`). */
    static boolean dependsOn(PluginMeta requester, PluginMeta target) {
        List<String> names = new ArrayList<>(target.getProvidedPlugins());
        names.add(target.getName());
        for (String n : names) {
            if (requester.getPluginDependencies().contains(n)
                    || requester.getPluginSoftDependencies().contains(n)) {
                return true;
            }
        }
        return false;
    }

    @Override
    public void remove(ConfiguredPluginClassLoader loader) {
        loaders.remove(loader);
    }

    @Override
    public void add(ConfiguredPluginClassLoader loader) {
        loaders.add(loader);
    }

    @Override
    public ClassLoaderAccess getAccess() {
        return loader -> true;
    }
}
