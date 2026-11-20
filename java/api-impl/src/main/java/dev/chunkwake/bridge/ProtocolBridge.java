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
    private static Object soundType;
    private static Object titleType;
    private static Object entityDataType;
    private static Object chunkForgetType;
    private static Object playerInfoType;
    private static Object playerChatType;
    private static Object addEntityType;
    private static Object blockUpdateType;
    private static Object multiBlockType;
    private static Object equipmentType;
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
                if (!"PLAY".equals(String.valueOf(getProtocol.invoke(candidate)))
                        || !"SERVER".equals(String.valueOf(getSender.invoke(candidate)))) {
                    continue;
                }
                @SuppressWarnings("unchecked")
                java.util.List<String> classNames = (java.util.List<String>)
                        candidate.getClass().getMethod("getClassNames").invoke(candidate);
                Class<?> carrier = RawPayload.class;
                if (classNames.contains(
                        "net.minecraft.network.protocol.game.ClientboundSystemChatPacket")) {
                    carrier = Class.forName(
                            "net.minecraft.network.protocol.game.ClientboundSystemChatPacket",
                            true, loader);
                } else if (classNames.contains(
                        "net.minecraft.network.protocol.game.ClientboundSoundPacket")) {
                    carrier = Class.forName(
                            "net.minecraft.network.protocol.game.ClientboundSoundPacket",
                            true, loader);
                    soundType = candidate;
                } else if (classNames.contains(
                        "net.minecraft.network.protocol.game.ClientboundSetTitleTextPacket")) {
                    carrier = Class.forName(
                            "net.minecraft.network.protocol.game.ClientboundSetTitleTextPacket",
                            true, loader);
                    titleType = candidate;
                } else if ("SPAWN_ENTITY".equals(String.valueOf(
                        candidate.getClass().getMethod("name").invoke(candidate)))) {
                    carrier = Class.forName(
                            "net.minecraft.network.protocol.game.ClientboundAddEntityPacket",
                            true, loader);
                    addEntityType = candidate;
                } else if ("BLOCK_CHANGE".equals(String.valueOf(
                        candidate.getClass().getMethod("name").invoke(candidate)))) {
                    carrier = Class.forName(
                            "net.minecraft.network.protocol.game.ClientboundBlockUpdatePacket",
                            true, loader);
                    blockUpdateType = candidate;
                } else if ("MULTI_BLOCK_CHANGE".equals(String.valueOf(
                        candidate.getClass().getMethod("name").invoke(candidate)))) {
                    carrier = Class.forName(
                            "net.minecraft.network.protocol.game.ClientboundSectionBlocksUpdatePacket",
                            true, loader);
                    multiBlockType = candidate;
                } else if (classNames.contains(
                        "net.minecraft.network.protocol.game.ClientboundSetEquipmentPacket")) {
                    carrier = Class.forName(
                            "net.minecraft.network.protocol.game.ClientboundSetEquipmentPacket",
                            true, loader);
                    equipmentType = candidate;
                } else if (classNames.contains(
                        "net.minecraft.network.protocol.game.ClientboundPlayerChatPacket")
                        || "CHAT".equals(String.valueOf(
                                candidate.getClass().getMethod("name").invoke(candidate)))) {
                    // ProtocolLib называет этот тип `CHAT` (legacy), а классы
                    // ищет по именам PlayerChat/Chat.
                    carrier = Class.forName(
                            "net.minecraft.network.protocol.game.ClientboundPlayerChatPacket",
                            true, loader);
                    playerChatType = candidate;
                } else if (classNames.contains(
                        "net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket")) {
                    carrier = Class.forName(
                            "net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket",
                            true, loader);
                    playerInfoType = candidate;
                } else if (classNames.contains(
                        "net.minecraft.network.protocol.game.ClientboundForgetLevelChunkPacket")) {
                    carrier = Class.forName(
                            "net.minecraft.network.protocol.game.ClientboundForgetLevelChunkPacket",
                            true, loader);
                    chunkForgetType = candidate;
                }
                if (carrier != RawPayload.class
                        || !Boolean.TRUE.equals(isSupported.invoke(candidate))) {
                    registerPacket.invoke(register, candidate, carrier, clientSender, null);
                    registered++;
                }
            }
            // Serverbound USE_ENTITY: ProtocolLib выводит из него EnumEntityUseAction,
            // без этого `EnumWrappers` не инициализируется (PlayerInfoData).
            try {
                Object useEntity = null;
                for (Object candidate : (Iterable<?>) values.invoke(null)) {
                    if (!"PLAY".equals(String.valueOf(getProtocol.invoke(candidate)))
                            || !"CLIENT".equals(String.valueOf(getSender.invoke(candidate)))) {
                        continue;
                    }
                    @SuppressWarnings("unchecked")
                    java.util.List<String> names = (java.util.List<String>) candidate.getClass()
                            .getMethod("getClassNames").invoke(candidate);
                    boolean interact = false;
                    for (String packetName : names) {
                        if (packetName != null
                                && packetName.endsWith("ServerboundInteractPacket")) {
                            interact = true;
                        }
                    }
                    if (interact) {
                        useEntity = candidate;
                        break;
                    }
                }
                if (useEntity != null) {
                    Class<?> carrier = Class.forName(
                            "net.minecraft.network.protocol.game.ServerboundInteractPacket",
                            true, loader);
                    Object client = java.lang.Enum.valueOf((Class) senderClass, "CLIENT");
                    registerPacket.invoke(register, useEntity, carrier, client, null);
                    Bridge.log(2, "protocol: serverbound USE_ENTITY registered");
                }
            } catch (Throwable broken) {
                Bridge.log(1, "protocol: USE_ENTITY registration failed: " + broken);
            }
            protocolLib.getLogger().info("ProtocolBridge: clientbound raw types registered: "
                    + registered
                    + "; carriers sound=" + (soundType != null)
                    + " title=" + (titleType != null)
                    + " entity=" + (entityDataType != null)
                    + " chunk=" + (chunkForgetType != null)
                    + " playerInfo=" + (playerInfoType != null)
                    + " playerChat=" + (playerChatType != null)
                    + " addEntity=" + (addEntityType != null)
                    + " blockUpdate=" + (blockUpdateType != null)
                    + " multiBlock=" + (multiBlockType != null));
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
            if ("sound".equalsIgnoreCase(name) && soundType != null) {
                return sound(player, soundType, data);
            }
            if ("set_title_text".equalsIgnoreCase(name) && titleType != null) {
                return title(player, titleType, data);
            }
            if ("player_info_update".equalsIgnoreCase(name) && playerInfoType != null) {
                return playerInfo(player, playerInfoType, data);
            }
            if ("player_chat".equalsIgnoreCase(name) && playerChatType != null) {
                return playerChat(player, playerChatType, data);
            }
            if ("add_entity".equalsIgnoreCase(name) && addEntityType != null) {
                return addEntity(player, addEntityType, data);
            }
            if ("block_update".equalsIgnoreCase(name) && blockUpdateType != null) {
                return blockUpdate(player, blockUpdateType, data);
            }
            if ("section_blocks_update".equalsIgnoreCase(name) && multiBlockType != null) {
                return sectionBlocks(player, multiBlockType, data);
            }
            if ("set_equipment".equalsIgnoreCase(name) && equipmentType != null) {
                return setEquipment(player, equipmentType, data);
            }
            if ("set_entity_data".equalsIgnoreCase(name) && entityDataType != null) {
                return entityData(player, entityDataType, data);
            }
            if ("forget_level_chunk".equalsIgnoreCase(name) && chunkForgetType != null) {
                return forgetChunk(player, chunkForgetType, data);
            }
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

    /** Field-level for {@code ClientboundSectionBlocksUpdatePacket}. */
    private static byte[] sectionBlocks(org.bukkit.entity.Player player, Object type, byte[] data) {
        try {
            int[] pos = {0};
            int id = readVarInt(data, pos);
            java.io.DataInputStream in = new java.io.DataInputStream(
                    new java.io.ByteArrayInputStream(data, pos[0], data.length - pos[0]));
            long sectionPos = in.readLong();
            int count = readVarInt(in);
            short[] positions = new short[count];
            int[] states = new int[count];
            for (int i = 0; i < count; i++) {
                long packed = readVarLong(in);
                positions[i] = (short) (packed & 0xFFF);
                states[i] = (int) (packed >>> 12);
            }
            Class<?> packetClass = Class.forName(
                    "net.minecraft.network.protocol.game.ClientboundSectionBlocksUpdatePacket",
                    true, type.getClass().getClassLoader());
            Object packet = packetClass.getConstructor(long.class, short[].class, int[].class)
                    .newInstance(sectionPos, positions, states);
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
            long newSection = (Long) packetClass.getMethod("sectionPos").invoke(packet);
            short[] newPositions = (short[]) packetClass.getMethod("positions").invoke(packet);
            int[] newStates = (int[]) packetClass.getMethod("states").invoke(packet);
            int size = Math.min(newPositions.length, newStates.length);
            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream(data.length);
            writeVarInt(bytes, id);
            java.io.DataOutputStream out = new java.io.DataOutputStream(bytes);
            out.writeLong(newSection);
            out.flush();
            writeVarInt(bytes, size);
            for (int i = 0; i < size; i++) {
                writeVarLong(bytes, (((long) newStates[i]) << 12)
                        | (newPositions[i] & 0xFFFL));
            }
            out.flush();
            byte[] result = bytes.toByteArray();
            if (!java.util.Arrays.equals(result, data)) {
                Bridge.log(2, "protocol: section_blocks_update re-encoded (section=" + newSection
                        + ", changes=" + size + ", first_state="
                        + (size == 0 ? -1 : newStates[0]) + ")");
            }
            return result;
        } catch (Throwable broken) {
            Throwable cause = broken.getCause() == null ? broken : broken.getCause();
            Bridge.log(1, "protocol: section_blocks_update bridge failed: " + cause);
            return data;
        }
    }

    /** Field-level for {@code ClientboundAddEntityPacket}. */
    private static byte[] addEntity(org.bukkit.entity.Player player, Object type, byte[] data) {
        try {
            int[] pos = {0};
            int id = readVarInt(data, pos);
            java.io.DataInputStream in = new java.io.DataInputStream(
                    new java.io.ByteArrayInputStream(data, pos[0], data.length - pos[0]));
            int entityId = readVarInt(in);
            java.util.UUID uuid = new java.util.UUID(in.readLong(), in.readLong());
            int entityType = readVarInt(in);
            double x = in.readDouble();
            double y = in.readDouble();
            double z = in.readDouble();
            // `Vec3.LP_STREAM_CODEC` (LpVec3): 1 байт b0; если не 0 — b1,
            // 4 байта unsigned int BE и VarInt масштаба при continuation-бите
            // (b0 & 4). Скорость сохраняем как есть: плагин может заменить
            // байты, а нулевая — это просто один 0.
            java.io.ByteArrayOutputStream velocity = new java.io.ByteArrayOutputStream(9);
            int b0 = in.readUnsignedByte();
            velocity.write(b0);
            if (b0 != 0) {
                velocity.write(in.readUnsignedByte());
                for (int i = 0; i < 4; i++) {
                    velocity.write(in.readUnsignedByte());
                }
                if ((b0 & 4) != 0) {
                    while (true) {
                        int part = in.readUnsignedByte();
                        velocity.write(part);
                        if ((part & 0x80) == 0) {
                            break;
                        }
                    }
                }
            }
            int pitch = in.readUnsignedByte();
            int yaw = in.readUnsignedByte();
            int headYaw = in.readUnsignedByte();
            int extra = readVarInt(in);
            Class<?> packetClass = Class.forName(
                    "net.minecraft.network.protocol.game.ClientboundAddEntityPacket", true,
                    type.getClass().getClassLoader());
            Object packet = packetClass.getConstructor(int.class, java.util.UUID.class, int.class,
                    double.class, double.class, double.class, byte[].class, int.class, int.class,
                    int.class, int.class)
                    .newInstance(entityId, uuid, entityType, x, y, z, velocity.toByteArray(), pitch, yaw,
                            headYaw, extra);
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
            int newId = (Integer) packetClass.getMethod("id").invoke(packet);
            java.util.UUID newUuid = (java.util.UUID) packetClass.getMethod("uuid").invoke(packet);
            int newType = (Integer) packetClass.getMethod("type").invoke(packet);
            double newX = (Double) packetClass.getMethod("x").invoke(packet);
            double newY = (Double) packetClass.getMethod("y").invoke(packet);
            double newZ = (Double) packetClass.getMethod("z").invoke(packet);
            byte[] newVelocity = (byte[]) packetClass.getMethod("velocity").invoke(packet);
            int newPitch = (Integer) packetClass.getMethod("pitch").invoke(packet);
            int newYaw = (Integer) packetClass.getMethod("yaw").invoke(packet);
            int newHead = (Integer) packetClass.getMethod("headYaw").invoke(packet);
            int newData = (Integer) packetClass.getMethod("data").invoke(packet);
            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream(data.length);
            writeVarInt(bytes, id);
            writeVarInt(bytes, newId);
            java.io.DataOutputStream out = new java.io.DataOutputStream(bytes);
            out.writeLong(newUuid.getMostSignificantBits());
            out.writeLong(newUuid.getLeastSignificantBits());
            out.flush();
            writeVarInt(bytes, newType);
            out.writeDouble(newX);
            out.writeDouble(newY);
            out.writeDouble(newZ);
            out.write(newVelocity == null ? new byte[] {0} : newVelocity);
            out.writeByte(newPitch);
            out.writeByte(newYaw);
            out.writeByte(newHead);
            out.flush();
            writeVarInt(bytes, newData);
            out.flush();
            byte[] result = bytes.toByteArray();
            if (!java.util.Arrays.equals(result, data)) {
                Bridge.log(2, "protocol: add_entity re-encoded (entity=" + newId + ", y=" + newY
                        + ")");
            }
            return result;
        } catch (Throwable broken) {
            Throwable cause = broken.getCause() == null ? broken : broken.getCause();
            Bridge.log(1, "protocol: add_entity bridge failed: " + cause);
            return data;
        }
    }

    /** Field-level for {@code ClientboundBlockUpdatePacket}. */
    private static byte[] blockUpdate(org.bukkit.entity.Player player, Object type, byte[] data) {
        try {
            int[] pos = {0};
            int id = readVarInt(data, pos);
            java.io.DataInputStream in = new java.io.DataInputStream(
                    new java.io.ByteArrayInputStream(data, pos[0], data.length - pos[0]));
            long packed = in.readLong();
            int stateId = readVarInt(in);
            net.minecraft.core.BlockPos blockPos = net.minecraft.core.BlockPos.of(packed);
            Class<?> packetClass = Class.forName(
                    "net.minecraft.network.protocol.game.ClientboundBlockUpdatePacket", true,
                    type.getClass().getClassLoader());
            Object packet = packetClass.getConstructor(net.minecraft.core.BlockPos.class,
                    int.class).newInstance(blockPos, stateId);
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
            net.minecraft.core.BlockPos newPos =
                    (net.minecraft.core.BlockPos) packetClass.getMethod("getPos").invoke(packet);
            int newState = (Integer) packetClass.getMethod("stateId").invoke(packet);
            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream(data.length);
            writeVarInt(bytes, id);
            java.io.DataOutputStream out = new java.io.DataOutputStream(bytes);
            out.writeLong(newPos.asLong());
            out.flush();
            writeVarInt(bytes, newState);
            out.flush();
            byte[] result = bytes.toByteArray();
            if (!java.util.Arrays.equals(result, data)) {
                Bridge.log(2, "protocol: block_update re-encoded (pos=" + newPos.getX() + ","
                        + newPos.getY() + "," + newPos.getZ() + " state=" + newState + ")");
            }
            return result;
        } catch (Throwable broken) {
            Throwable cause = broken.getCause() == null ? broken : broken.getCause();
            Bridge.log(1, "protocol: block_update bridge failed: " + cause);
            return data;
        }
    }

    /** Прогоняет пакет через ProtocolLib; true — событие отменено. */
    private static boolean fireEvent(org.bukkit.entity.Player player, Object type, Object packet)
            throws Exception {
        Object container = containerConstructor.newInstance(type, packet);
        Object event = fromServer.invoke(null, plugin, container, player);
        for (Object listener : (Set<?>) getListeners.invoke(manager)) {
            Set<?> types = (Set<?>) getTypes.invoke(getSendingWhitelist.invoke(listener));
            if (types.contains(type)) {
                onSending.invoke(listener, event);
            }
        }
        return Boolean.TRUE.equals(event.getClass().getMethod("isCancelled").invoke(event));
    }

    /**
     * SET_EQUIPMENT: настоящий носитель с `getSlots()` + перекодирование.
     * Сложные компоненты — passthrough через RawPayload (прежняя семантика).
     */
    private static byte[] setEquipment(org.bukkit.entity.Player player, Object type, byte[] data) {
        try {
            int[] pos = {0};
            int id = readVarInt(data, pos);
            java.util.List<EquipmentWire.RawSlot> raw = EquipmentWire.parseSlots(data, pos[0]);
            if (raw == null) {
                return rawEvent(player, type, data);
            }
            if (EQUIPMENT_LOGGED.compareAndSet(false, true)) {
                Bridge.log(2, "protocol: set_equipment parsed (slots=" + raw.size()
                        + ", field-level ready)");
            }
            int entity = readEntity(data, pos[0]);
            java.util.List<com.mojang.datafixers.util.Pair<
                    net.minecraft.world.entity.EquipmentSlot,
                    net.minecraft.world.item.ItemStack>> slots = new java.util.ArrayList<>();
            for (EquipmentWire.RawSlot slot : raw) {
                net.minecraft.world.item.ItemStack stack =
                        new net.minecraft.world.item.ItemStack(nameOf(slot.itemId()), slot.count());
                stack.rawId = slot.itemId();
                stack.patch = slot.patch();
                slots.add(com.mojang.datafixers.util.Pair.of(
                        net.minecraft.world.entity.EquipmentSlot.byId(slot.slot()), stack));
            }
            net.minecraft.network.protocol.game.ClientboundSetEquipmentPacket packet =
                    new net.minecraft.network.protocol.game.ClientboundSetEquipmentPacket(entity, slots);
            if (fireEvent(player, type, packet)) {
                return null;
            }
            java.util.List<com.mojang.datafixers.util.Pair<
                    net.minecraft.world.entity.EquipmentSlot,
                    net.minecraft.world.item.ItemStack>> after = packet.getSlots();
            java.util.List<EquipmentWire.RawSlot> out = new java.util.ArrayList<>();
            for (com.mojang.datafixers.util.Pair<net.minecraft.world.entity.EquipmentSlot,
                    net.minecraft.world.item.ItemStack> pair : after) {
                net.minecraft.world.item.ItemStack stack = pair.getSecond();
                int slot = pair.getFirst() == null ? 0 : pair.getFirst().ordinal();
                int count = stack == null ? 0 : stack.getCount();
                int itemId = stack == null ? -1 : stack.rawId;
                byte[] patch = stack == null ? new byte[0] : stack.patch;
                out.add(new EquipmentWire.RawSlot(slot, count, itemId, patch));
            }
            byte[] body = EquipmentWire.encode(packet.getEntity(), out);
            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream(body.length + 5);
            writeVarInt(bytes, id);
            bytes.write(body, 0, body.length);
            byte[] result = bytes.toByteArray();
            if (!java.util.Arrays.equals(result, data)) {
                Bridge.log(2, "protocol: set_equipment re-encoded (slots=" + out.size()
                        + ", modified=true)");
            }
            return result;
        } catch (Throwable broken) {
            Bridge.log(1, "protocol: set_equipment fallback: " + broken);
            return rawEvent(player, type, data);
        }
    }

    private static String nameOf(int itemId) {
        String name = Bridge.world("itemname " + itemId);
        return name == null || name.isBlank() ? "minecraft:air" : name.trim();
    }

    private static int readEntity(byte[] data, int offset) {
        int[] pos = {offset};
        return readVarInt(data, pos);
    }

    /** Прежняя семантика: RawPayload-событие (отмена/замена данных). */
    private static byte[] rawEvent(org.bukkit.entity.Player player, Object type, byte[] data) {
        try {
            Object container = containerConstructor.newInstance(type, new RawPayload(data));
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
            Object handle = container.getClass().getMethod("getHandle").invoke(container);
            if (handle instanceof RawPayload rawPayload && rawPayload.data != null) {
                return rawPayload.data;
            }
            return data;
        } catch (Throwable broken) {
            Bridge.log(1, "protocol: set_equipment raw event failed: " + broken);
            return data;
        }
    }

    private static final java.util.concurrent.atomic.AtomicBoolean EQUIPMENT_LOGGED =
            new java.util.concurrent.atomic.AtomicBoolean(false);

    private static void staticMap() {
        // прогрев таблицы component id (нужна парсеру)
        ComponentIds.count();
    }

    /** Field-level for {@code ClientboundPlayerChatPacket}. */
    private static byte[] playerChat(org.bukkit.entity.Player player, Object type, byte[] data) {
        try {
            int[] pos = {0};
            int id = readVarInt(data, pos);
            java.io.DataInputStream in = new java.io.DataInputStream(
                    new java.io.ByteArrayInputStream(data, pos[0], data.length - pos[0]));
            java.util.UUID sender = new java.util.UUID(in.readLong(), in.readLong());
            int index = readVarInt(in);
            byte[] signature = in.readBoolean() ? readFixed(in, 256) : null;
            String content = readString(in);
            long timestamp = in.readLong();
            long salt = in.readLong();
            int seenCount = readVarInt(in);
            java.util.List<Object> lastSeen = new java.util.ArrayList<>(seenCount);
            Class<?> packetClass = Class.forName(
                    "net.minecraft.network.protocol.game.ClientboundPlayerChatPacket", true,
                    type.getClass().getClassLoader());
            Class<?> seenClass = Class.forName(
                    "net.minecraft.network.protocol.game.ClientboundPlayerChatPacket$Seen", true,
                    type.getClass().getClassLoader());
            for (int i = 0; i < seenCount; i++) {
                java.util.UUID seenSender = new java.util.UUID(in.readLong(), in.readLong());
                lastSeen.add(seenClass.getConstructor(java.util.UUID.class, byte[].class)
                        .newInstance(seenSender, readFixed(in, 256)));
            }
            net.minecraft.nbt.Tag unsignedTag = null;
            net.minecraft.network.chat.Component unsignedContent = null;
            if (in.readBoolean()) {
                unsignedTag = net.minecraft.nbt.NbtIo.readNetworkTag(
                        in, net.minecraft.nbt.NbtAccounter.unlimitedHeap());
                unsignedContent = componentOf(unsignedTag);
            }
            int filterMaskType = readVarInt(in);
            if (filterMaskType == 2) {
                // Частичная фильтрация: битсет не моделируем, пропускаем как есть.
                Bridge.log(2, "protocol: player_chat partial-filter passthrough");
                return data;
            }
            int chatTypeId = readVarInt(in);
            net.minecraft.nbt.Tag chatNameTag = net.minecraft.nbt.NbtIo.readNetworkTag(
                    in, net.minecraft.nbt.NbtAccounter.unlimitedHeap());
            net.minecraft.network.chat.Component chatName = componentOf(chatNameTag);
            net.minecraft.nbt.Tag targetTag = in.readBoolean()
                    ? net.minecraft.nbt.NbtIo.readNetworkTag(
                            in, net.minecraft.nbt.NbtAccounter.unlimitedHeap())
                    : null;
            net.minecraft.network.chat.Component targetName =
                    targetTag == null ? null : componentOf(targetTag);
            Class<?> boundClass = Class.forName(
                    "net.minecraft.network.protocol.game.ClientboundPlayerChatPacket$ChatTypeBound",
                    true, type.getClass().getClassLoader());
            Object bound = boundClass.getConstructor(int.class,
                    net.minecraft.network.chat.Component.class,
                    net.minecraft.network.chat.Component.class)
                    .newInstance(chatTypeId, chatName, targetName);
            Object packet = packetClass.getConstructor(
                    java.util.UUID.class, int.class, byte[].class, String.class,
                    long.class, long.class, java.util.List.class,
                    net.minecraft.network.chat.Component.class, int.class, boundClass)
                    .newInstance(sender, index, signature, content, timestamp, salt, lastSeen,
                            unsignedContent, filterMaskType, bound);
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
            java.util.UUID changedSender =
                    (java.util.UUID) packetClass.getMethod("sender").invoke(packet);
            int changedIndex = (Integer) packetClass.getMethod("index").invoke(packet);
            byte[] changedSignature =
                    (byte[]) packetClass.getMethod("signature").invoke(packet);
            String changedContent = (String) packetClass.getMethod("content").invoke(packet);
            long changedTimestamp = (Long) packetClass.getMethod("timestamp").invoke(packet);
            long changedSalt = (Long) packetClass.getMethod("salt").invoke(packet);
            java.util.List<?> changedSeen =
                    (java.util.List<?>) packetClass.getMethod("lastSeen").invoke(packet);
            net.minecraft.network.chat.Component changedUnsigned =
                    (net.minecraft.network.chat.Component) packetClass
                            .getMethod("unsignedContent").invoke(packet);
            int changedFilter = (Integer) packetClass.getMethod("filterMaskType").invoke(packet);
            Object changedBound = packetClass.getMethod("chatType").invoke(packet);

            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream(data.length);
            writeVarInt(bytes, id);
            java.io.DataOutputStream out = new java.io.DataOutputStream(bytes);
            out.writeLong(changedSender.getMostSignificantBits());
            out.writeLong(changedSender.getLeastSignificantBits());
            writeVarInt(bytes, changedIndex);
            out.writeBoolean(changedSignature != null);
            if (changedSignature != null) {
                out.write(fixed(changedSignature, 256));
            }
            writeString(bytes, changedContent);
            out.writeLong(changedTimestamp);
            out.writeLong(changedSalt);
            writeVarInt(bytes, changedSeen.size());
            for (Object seen : changedSeen) {
                java.util.UUID seenSender = (java.util.UUID) seenClass.getMethod("sender")
                        .invoke(seen);
                out.writeLong(seenSender.getMostSignificantBits());
                out.writeLong(seenSender.getLeastSignificantBits());
                out.write(fixed((byte[]) seenClass.getMethod("signature").invoke(seen), 256));
            }
            out.writeBoolean(changedUnsigned != null);
            if (changedUnsigned != null) {
                writeComponentLike(out, changedUnsigned, unsignedTag, unsignedContent);
            }
            writeVarInt(bytes, changedFilter);
            writeVarInt(bytes, (Integer) boundClass.getMethod("chatType").invoke(changedBound));
            net.minecraft.network.chat.Component changedName =
                    (net.minecraft.network.chat.Component) boundClass.getMethod("name")
                            .invoke(changedBound);
            writeComponentLike(out, changedName, chatNameTag, chatName);
            net.minecraft.network.chat.Component changedTarget =
                    (net.minecraft.network.chat.Component) boundClass.getMethod("targetName")
                            .invoke(changedBound);
            out.writeBoolean(changedTarget != null);
            if (changedTarget != null) {
                writeComponentLike(out, changedTarget, targetTag, targetName);
            }
            out.flush();
            byte[] result = bytes.toByteArray();
            if (!java.util.Arrays.equals(result, data)) {
                Bridge.log(2, "protocol: player_chat re-encoded (text="
                        + (changedUnsigned == null ? changedContent : changedUnsigned.getString())
                        + ")");
            }
            return result;
        } catch (Throwable broken) {
            Throwable cause = broken.getCause() == null ? broken : broken.getCause();
            Bridge.log(1, "protocol: player_chat bridge failed: " + cause);
            return data;
        }
    }

    /** Field-level for {@code ClientboundPlayerInfoUpdatePacket}. */
    private static byte[] playerInfo(org.bukkit.entity.Player player, Object type, byte[] data) {
        try {
            int[] pos = {0};
            int id = readVarInt(data, pos);
            java.io.DataInputStream in = new java.io.DataInputStream(
                    new java.io.ByteArrayInputStream(data, pos[0], data.length - pos[0]));
            int actionBits = in.readUnsignedByte();
            if ((actionBits & 2) != 0) {
                // INITIALIZE_CHAT: chat session data is not modelled yet; pass through.
                Bridge.log(2, "protocol: player_info init-chat passthrough");
                return data;
            }
            int count = readVarInt(in);
            Class<?> packetClass = Class.forName(
                    "net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket", true,
                    type.getClass().getClassLoader());
            Class<?> entryClass = Class.forName(
                    "net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket$Entry",
                    true, type.getClass().getClassLoader());
            Class<?> actionClass = Class.forName(
                    "net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket$Action",
                    true, type.getClass().getClassLoader());
            @SuppressWarnings({"unchecked", "rawtypes"})
            java.util.EnumSet actions = java.util.EnumSet.noneOf((Class) actionClass);
            for (Object constant : actionClass.getEnumConstants()) {
                if ((actionBits & (1 << ((Enum<?>) constant).ordinal())) != 0) {
                    @SuppressWarnings("unchecked")
                    java.util.EnumSet raw = actions;
                    raw.add((Enum) constant);
                }
            }
            java.util.List<Object> entries = new java.util.ArrayList<>(count);
            for (int i = 0; i < count; i++) {
                java.util.UUID uuid = new java.util.UUID(in.readLong(), in.readLong());
                String name = "";
                com.mojang.authlib.properties.PropertyMap properties =
                        new com.mojang.authlib.properties.PropertyMap(
                                com.google.common.collect.ArrayListMultimap.create());
                if ((actionBits & 1) != 0) {
                    name = readString(in);
                    int props = readVarInt(in);
                    for (int j = 0; j < props; j++) {
                        String key = readString(in);
                        String value = readString(in);
                        String signature = in.readBoolean() ? readString(in) : null;
                        properties.put(key, new com.mojang.authlib.properties.Property(
                                key, value, signature));
                    }
                }
                int gameMode = (actionBits & 4) != 0 ? readVarInt(in) : 0;
                boolean listed = (actionBits & 8) == 0 || in.readBoolean();
                int latency = (actionBits & 16) != 0 ? readVarInt(in) : 0;
                net.minecraft.network.chat.Component displayName = null;
                if ((actionBits & 32) != 0 && in.readBoolean()) {
                    net.minecraft.nbt.Tag root = net.minecraft.nbt.NbtIo.readNetworkTag(
                            in, net.minecraft.nbt.NbtAccounter.unlimitedHeap());
                    String text = root instanceof net.minecraft.nbt.CompoundTag tag
                            ? tag.getStringOr("text", null)
                            : root.getAsString();
                    displayName = net.minecraft.network.chat.Component.literal(
                            text == null ? "" : text);
                }
                int listOrder = (actionBits & 64) != 0 ? readVarInt(in) : 0;
                boolean showHat = (actionBits & 128) == 0 || in.readBoolean();
                com.mojang.authlib.GameProfile profile =
                        new com.mojang.authlib.GameProfile(uuid, name, properties);
                Object entry = entryClass.getConstructor(
                        java.util.UUID.class, com.mojang.authlib.GameProfile.class,
                        boolean.class, int.class, net.minecraft.world.level.GameType.class,
                        net.minecraft.network.chat.Component.class, boolean.class, int.class,
                        net.minecraft.network.chat.RemoteChatSession.Data.class)
                        .newInstance(uuid, profile, listed, latency,
                                net.minecraft.world.level.GameType.byId(gameMode), displayName,
                                showHat, listOrder, null);
                entries.add(entry);
            }
            Object packet = packetClass.getConstructor(java.util.EnumSet.class, java.util.List.class)
                    .newInstance(actions, entries);
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
            java.util.EnumSet<?> changedActions =
                    (java.util.EnumSet<?>) packetClass.getMethod("actions").invoke(packet);
            java.util.List<?> changedEntries =
                    (java.util.List<?>) packetClass.getMethod("entries").invoke(packet);
            int newBits = 0;
            for (Object constant : changedActions) {
                newBits |= 1 << ((Enum<?>) constant).ordinal();
            }
            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream(data.length);
            writeVarInt(bytes, id);
            bytes.write(newBits);
            writeVarInt(bytes, changedEntries.size());
            java.io.DataOutputStream out = new java.io.DataOutputStream(bytes);
            for (Object entry : changedEntries) {
                java.util.UUID uuid = (java.util.UUID) entryClass.getMethod("profileId")
                        .invoke(entry);
                out.writeLong(uuid.getMostSignificantBits());
                out.writeLong(uuid.getLeastSignificantBits());
                com.mojang.authlib.GameProfile profile =
                        (com.mojang.authlib.GameProfile) entryClass.getMethod("profile")
                                .invoke(entry);
                if ((newBits & 1) != 0) {
                    writeString(bytes, profile == null ? "" : profile.getName());
                    com.google.common.collect.Multimap<String,
                            com.mojang.authlib.properties.Property> props =
                            profile == null ? com.google.common.collect.ArrayListMultimap.create()
                                    : profile.getProperties();
                    writeVarInt(bytes, props.size());
                    for (Object value : props.values()) {
                        com.mojang.authlib.properties.Property property =
                                (com.mojang.authlib.properties.Property) value;
                        writeString(bytes, property.getName());
                        writeString(bytes, property.getValue());
                        out.writeBoolean(property.getSignature() != null);
                        if (property.getSignature() != null) {
                            writeString(bytes, property.getSignature());
                        }
                    }
                }
                if ((newBits & 4) != 0) {
                    net.minecraft.world.level.GameType mode =
                            (net.minecraft.world.level.GameType) entryClass.getMethod("gameMode")
                                    .invoke(entry);
                    writeVarInt(bytes, mode == null ? 0 : mode.getId());
                }
                if ((newBits & 8) != 0) {
                    out.writeBoolean((Boolean) entryClass.getMethod("listed").invoke(entry));
                }
                if ((newBits & 16) != 0) {
                    writeVarInt(bytes, (Integer) entryClass.getMethod("latency").invoke(entry));
                }
                if ((newBits & 32) != 0) {
                    net.minecraft.network.chat.Component display =
                            (net.minecraft.network.chat.Component) entryClass
                                    .getMethod("displayName").invoke(entry);
                    out.writeBoolean(display != null);
                    if (display != null) {
                        net.minecraft.nbt.CompoundTag tag = new net.minecraft.nbt.CompoundTag();
                        tag.putString("text", display.getString());
                        net.minecraft.nbt.NbtIo.writeNetwork(tag, out);
                    }
                }
                if ((newBits & 64) != 0) {
                    writeVarInt(bytes, (Integer) entryClass.getMethod("listOrder").invoke(entry));
                }
                if ((newBits & 128) != 0) {
                    out.writeBoolean((Boolean) entryClass.getMethod("showHat").invoke(entry));
                }
            }
            out.flush();
            byte[] result = bytes.toByteArray();
            if (!java.util.Arrays.equals(result, data)) {
                int firstLatency = changedEntries.isEmpty() ? -1
                        : (Integer) entryClass.getMethod("latency").invoke(changedEntries.get(0));
                Bridge.log(2, "protocol: player_info re-encoded (" + changedEntries.size()
                        + " entries, latency=" + firstLatency + ")");
            }
            return result;
        } catch (Throwable broken) {
            Throwable cause = broken.getCause() == null ? broken : broken.getCause();
            Bridge.log(1, "protocol: player_info bridge failed: " + cause);
            return data;
        }
    }

    /** Field-level для {@code ClientboundSetEntityDataPacket}. */
    private static byte[] entityData(org.bukkit.entity.Player player, Object type, byte[] data) {
        try {
            int[] pos = {0};
            int id = readVarInt(data, pos);
            java.io.DataInputStream in = new java.io.DataInputStream(
                    new java.io.ByteArrayInputStream(data, pos[0], data.length - pos[0]));
            int entityId = readVarInt(in);
            byte[] rest = in.readAllBytes();
            Object packet = Class.forName(
                    "net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket", true,
                    type.getClass().getClassLoader())
                    .getConstructor(int.class, byte[].class)
                    .newInstance(entityId, rest);
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
            int newId = (Integer) packet.getClass().getMethod("id").invoke(packet);
            byte[] values = (byte[]) packet.getClass().getMethod("packedItems").invoke(packet);
            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream(data.length);
            writeVarInt(bytes, id);
            writeVarInt(bytes, newId);
            if (values != null) {
                bytes.write(values);
            }
            byte[] result = bytes.toByteArray();
            if (!java.util.Arrays.equals(result, data)) {
                Bridge.log(2, "protocol: entity_data re-encoded (entity=" + newId + ")");
            }
            return result;
        } catch (Throwable broken) {
            Throwable cause = broken.getCause() == null ? broken : broken.getCause();
            Bridge.log(1, "protocol: entity_data bridge failed: " + cause);
            return data;
        }
    }

    /** Field-level для {@code ClientboundForgetLevelChunkPacket}. */
    private static byte[] forgetChunk(org.bukkit.entity.Player player, Object type, byte[] data) {
        try {
            int[] pos = {0};
            int id = readVarInt(data, pos);
            java.io.DataInputStream in = new java.io.DataInputStream(
                    new java.io.ByteArrayInputStream(data, pos[0], data.length - pos[0]));
            long chunkPos = in.readLong();
            Object packet = Class.forName(
                    "net.minecraft.network.protocol.game.ClientboundForgetLevelChunkPacket", true,
                    type.getClass().getClassLoader())
                    .getConstructor(long.class)
                    .newInstance(chunkPos);
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
            long newPos = (Long) packet.getClass().getMethod("pos").invoke(packet);
            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream(data.length);
            writeVarInt(bytes, id);
            new java.io.DataOutputStream(bytes).writeLong(newPos);
            byte[] result = bytes.toByteArray();
            if (!java.util.Arrays.equals(result, data)) {
                Bridge.log(2, "protocol: forget_chunk re-encoded (pos=" + newPos + ")");
            }
            return result;
        } catch (Throwable broken) {
            Throwable cause = broken.getCause() == null ? broken : broken.getCause();
            Bridge.log(1, "protocol: forget_chunk bridge failed: " + cause);
            return data;
        }
    }

    /** Field-level для {@code ClientboundSoundPacket}. */
    private static byte[] sound(org.bukkit.entity.Player player, Object type, byte[] data) {
        try {
            int[] pos = {0};
            int id = readVarInt(data, pos);
            java.io.DataInputStream in = new java.io.DataInputStream(
                    new java.io.ByteArrayInputStream(data, pos[0], data.length - pos[0]));
            int soundId = readVarInt(in);
            int source = readVarInt(in);
            int x = in.readInt();
            int y = in.readInt();
            int z = in.readInt();
            float volume = in.readFloat();
            float pitch = in.readFloat();
            long seed = in.readLong();
            Object packet = Class.forName(
                    "net.minecraft.network.protocol.game.ClientboundSoundPacket", true,
                    type.getClass().getClassLoader())
                    .getConstructor(int.class, int.class, int.class, int.class, int.class,
                            float.class, float.class, long.class)
                    .newInstance(soundId, source, x, y, z, volume, pitch, seed);
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
            int newSound = (Integer) packet.getClass().getMethod("soundId").invoke(packet);
            int newSource = (Integer) packet.getClass().getMethod("source").invoke(packet);
            int newX = (Integer) packet.getClass().getMethod("x").invoke(packet);
            int newY = (Integer) packet.getClass().getMethod("y").invoke(packet);
            int newZ = (Integer) packet.getClass().getMethod("z").invoke(packet);
            float newVolume = (Float) packet.getClass().getMethod("volume").invoke(packet);
            float newPitch = (Float) packet.getClass().getMethod("pitch").invoke(packet);
            long newSeed = (Long) packet.getClass().getMethod("seed").invoke(packet);
            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream(data.length);
            writeVarInt(bytes, id);
            writeVarInt(bytes, newSound);
            writeVarInt(bytes, newSource);
            java.io.DataOutputStream out = new java.io.DataOutputStream(bytes);
            out.writeInt(newX);
            out.writeInt(newY);
            out.writeInt(newZ);
            out.writeFloat(newVolume);
            out.writeFloat(newPitch);
            out.writeLong(newSeed);
            out.flush();
            byte[] result = bytes.toByteArray();
            if (!java.util.Arrays.equals(result, data)) {
                Bridge.log(2, "protocol: sound re-encoded (volume=" + newVolume + ")");
            }
            return result;
        } catch (Throwable broken) {
            Throwable cause = broken.getCause() == null ? broken : broken.getCause();
            Bridge.log(1, "protocol: sound field bridge failed: " + cause);
            return data;
        }
    }

    /** Field-level для {@code ClientboundSetTitleTextPacket}. */
    private static byte[] title(org.bukkit.entity.Player player, Object type, byte[] data) {
        try {
            int[] pos = {0};
            int id = readVarInt(data, pos);
            java.io.DataInputStream in = new java.io.DataInputStream(
                    new java.io.ByteArrayInputStream(data, pos[0], data.length - pos[0]));
            net.minecraft.nbt.Tag root = net.minecraft.nbt.NbtIo.readNetworkTag(
                    in, net.minecraft.nbt.NbtAccounter.unlimitedHeap());
            String text = root instanceof net.minecraft.nbt.CompoundTag tag
                    ? tag.getStringOr("text", null)
                    : root.getAsString();
            net.minecraft.network.chat.Component content = text != null
                    ? net.minecraft.network.chat.Component.literal(text)
                    : net.minecraft.network.chat.Component.literal("");
            Object packet = Class.forName(
                    "net.minecraft.network.protocol.game.ClientboundSetTitleTextPacket", true,
                    type.getClass().getClassLoader())
                    .getConstructor(net.minecraft.network.chat.Component.class)
                    .newInstance(content);
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
            net.minecraft.network.chat.Component changed =
                    (net.minecraft.network.chat.Component) packet.getClass()
                            .getMethod("text").invoke(packet);
            net.minecraft.nbt.CompoundTag outTag = new net.minecraft.nbt.CompoundTag();
            outTag.putString("text", changed == null ? "" : changed.getString());
            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream(data.length);
            writeVarInt(bytes, id);
            net.minecraft.nbt.NbtIo.writeNetwork(outTag, new java.io.DataOutputStream(bytes));
            byte[] result = bytes.toByteArray();
            if (!java.util.Arrays.equals(result, data)) {
                Bridge.log(2, "protocol: title re-encoded (text="
                        + (changed == null ? "" : changed.getString()) + ")");
            }
            return result;
        } catch (Throwable broken) {
            Throwable cause = broken.getCause() == null ? broken : broken.getCause();
            Bridge.log(1, "protocol: title field bridge failed: " + cause);
            return data;
        }
    }

    /** Real-field путь для {@code ClientboundSystemChatPacket}. */
    private static byte[] systemChat(org.bukkit.entity.Player player, Object type, byte[] data) {
        try {
            int[] pos = {0};
            int id = readVarInt(data, pos);
            java.io.DataInputStream in = new java.io.DataInputStream(
                    new java.io.ByteArrayInputStream(data, pos[0], data.length - pos[0]));
            net.minecraft.nbt.Tag root = net.minecraft.nbt.NbtIo.readNetworkTag(
                    in, net.minecraft.nbt.NbtAccounter.unlimitedHeap());
            boolean overlay = in.readBoolean();
            String text = null;
            String translate = null;
            if (root instanceof net.minecraft.nbt.CompoundTag tag) {
                text = tag.getStringOr("text", null);
                translate = tag.getStringOr("translate", null);
            } else if (root instanceof net.minecraft.nbt.StringTag string) {
                text = string.getAsString();
            } else {
                text = root.getAsString();
            }
            net.minecraft.network.chat.Component content = text != null
                    ? net.minecraft.network.chat.Component.literal(text)
                    : translate != null
                            ? net.minecraft.network.chat.Component.translatable(translate)
                            : net.minecraft.network.chat.Component.literal(root.toString());
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
            net.minecraft.nbt.Tag outTag = changed == content && root != null
                    ? root
                    : literalTag(changed);
            net.minecraft.nbt.NbtIo.writeNetworkTag(outTag,
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

    private static byte[] readFixed(java.io.DataInputStream in, int length)
            throws java.io.IOException {
        byte[] bytes = new byte[length];
        in.readFully(bytes);
        return bytes;
    }

    private static byte[] fixed(byte[] value, int length) {
        byte[] bytes = new byte[length];
        if (value != null) {
            System.arraycopy(value, 0, bytes, 0, Math.min(value.length, length));
        }
        return bytes;
    }

    private static net.minecraft.network.chat.Component componentOf(net.minecraft.nbt.Tag root) {
        String text = null;
        if (root instanceof net.minecraft.nbt.CompoundTag tag) {
            text = tag.getStringOr("text", null);
        } else if (root instanceof net.minecraft.nbt.StringTag string) {
            text = string.getAsString();
        } else if (root != null) {
            text = root.getAsString();
        }
        return net.minecraft.network.chat.Component.literal(text == null ? "" : text);
    }

    private static net.minecraft.nbt.CompoundTag literalTag(
            net.minecraft.network.chat.Component component) {
        net.minecraft.nbt.CompoundTag tag = new net.minecraft.nbt.CompoundTag();
        tag.putString("text", component == null ? "" : component.getString());
        return tag;
    }

    /** Неизменённый компонент пишем исходным тегом (translate/extra сохраняются). */
    private static void writeComponentLike(java.io.DataOutputStream out,
            net.minecraft.network.chat.Component component, net.minecraft.nbt.Tag original,
            net.minecraft.network.chat.Component decoded) throws java.io.IOException {
        if (component == decoded && original != null) {
            net.minecraft.nbt.NbtIo.writeNetworkTag(original, out);
        } else {
            writeComponent(out, component);
        }
    }

    private static net.minecraft.network.chat.Component readComponent(java.io.DataInputStream in)
            throws java.io.IOException {
        return componentOf(net.minecraft.nbt.NbtIo.readNetworkTag(
                in, net.minecraft.nbt.NbtAccounter.unlimitedHeap()));
    }

    private static void writeComponent(java.io.DataOutputStream out,
            net.minecraft.network.chat.Component component) throws java.io.IOException {
        net.minecraft.nbt.CompoundTag tag = new net.minecraft.nbt.CompoundTag();
        tag.putString("text", component == null ? "" : component.getString());
        net.minecraft.nbt.NbtIo.writeNetwork(tag, out);
    }

    private static String readString(java.io.DataInputStream in) throws java.io.IOException {
        int length = readVarInt(in);
        byte[] bytes = new byte[length];
        in.readFully(bytes);
        return new String(bytes, java.nio.charset.StandardCharsets.UTF_8);
    }

    private static void writeString(java.io.ByteArrayOutputStream out, String value) {
        byte[] bytes = (value == null ? "" : value)
                .getBytes(java.nio.charset.StandardCharsets.UTF_8);
        writeVarInt(out, bytes.length);
        out.write(bytes, 0, bytes.length);
    }

    private static long readVarLong(java.io.DataInputStream in) throws java.io.IOException {
        long value = 0L;
        int shift = 0;
        while (true) {
            byte b = in.readByte();
            value |= (long) (b & 0x7f) << shift;
            if ((b & 0x80) == 0) {
                return value;
            }
            shift += 7;
        }
    }

    private static void writeVarLong(java.io.ByteArrayOutputStream out, long value) {
        while ((value & ~0x7FL) != 0L) {
            out.write((int) (value & 0x7F) | 0x80);
            value >>>= 7;
        }
        out.write((int) value);
    }

    private static int readVarInt(java.io.DataInputStream in) throws java.io.IOException {
        int value = 0;
        int shift = 0;
        while (true) {
            byte b = in.readByte();
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
        Bridge.log(2, "protocol: component ids=" + ComponentIds.count());
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
