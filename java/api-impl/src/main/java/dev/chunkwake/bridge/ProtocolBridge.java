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
    private static Method fromServer;
    private static Method onSending;
    private static Method getSendingWhitelist;
    private static Method findCurrent;
    private static Object protocolPlay;
    private static Object senderClient;
    private static Object senderServer;
    private static java.util.Map<String, Object> byName = java.util.Map.of();
    /** Класс-носитель сырых байтов для PacketContainer. */
    public static final class RawPayload {
        public byte[] data;

        public RawPayload(byte[] data) {
            this.data = data;
        }
    }

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
        initialized = false;
        init();
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
            // Остальные clientbound-типы: их NMS-классов у Chunkwake нет, но
            // слушатели ProtocolLib должны регистрироваться. Даём каждому
            // типу носитель сырых байтов (RawPayload) — события и отмена
            // работают, доступ к полям пакета — нет.
            Class<?> senderClass = Class.forName(
                    "com.comphenix.protocol.PacketType$Sender", true, loader);
            Object clientSender = java.lang.Enum.valueOf((Class) senderClass, "SERVER");
            java.lang.reflect.Method values = packetType.getMethod("values");
            java.lang.reflect.Method isSupported = packetType.getMethod("isSupported");
            java.lang.reflect.Method getProtocol = packetType.getMethod("getProtocol");
            java.lang.reflect.Method getSender = packetType.getMethod("getSender");
            int registered = 0;
            for (Object candidate : (Iterable<?>) values.invoke(null)) {
                if (!Boolean.TRUE.equals(isSupported.invoke(candidate))
                        && "PLAY".equals(String.valueOf(getProtocol.invoke(candidate)))
                        && "SERVER".equals(String.valueOf(getSender.invoke(candidate)))) {
                    Class<?> carrier = "SYSTEM_CHAT".equalsIgnoreCase(
                            String.valueOf(candidate.getClass().getMethod("name").invoke(candidate)))
                                    ? Class.forName(
                                            "net.minecraft.network.protocol.game.ClientboundSystemChatPacket",
                                            true, loader)
                                    : RawPayload.class;
                    registerPacket.invoke(register, candidate, carrier, clientSender, null);
                    registered++;
                }
            }
            protocolLib.getLogger().info("ProtocolBridge: clientbound raw types registered: " + registered);
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

    /**
     * Clientbound-пакет из Rust: событие ProtocolLib для конкретного игрока.
     * Возвращает байты для отправки либо {@code null}, если плагин отменил пакет.
     */
    static byte[] clientbound(org.bukkit.entity.Player player, String name, byte[] data) {
        if (player == null || data == null || name == null) {
            return data;
        }
        init();
        if (!available) {
            return data;
        }
        try {
            Object type = byName.get(name.toUpperCase(java.util.Locale.ROOT));
            if (type == null) {
                return data;
            }
            if ("SYSTEM_CHAT".equalsIgnoreCase(name)) {
                return systemChat(player, type, data);
            }
            Set<?> listeners = (Set<?>) getListeners.invoke(manager);
            boolean any = false;
            for (Object listener : listeners) {
                Set<?> types = (Set<?>) getTypes.invoke(getSendingWhitelist.invoke(listener));
                if (types.isEmpty()) {
                    continue;
                }
                if (types.contains(type)) {
                    any = true;
                }
            }
            if (!any) {
                return data;
            }
            Object container = containerConstructor.newInstance(type, new RawPayload(data));
            Object event = fromServer.invoke(null, plugin, container, player);
            for (Object listener : listeners) {
                Set<?> types = (Set<?>) getTypes.invoke(getSendingWhitelist.invoke(listener));
                if (types.contains(type)) {
                    onSending.invoke(listener, event);
                }
            }
            if (Boolean.TRUE.equals(event.getClass().getMethod("isCancelled").invoke(event))) {
                return null;
            }
            Object handle = container.getClass().getMethod("getHandle").invoke(container);
            if (handle instanceof RawPayload raw && raw.data != null) {
                return raw.data;
            }
            return data;
        } catch (Throwable broken) {
            Bridge.log(1, "protocol: clientbound failed: " + broken);
            return data;
        }
    }

    /** Есть ли хоть один слушатель clientbound-пакетов. */
    static boolean hasSending() {
        init();
        if (!available) {
            return false;
        }
        try {
            for (Object listener : (Set<?>) getListeners.invoke(manager)) {
                Set<?> types = (Set<?>) getTypes.invoke(getSendingWhitelist.invoke(listener));
                if (!types.isEmpty()) {
                    return true;
                }
            }
        } catch (Throwable broken) {
            Bridge.log(1, "protocol: sending probe failed: " + broken);
        }
        return false;
    }

    /** Real-field путь для {@code ClientboundSystemChatPacket}. */
    private static byte[] systemChat(org.bukkit.entity.Player player, Object type, byte[] data) {
        try {
            int[] pos = {0};
            int id = readVarInt(data, pos);
            java.io.DataInputStream in = new java.io.DataInputStream(
                    new java.io.ByteArrayInputStream(data, pos[0], data.length - pos[0]));
            net.minecraft.nbt.CompoundTag tag = net.minecraft.nbt.NbtIo.readNetwork(
                    in, net.minecraft.nbt.NbtAccounter.unlimitedHeap());
            boolean overlay = in.readBoolean();
            String text = tag.getStringOr("text", null);
            String translate = tag.getStringOr("translate", null);
            net.minecraft.network.chat.Component content = text != null
                    ? net.minecraft.network.chat.Component.literal(text)
                    : translate != null
                            ? net.minecraft.network.chat.Component.translatable(translate)
                            : net.minecraft.network.chat.Component.literal(tag.toString());
            Object packet = Class.forName(
                    "net.minecraft.network.protocol.game.ClientboundSystemChatPacket", true,
                    type.getClass().getClassLoader())
                    .getConstructor(net.minecraft.network.chat.Component.class, boolean.class)
                    .newInstance(content, overlay);
            Object container = containerConstructor.newInstance(type, packet);
            Object event = fromServer.invoke(null, plugin, container, player);
            for (Object listener : (Set<?>) getListeners.invoke(manager)) {
                Set<?> types = (Set<?>) getTypes.invoke(getSendingWhitelist.invoke(listener));
                if (types.contains(type)) {
                    onSending.invoke(listener, event);
                }
            }
            if (Boolean.TRUE.equals(event.getClass().getMethod("isCancelled").invoke(event))) {
                return null;
            }
            net.minecraft.network.chat.Component changed = (net.minecraft.network.chat.Component)
                    packet.getClass().getMethod("content").invoke(packet);
            boolean changedOverlay = (Boolean) packet.getClass().getMethod("overlay").invoke(packet);
            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream(data.length + 16);
            writeVarInt(bytes, id);
            net.minecraft.nbt.CompoundTag outTag = new net.minecraft.nbt.CompoundTag();
            outTag.putString("text", changed == null ? "" : changed.getString());
            net.minecraft.nbt.NbtIo.writeNetwork(outTag,
                    new java.io.DataOutputStream(bytes));
            bytes.write(changedOverlay ? 1 : 0);
            return bytes.toByteArray();
        } catch (Throwable broken) {
            Throwable cause = broken.getCause() == null ? broken : broken.getCause();
            java.util.logging.Logger.getLogger("Chunkwake").warning(
                    "protocol: system_chat field bridge failed: " + cause);
            return data;
        }
    }

    private static int readVarInt(byte[] data, int[] pos) {
        int value = 0;
        int shift = 0;
        while (true) {
            byte b = data[pos[0]++];
            value |= (b & 0x7f) << shift;
            if ((b & 0x80) == 0) {
                return value;
            }
            shift += 7;
        }
    }

    private static void writeVarInt(java.io.ByteArrayOutputStream out, int value) {
        while (true) {
            if ((value & ~0x7F) == 0) {
                out.write(value);
                return;
            }
            out.write((value & 0x7F) | 0x80);
            value >>>= 7;
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
            if (protocolLib == null) {
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
            fromServer = event.getMethod("fromServer", Object.class, container,
                    org.bukkit.entity.Player.class);
            onSending = listener.getMethod("onPacketSending", event);
            getSendingWhitelist = listener.getMethod("getSendingWhitelist");
            Class<?> protocol = Class.forName(
                    "com.comphenix.protocol.PacketType$Protocol", true, loader);
            Class<?> sender = Class.forName(
                    "com.comphenix.protocol.PacketType$Sender", true, loader);
            protocolPlay = protocol.getField("PLAY").get(null);
            senderClient = java.lang.Enum.valueOf((Class) sender, "CLIENT");
            senderServer = java.lang.Enum.valueOf((Class) sender, "SERVER");
            findCurrent = packetType.getMethod("findCurrent", protocol, sender, int.class);
            java.lang.reflect.Method values = packetType.getMethod("values");
            java.lang.reflect.Method getProtocol = packetType.getMethod("getProtocol");
            java.lang.reflect.Method getSender = packetType.getMethod("getSender");
            java.lang.reflect.Method nameOf = packetType.getMethod("name");
            java.util.Map<String, Object> names = new java.util.HashMap<>();
            for (Object candidate : (Iterable<?>) values.invoke(null)) {
                if (protocolPlay.equals(getProtocol.invoke(candidate))
                        && senderServer.equals(getSender.invoke(candidate))) {
                    names.put(((String) nameOf.invoke(candidate))
                            .toUpperCase(java.util.Locale.ROOT), candidate);
                }
            }
            byName = names;
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
