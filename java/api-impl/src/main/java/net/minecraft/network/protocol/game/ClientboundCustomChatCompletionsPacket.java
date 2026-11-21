package net.minecraft.network.protocol.game;

import java.util.List;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundCustomChatCompletionsPacket} (CraftEngine). */
public class ClientboundCustomChatCompletionsPacket implements Packet {
    /** NMS {@code ClientboundCustomChatCompletionsPacket.Action}. */
    public enum Action {
        ADD,
        REMOVE,
        SET
    }

    private Action action;
    private List<String> entries = List.of();

    public ClientboundCustomChatCompletionsPacket() {
    }

    public ClientboundCustomChatCompletionsPacket(Action action, List<String> entries) {
        this.action = action;
        this.entries = entries == null ? List.of() : entries;
    }

    public Action getAction() {
        return this.action;
    }

    public List<String> getEntries() {
        return this.entries;
    }

    public void setEntries(List<String> entries) {
        this.entries = entries == null ? List.of() : entries;
    }
}
