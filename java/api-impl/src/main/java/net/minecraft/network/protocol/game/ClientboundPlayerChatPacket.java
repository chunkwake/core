package net.minecraft.network.protocol.game;

import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;

/**
 * NMS {@code ClientboundPlayerChatPacket} (unsigned-вариант).
 *
 * <p>Поля реальные: плагины через ProtocolLib видят компонент
 * ({@code getChatComponents()}) и могут его менять.
 */
public class ClientboundPlayerChatPacket implements Packet {
    private UUID sender;
    private int index;
    private byte[] signature;
    private String content = "";
    private long timestamp;
    private long salt;
    private List<Seen> lastSeen = new ArrayList<>();
    private Component unsignedContent;
    private int filterMaskType;
    private ChatTypeBound chatType;

    public ClientboundPlayerChatPacket() {
    }

    public ClientboundPlayerChatPacket(UUID sender, int index, byte[] signature, String content,
            long timestamp, long salt, List<Seen> lastSeen, Component unsignedContent,
            int filterMaskType, ChatTypeBound chatType) {
        this.sender = sender;
        this.index = index;
        this.signature = signature;
        this.content = content;
        this.timestamp = timestamp;
        this.salt = salt;
        this.lastSeen = lastSeen == null ? new ArrayList<>() : lastSeen;
        this.unsignedContent = unsignedContent;
        this.filterMaskType = filterMaskType;
        this.chatType = chatType;
    }

    public UUID sender() {
        return this.sender;
    }

    public int index() {
        return this.index;
    }

    public byte[] signature() {
        return this.signature;
    }

    public String content() {
        return this.content;
    }

    public long timestamp() {
        return this.timestamp;
    }

    public long salt() {
        return this.salt;
    }

    public List<Seen> lastSeen() {
        return this.lastSeen;
    }

    public Component unsignedContent() {
        return this.unsignedContent;
    }

    public int filterMaskType() {
        return this.filterMaskType;
    }

    public ChatTypeBound chatType() {
        return this.chatType;
    }

    /** Подпись ранее виденного сообщения. */
    public static class Seen {
        private final UUID sender;
        private final byte[] signature;

        public Seen(UUID sender, byte[] signature) {
            this.sender = sender;
            this.signature = signature;
        }

        public UUID sender() {
            return this.sender;
        }

        public byte[] signature() {
            return this.signature;
        }
    }

    /** Привязка к типу чата. */
    public static class ChatTypeBound {
        private final int chatType;
        private final Component name;
        private final Component targetName;

        public ChatTypeBound(int chatType, Component name, Component targetName) {
            this.chatType = chatType;
            this.name = name;
            this.targetName = targetName;
        }

        public int chatType() {
            return this.chatType;
        }

        public Component name() {
            return this.name;
        }

        public Component targetName() {
            return this.targetName;
        }
    }
}
