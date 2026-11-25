package dev.chunkwake.bridge;

import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.OptionalInt;
import java.util.concurrent.ConcurrentHashMap;
import org.bukkit.Material;
import org.bukkit.block.data.BlockData;
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
 * свойства блоков берутся из Bukkit {@code BlockData} (имена и значения
 * свойств разбираются из {@code getAsString()}), блоки читаются/пишутся через
 * {@code BukkitAdapter} + обычный Bukkit API. Затем повторно шлём
 * {@code PlatformReadyEvent}, потому что первый вызов упал внутри WorldEdit.
 */
final class WorldEditSupport {
    private WorldEditSupport() {
    }

    /**
     * Вызывается ДО {@code enablePlugin}: вешает слушатель
     * {@code ConfigurationLoadEvent}. Он случается после того, как WorldEdit
     * попытался загрузить NMS-адаптер (и обнулил его), но до
     * {@code PlatformReadyEvent}. Там и подставляем API-адаптер — тогда первый
     * же {@code PlatformReadyEvent} проходит без NPE.
     */
    static void prepare(Plugin plugin) {
        if (!"WorldEdit".equals(plugin.getName())) {
            return;
        }
        try {
            ClassLoader loader = plugin.getClass().getClassLoader();
            Object adapter = newAdapter(loader);
            subscribeConfigurationLoad(loader, new Listener(plugin, adapter));
        } catch (Throwable t) {
            plugin.getLogger().warning("WorldEdit: не удалось подписаться на ConfigurationLoadEvent: " + t);
        }
    }

    /**
     * Запасной путь: если слушатель не сработал (другая версия WorldEdit),
     * ставим адаптер после включения и переотправляем {@code PlatformReadyEvent}.
     */
    static void install(Plugin plugin) {
        if (!"WorldEdit".equals(plugin.getName())) {
            return;
        }
        try {
            ClassLoader loader = plugin.getClass().getClassLoader();
            Field field = plugin.getClass().getDeclaredField("adapter");
            field.setAccessible(true);
            Object lifecycle = field.get(plugin);
            Object value = lifecycle.getClass().getMethod("value").invoke(lifecycle);
            if (value instanceof java.util.Optional<?> present && present.isPresent()) {
                return;
            }
            apply(plugin, loader, newAdapter(loader));
            repostReady(loader);
        } catch (Throwable t) {
            plugin.getLogger().warning("WorldEdit: не удалось включить API-режим: " + t);
        }
    }

    private static Object newAdapter(ClassLoader loader) throws Exception {
        Class<?> adapterType = Class.forName(
                "com.sk89q.worldedit.bukkit.adapter.BukkitImplAdapter", true, loader);
        return Proxy.newProxyInstance(
                loader, new Class<?>[] { adapterType }, new Adapter(loader));
    }

    private static void apply(Plugin plugin, ClassLoader loader, Object adapter) throws Exception {
        Field field = plugin.getClass().getDeclaredField("adapter");
        field.setAccessible(true);
        Object lifecycle = field.get(plugin);
        lifecycle.getClass().getMethod("newValue", Object.class).invoke(lifecycle, adapter);
        allowUnsupportedVersionEditing(plugin);
    }

    /** Подписка на {@code ConfigurationLoadEvent} через MethodHandleEventHandler. */
    private static void subscribeConfigurationLoad(ClassLoader loader, Listener listener)
            throws Exception {
        Class<?> worldEditType = Class.forName("com.sk89q.worldedit.WorldEdit", true, loader);
        Object worldEdit = method(worldEditType, "getInstance").invoke(null);
        Object bus = method(worldEditType, "getEventBus").invoke(worldEdit);
        Class<?> eventType = Class.forName(
                "com.sk89q.worldedit.event.platform.ConfigurationLoadEvent", true, loader);
        Class<?> handlerType = Class.forName(
                "com.sk89q.worldedit.util.eventbus.MethodHandleEventHandler", true, loader);
        Class<?> eventHandlerType = Class.forName(
                "com.sk89q.worldedit.util.eventbus.EventHandler", true, loader);
        Class<?> priorityType = Class.forName(
                "com.sk89q.worldedit.util.eventbus.EventHandler$Priority", true, loader);
        Object normal = Enum.valueOf(priorityType.asSubclass(Enum.class), "NORMAL");
        MethodHandle handle = MethodHandles.lookup().findVirtual(Listener.class,
                "onConfigurationLoad", MethodType.methodType(void.class, Object.class));
        Constructor<?> ctor = handlerType.getDeclaredConstructor(
                priorityType, Object.class, MethodHandle.class, String.class);
        ctor.setAccessible(true);
        Object handler = ctor.newInstance(normal, listener, handle, "onConfigurationLoad");
        method(bus.getClass(), "subscribe", Class.class, eventHandlerType)
                .invoke(bus, eventType, handler);
    }

    /**
     * Без NMS-адаптера WorldEdit умеет писать блоки только через generic-путь,
     * который включается флагом {@code unsupported-version-editing}.
     */
    private static void allowUnsupportedVersionEditing(Plugin plugin) throws Exception {
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

    /** Слушатель {@code ConfigurationLoadEvent}: ставит адаптер до PlatformReady. */
    private static final class Listener {
        private final Plugin plugin;
        private final Object adapter;

        Listener(Plugin plugin, Object adapter) {
            this.plugin = plugin;
            this.adapter = adapter;
        }

        public void onConfigurationLoad(Object event) {
            try {
                apply(plugin, plugin.getClass().getClassLoader(), adapter);
            } catch (Throwable t) {
                plugin.getLogger().warning("WorldEdit: не удалось поставить адаптер: " + t);
            }
        }
    }

    /** {@code BukkitImplAdapter} поверх Bukkit-API. */
    private static final class Adapter implements InvocationHandler {
        private final ClassLoader loader;
        /** Свойства по id блока: считаем один раз (getAllStates зовёт часто). */
        private final Map<String, Map<String, ?>> properties = new ConcurrentHashMap<>();

        Adapter(ClassLoader loader) {
            this.loader = loader;
        }

        @Override
        public Object invoke(Object proxy, Method method, Object[] args) {
            switch (method.getName()) {
                case "getProperties":
                    if (args != null && args.length == 1) {
                        return propertiesFor(args[0]);
                    }
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
                case "adapt":
                    // WorldEdit ждёт BaseItemStack даже для воздуха:
                    // PlatformManager.handleBlockInteract зовёт getType().
                    if (args != null && args.length == 1
                            && args[0] instanceof org.bukkit.inventory.ItemStack stack) {
                        return adaptItem(stack);
                    }
                    if (args != null && args.length == 1 && args[0] != null
                            && args[0].getClass().getName()
                                    .equals("com.sk89q.worldedit.blocks.BaseItemStack")) {
                        return adaptBaseItem(args[0]);
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

        // --- свойства блоков ---

        private Map<String, ?> propertiesFor(Object blockType) {
            try {
                String id = (String) method(blockType.getClass(), "id").invoke(blockType);
                return properties.computeIfAbsent(id, ignored -> computeProperties(blockType));
            } catch (Throwable broken) {
                return Map.of();
            }
        }

        private Map<String, ?> computeProperties(Object blockType) {
            Map<String, Object> out = new LinkedHashMap<>();
            try {
                Class<?> bukkitAdapter = Class.forName(
                        "com.sk89q.worldedit.bukkit.BukkitAdapter", true, loader);
                Class<?> blockTypeType = Class.forName(
                        "com.sk89q.worldedit.world.block.BlockType", true, loader);
                Material material = (Material) bukkitAdapter
                        .getMethod("adapt", blockTypeType).invoke(null, blockType);
                if (material == null) {
                    return out;
                }
                BlockData data = material.createBlockData();
                for (Map.Entry<String, String> e : parseProperties(data.getAsString()).entrySet()) {
                    Object property = makeProperty(e.getKey(), e.getValue());
                    if (property != null) {
                        out.put(e.getKey(), property);
                    }
                }
            } catch (Throwable ignored) {
                // блок без свойств
            }
            return out;
        }

        private Object makeProperty(String name, String current) {
            try {
                if (current.equals("true") || current.equals("false")) {
                    Class<?> type = Class.forName(
                            "com.sk89q.worldedit.registry.state.BooleanProperty", true, loader);
                    return type.getConstructor(String.class, List.class)
                            .newInstance(name, List.of(Boolean.TRUE, Boolean.FALSE));
                }
                if (isInt(current)) {
                    Class<?> type = Class.forName(
                            "com.sk89q.worldedit.registry.state.IntegerProperty", true, loader);
                    return type.getConstructor(String.class, List.class)
                            .newInstance(name, intValues(name));
                }
                Class<?> type = Class.forName(
                        "com.sk89q.worldedit.registry.state.EnumProperty", true, loader);
                return type.getConstructor(String.class, List.class)
                        .newInstance(name, enumValues(name, current));
            } catch (Throwable broken) {
                return null;
            }
        }

        /** Разбор {@code minecraft:oak_stairs[facing=north,half=bottom]} в карту. */
        private static Map<String, String> parseProperties(String asString) {
            int open = asString.indexOf('[');
            int close = asString.lastIndexOf(']');
            if (open < 0 || close <= open) {
                return Map.of();
            }
            Map<String, String> out = new LinkedHashMap<>();
            for (String part : asString.substring(open + 1, close).split(",")) {
                int eq = part.indexOf('=');
                if (eq > 0) {
                    out.put(part.substring(0, eq).trim(), part.substring(eq + 1).trim());
                }
            }
            return out;
        }

        private static boolean isInt(String value) {
            if (value.isEmpty()) {
                return false;
            }
            for (int i = 0; i < value.length(); i++) {
                char c = value.charAt(i);
                if (i == 0 && c == '-') {
                    continue;
                }
                if (c < '0' || c > '9') {
                    return false;
                }
            }
            return true;
        }

        private static List<Integer> intValues(String name) {
            int[] range = intRange(name);
            List<Integer> out = new ArrayList<>(range[1] - range[0] + 1);
            for (int i = range[0]; i <= range[1]; i++) {
                out.add(i);
            }
            return out;
        }

        private static int[] intRange(String name) {
            return switch (name) {
                case "age" -> new int[] {0, 25};
                case "level" -> new int[] {0, 15};
                case "power" -> new int[] {0, 15};
                case "rotation" -> new int[] {0, 15};
                case "layers" -> new int[] {1, 8};
                case "distance" -> new int[] {1, 7};
                case "moisture" -> new int[] {0, 7};
                case "note" -> new int[] {0, 24};
                case "delay" -> new int[] {1, 4};
                case "bites" -> new int[] {0, 6};
                case "pickles" -> new int[] {1, 4};
                case "eggs" -> new int[] {1, 4};
                case "hatch" -> new int[] {0, 2};
                case "stage" -> new int[] {0, 1};
                case "candles" -> new int[] {1, 4};
                case "flower_amount" -> new int[] {1, 4};
                case "segment_amount" -> new int[] {1, 4};
                case "charges" -> new int[] {0, 4};
                default -> new int[] {0, 15};
            };
        }

        private static List<String> enumValues(String name, String current) {
            List<String> known = switch (name) {
                case "facing" -> List.of("north", "south", "east", "west", "up", "down");
                case "half" -> List.of("top", "bottom");
                case "shape" -> List.of("straight", "inner_left", "inner_right",
                        "outer_left", "outer_right");
                case "type" -> List.of("top", "bottom", "double");
                case "axis" -> List.of("x", "y", "z");
                case "attachment" -> List.of("floor", "ceiling", "single_wall", "double_wall");
                case "face" -> List.of("floor", "wall", "ceiling");
                case "hinge" -> List.of("left", "right");
                case "part" -> List.of("head", "foot");
                case "mode" -> List.of("compare", "subtract", "save", "load",
                        "corner", "data", "start", "log", "fail");
                case "orientation" -> List.of("down_east", "down_north", "down_south",
                        "down_west", "up_east", "up_north", "up_south", "up_west",
                        "north_up", "east_up", "south_up", "west_up");
                case "tilt" -> List.of("none", "unstable", "partial", "full");
                case "thickness" -> List.of("tip", "frustum", "middle", "base");
                case "vertical_direction" -> List.of("up", "down");
                case "instrument" -> List.of("harp", "basedrum", "snare", "hat",
                        "glockenspiel", "bell", "flute", "chime", "guitar",
                        "xylophone", "iron_xylophone", "cow_bell", "didgeridoo",
                        "bit", "banjo", "pling", "zombie", "skeleton", "creeper",
                        "dragon_head", "piglin", "custom_head");
                case "sculk_sensor_phase" -> List.of("inactive", "active", "cooldown");
                default -> List.of(current.toLowerCase(Locale.ROOT));
            };
            return known;
        }

        // --- блоки ---

        /** {@code BlockState.toBaseBlock()} (класс WorldEdit, не виден на компиляции). */
        private static Object toBaseBlock(Object state) {
            try {
                return state.getClass().getMethod("toBaseBlock").invoke(state);
            } catch (Throwable broken) {
                return null;
            }
        }

        /** {@code BukkitAdapter.adapt(BaseItemStack)}: обратно в Bukkit ItemStack. */
        private Object adaptBaseItem(Object base) {
            try {
                Object type = base.getClass().getMethod("getType").invoke(base);
                String id = (String) type.getClass().getMethod("id").invoke(type);
                int amount = (int) base.getClass().getMethod("getAmount").invoke(base);
                Material material = Material.matchMaterial(id);
                return new org.bukkit.inventory.ItemStack(
                        material == null ? Material.AIR : material, Math.max(0, amount));
            } catch (Throwable broken) {
                return new org.bukkit.inventory.ItemStack(Material.AIR);
            }
        }

        /** {@code BukkitAdapter.adapt(ItemStack)}: тип и количество без NMS. */
        private Object adaptItem(org.bukkit.inventory.ItemStack stack) {
            try {
                Class<?> itemTypes = Class.forName(
                        "com.sk89q.worldedit.world.item.ItemTypes", true, loader);
                Object type = itemTypes.getMethod("get", String.class).invoke(
                        null, stack.getType().getKey().toString());
                if (type == null) {
                    return null;
                }
                Class<?> itemType = Class.forName(
                        "com.sk89q.worldedit.world.item.ItemType", true, loader);
                Class<?> base = Class.forName(
                        "com.sk89q.worldedit.blocks.BaseItemStack", true, loader);
                return base.getConstructor(itemType, int.class)
                        .newInstance(type, Math.max(0, stack.getAmount()));
            } catch (Throwable broken) {
                return null;
            }
        }

        /** {@code BukkitAdapter.adapt(blockData)} через classloader плагина. */
        private Object toBlockState(org.bukkit.Location location) {
            try {
                BlockData data = location.getBlock().getBlockData();
                Class<?> adapter = Class.forName(
                        "com.sk89q.worldedit.bukkit.BukkitAdapter", true, loader);
                return adapter.getMethod("adapt", BlockData.class).invoke(null, data);
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
