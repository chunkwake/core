package dev.chunkwake.bridge;

import java.lang.reflect.Field;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.Map;
import java.util.OptionalInt;
import org.bukkit.plugin.Plugin;

/**
 * Совместимость с WorldEdit без NMS-адаптера.
 *
 * <p>WorldEdit ищет в своём jar класс
 * {@code com.sk89q.worldedit.bukkit.adapter.impl.*}, скомпилированный против
 * NMS. У нас NMS нет, поэтому {@code BukkitImplAdapter} не загружается и
 * {@code BlockType.computeProperties} падает NPE на
 * {@code PlatformReadyEvent}.
 *
 * <p>После включения плагина подставляем лёгкий адаптер (dynamic proxy):
 * он отдаёт пустую карту свойств блоков (BlockType получает один default-state)
 * и {@code OptionalInt.empty()} для internal id — этого достаточно, чтобы
 * регистрация блоков прошла, и WorldEdit работал через Bukkit-API
 * (`Bukkit.createBlockData`). Затем повторно шлём {@code PlatformReadyEvent},
 * потому что первый вызов упал внутри WorldEdit.
 */
final class WorldEditSupport {
    private WorldEditSupport() {
    }

    static void install(Plugin plugin) {
        if (!"WorldEdit".equals(plugin.getName())) {
            return;
        }
        try {
            ClassLoader loader2 = plugin.getClass().getClassLoader();
            Class<?> adapterType = Class.forName(
                    "com.sk89q.worldedit.bukkit.adapter.BukkitImplAdapter", true, loader2);
            Object adapter = Proxy.newProxyInstance(
                    loader2, new Class<?>[] { adapterType }, new Adapter(loader2));
            Field field = plugin.getClass().getDeclaredField("adapter");
            field.setAccessible(true);
            Object lifecycle = field.get(plugin);
            lifecycle.getClass().getMethod("newValue", Object.class).invoke(lifecycle, adapter);
            allowUnsupportedVersionEditing(plugin, loader2);
            repostReady(loader2);
        } catch (Throwable t) {
            plugin.getLogger().warning("WorldEdit: не удалось включить API-режим: " + t);
        }
    }

    /**
     * Без NMS-адаптера WorldEdit умеет писать блоки только через generic-путь,
     * который включается флагом {@code unsupported-version-editing}.
     */
    private static void allowUnsupportedVersionEditing(Plugin plugin, ClassLoader loader)
            throws Exception {
        // WorldEditPlugin.getLocalConfiguration() -> тот же BukkitConfiguration,
        // который читает BukkitWorld.setBlock.
        Object config = plugin.getClass().getMethod("getLocalConfiguration").invoke(plugin);
        Field flag = config.getClass().getField("unsupportedVersionEditing");
        flag.setBoolean(config, true);
    }

    /** Повторно шлём {@code PlatformReadyEvent} с зарегистрированным адаптером. */
    private static void repostReady(ClassLoader loader) throws Exception {
        Class<?> worldEditType = Class.forName("com.sk89q.worldedit.WorldEdit", true, loader);
        Object worldEdit = method(worldEditType, "getInstance").invoke(null);
        Object manager = method(worldEditType, "getPlatformManager").invoke(worldEdit);
        Class<?> capabilityType = Class.forName(
                "com.sk89q.worldedit.extension.platform.Capability", true, loader);
        Object capability = Enum.valueOf(capabilityType.asSubclass(Enum.class), "WORLD_EDITING");
        Object platform = method(manager.getClass(), "queryCapability", capabilityType)
                .invoke(manager, capability);
        Class<?> platformType = Class.forName(
                "com.sk89q.worldedit.extension.platform.Platform", true, loader);
        Class<?> eventType = Class.forName(
                "com.sk89q.worldedit.event.platform.PlatformReadyEvent", true, loader);
        Object event = eventType.getConstructor(platformType).newInstance(platform);
        Object bus = method(worldEditType, "getEventBus").invoke(worldEdit);
        // EventBus.post(Object event), не post(Event).
        method(bus.getClass(), "post", Object.class).invoke(bus, event);
    }

    private static Method method(Class<?> type, String name, Class<?>... params) throws Exception {
        for (Class<?> c = type; c != null; c = c.getSuperclass()) {
            try {
                Method m = c.getDeclaredMethod(name, params);
                m.setAccessible(true);
                return m;
            } catch (NoSuchMethodException ignored) {
                // ищем выше
            }
        }
        throw new NoSuchMethodException(type.getName() + "." + name);
    }

    /** Минимальный {@code BukkitImplAdapter}: Bukkit-API + пустые свойства. */
    private static final class Adapter implements InvocationHandler {
        private final ClassLoader loader;

        Adapter(ClassLoader loader) {
            this.loader = loader;
        }

        @Override
        public Object invoke(Object proxy, Method method, Object[] args) {
            switch (method.getName()) {
                case "getProperties":
                    return Map.of();
                case "getInternalBlockStateId":
                    return OptionalInt.empty();
                case "getBlock":
                    // WorldEdit читает блок через адаптер, если он не null.
                    // Возвращаем BlockState из Bukkit-данных, иначе
                    // ChangeSetExtent падает на previous == null.
                    if (args != null && args.length == 1
                            && args[0] instanceof org.bukkit.Location location) {
                        return toBlockState(location);
                    }
                    break;
                case "getFullBlock":
                    if (args != null && args.length == 1
                            && args[0] instanceof org.bukkit.Location location) {
                        Object state = toBlockState(location);
                        return state == null ? null : toBaseBlock(state);
                    }
                    break;
                case "toString":
                    return "ChunkwakeWorldEditAdapter";
                case "hashCode":
                    return System.identityHashCode(proxy);
                case "equals":
                    return proxy == args[0];
                default:
                    return defaultValue(method.getReturnType());
            }
            return defaultValue(method.getReturnType());
        }

        /** {@code BlockState.toBaseBlock()} (класс WorldEdit, не виден на компиляции). */
        private static Object toBaseBlock(Object state) {
            try {
                return state.getClass().getMethod("toBaseBlock").invoke(state);
            } catch (Throwable broken) {
                return null;
            }
        }

        /** {@code BukkitAdapter.adapt(blockData)} через classloader плагина. */
        private Object toBlockState(org.bukkit.Location location) {
            try {
                org.bukkit.block.data.BlockData data = location.getBlock().getBlockData();
                Class<?> adapter = Class.forName(
                        "com.sk89q.worldedit.bukkit.BukkitAdapter", true, loader);
                return adapter.getMethod("adapt", org.bukkit.block.data.BlockData.class)
                        .invoke(null, data);
            } catch (Throwable broken) {
                return null;
            }
        }

        private static Object defaultValue(Class<?> type) {
            if (type == boolean.class) {
                return false;
            }
            if (type == int.class) {
                return 0;
            }
            if (type == long.class) {
                return 0L;
            }
            if (type == double.class) {
                return 0.0;
            }
            if (type == float.class) {
                return 0.0f;
            }
            if (type == short.class) {
                return (short) 0;
            }
            if (type == byte.class) {
                return (byte) 0;
            }
            if (type == char.class) {
                return '\0';
            }
            return null;
        }
    }
}
