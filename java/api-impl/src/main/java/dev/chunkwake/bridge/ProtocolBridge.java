package dev.chunkwake.bridge;

import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.util.Set;

/**
 * Мост Rust-сети в ProtocolLib без Netty-инжектора.
 *
 * <p>Настоящие TCP-каналы живут в Rust, поэтому ProtocolLib не может сам
 * перехватить пакет. Для команд, дошедших до Bukkit-диспетчера, строим
 * {@code PacketContainer} типа {@code PacketType.Play.Client.CHAT_COMMAND} и
 * вызываем {@code PacketListener.onPacketReceiving} у всех слушателей,
 * подписанных на этот тип.
 *
 * <p>Это первый рабочий путь: слушатель получает реальный пакет команды
 * (поле {@code command}) тогда, когда Rust передал её в Bukkit. Полный
 * перехват всех пакетов потребует владения Netty-каналом на Java-стороне.
 */
final class ProtocolBridge {
    private static boolean initialized;
    private static boolean available;
    private static Object plugin;
    private static Object manager;
    private static Object chatType;
    private static Class<?> packetClass;
    private static Constructor<?> containerConstructor;
    private static Method getListeners;
    private static Method getWhitelist;
    private static Method getTypes;
    private static Method onReceiving;
    private static Method fromClient;

    private ProtocolBridge() {
    }

    /**
     * Вызывается до {@code enablePlugin(ProtocolLib)}: привязываем
     * {@code CHAT_COMMAND} к нашему NMS-классу, иначе
     * {@code PacketRegistry.tryGetPacketClass()} кеширует «неизвестный пакет»
     * и слушатели отклоняются.
     */
    static void prepare(org.bukkit.plugin.Plugin protocolLib) {
        if (!"ProtocolLib".equals(protocolLib.getName())) {
            return;
        }
        try {
            ClassLoader loader = protocolLib.getClass().getClassLoader();
            Class<?> packetType = Class.forName(
                    "com.comphenix.protocol.PacketType", true, loader);
            Class<?> playClient = Class.forName(
                    "com.comphenix.protocol.PacketType$Play$Client", true, loader);
            Object type = playClient.getField("CHAT_COMMAND").get(null);
            Class<?> packet = Class.forName(
                    "net.minecraft.network.protocol.game.ServerboundChatCommandPacket", true, loader);
            Class<?> registry = Class.forName(
                    "com.comphenix.protocol.injector.packet.PacketRegistry", true, loader);
            Method initialize = registry.getDeclaredMethod("initialize");
            initialize.setAccessible(true);
            initialize.invoke(null);
            java.lang.reflect.Field registerField = registry.getDeclaredField("REGISTER");
            registerField.setAccessible(true);
            Object register = registerField.get(null);
            Object sender = packetType.getMethod("getSender").invoke(type);
            // Не грузим WrappedStreamCodec/Sender классы: их static init тянет
            // отсутствующие StreamDecoder/StreamEncoder. null подходит для
            // 4-го параметра registerPacket.
            Method registerPacket = null;
            for (Method candidate : register.getClass().getMethods()) {
                if (candidate.getName().equals("registerPacket")
                        && candidate.getParameterCount() == 4) {
                    registerPacket = candidate;
                    break;
                }
            }
            if (registerPacket == null) {
                throw new NoSuchMethodException("PacketRegistry.Register.registerPacket");
            }
            registerPacket.invoke(register, type, packet, sender, null);
            protocolLib.getLogger().info("ProtocolBridge: CHAT_COMMAND -> " + packet.getName());
        } catch (Throwable broken) {
            protocolLib.getLogger().log(java.util.logging.Level.WARNING,
                    "ProtocolBridge: prepare failed", broken);
        }
    }

    static void command(org.bukkit.command.CommandSender sender, String line) {
        if (!(sender instanceof org.bukkit.entity.Player player)) {
            return;
        }
        init();
        if (!available) {
            return;
        }
        try {
            Object packet = packetClass.getConstructor(String.class).newInstance(line);
            Object container = containerConstructor.newInstance(chatType, packet);
            Object event = fromClient.invoke(null, plugin, container, player);
            Set<?> listeners = (Set<?>) getListeners.invoke(manager);
            for (Object listener : listeners) {
                Object whitelist = getWhitelist.invoke(listener);
                Set<?> types = (Set<?>) getTypes.invoke(whitelist);
                if (types.contains(chatType)) {
                    onReceiving.invoke(listener, event);
                }
            }
        } catch (Throwable unavailable) {
            Bridge.log(1, "protocol: fire failed: " + unavailable);
        }
    }

    private static synchronized void init() {
        if (initialized) {
            return;
        }
        initialized = true;
        try {
            org.bukkit.plugin.Plugin protocolLib = org.bukkit.Bukkit.getPluginManager()
                    .getPlugin("ProtocolLib");
            if (protocolLib == null || !protocolLib.isEnabled()) {
                return;
            }
            ClassLoader loader = protocolLib.getClass().getClassLoader();
            Class<?> library = Class.forName(
                    "com.comphenix.protocol.ProtocolLibrary", true, loader);
            manager = library.getMethod("getProtocolManager").invoke(null);
            Class<?> packetType = Class.forName(
                    "com.comphenix.protocol.PacketType", true, loader);
            Class<?> playClient = Class.forName(
                    "com.comphenix.protocol.PacketType$Play$Client", true, loader);
            chatType = playClient.getField("CHAT_COMMAND").get(null);
            Class<?> container = Class.forName(
                    "com.comphenix.protocol.events.PacketContainer", true, loader);
            containerConstructor = container.getConstructor(packetType, Object.class);
            Class<?> event = Class.forName(
                    "com.comphenix.protocol.events.PacketEvent", true, loader);
            Class<?> listener = Class.forName(
                    "com.comphenix.protocol.events.PacketListener", true, loader);
            Class<?> whitelist = Class.forName(
                    "com.comphenix.protocol.events.ListeningWhitelist", true, loader);
            Class<?> protocolManager = Class.forName(
                    "com.comphenix.protocol.ProtocolManager", true, loader);
            getListeners = protocolManager.getMethod("getPacketListeners");
            onReceiving = listener.getMethod("onPacketReceiving", event);
            getWhitelist = listener.getMethod("getReceivingWhitelist");
            getTypes = whitelist.getMethod("getTypes");
            fromClient = event.getMethod("fromClient", Object.class, container,
                    org.bukkit.entity.Player.class);
            packetClass = Class.forName(
                    "net.minecraft.network.protocol.game.ServerboundChatCommandPacket", true, loader);
            plugin = protocolLib;
            available = true;
            Bridge.log(3, "protocol: bridge ready");
        } catch (Throwable unavailable) {
            available = false;
            Bridge.log(1, "protocol: init failed: " + unavailable);
        }
    }
}
