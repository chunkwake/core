package net.minecraft.network.protocol.game;

import java.util.ArrayList;
import java.util.EnumSet;
import java.util.List;
import java.util.UUID;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.RemoteChatSession;
import net.minecraft.network.protocol.Packet;
import net.minecraft.world.level.GameType;

/** NMS {@code ClientboundPlayerInfoUpdatePacket} с реальными Entry. */
public class ClientboundPlayerInfoUpdatePacket implements Packet {
    public enum Action {
        ADD_PLAYER, INITIALIZE_CHAT, UPDATE_GAME_MODE, UPDATE_LISTED, UPDATE_LATENCY,
        UPDATE_DISPLAY_NAME, UPDATE_LIST_ORDER, UPDATE_HAT
    }

    private EnumSet<Action> actions = EnumSet.noneOf(Action.class);
    private List<Entry> entries = new ArrayList<>();

    public ClientboundPlayerInfoUpdatePacket() {
    }

    public ClientboundPlayerInfoUpdatePacket(EnumSet<Action> actions, List<Entry> entries) {
        this.actions = actions;
        this.entries = entries;
    }

    public EnumSet<Action> actions() {
        return this.actions;
    }

    public List<Entry> entries() {
        return this.entries;
    }

    /** Запись таблицы игроков (реальные поля NMS). */
    public static class Entry {
        private final UUID profileId;
        private final com.mojang.authlib.GameProfile profile;
        private final boolean listed;
        private final int latency;
        private final GameType gameMode;
        private final Component displayName;
        private final int listOrder;
        private final RemoteChatSession.Data chatSession;
        private final boolean showHat;

        public Entry(UUID profileId, com.mojang.authlib.GameProfile profile, boolean listed,
                int latency, GameType gameMode, Component displayName, int listOrder,
                RemoteChatSession.Data chatSession, boolean showHat) {
            this.profileId = profileId;
            this.profile = profile;
            this.listed = listed;
            this.latency = latency;
            this.gameMode = gameMode;
            this.displayName = displayName;
            this.listOrder = listOrder;
            this.chatSession = chatSession;
            this.showHat = showHat;
        }

        public UUID profileId() {
            return this.profileId;
        }

        public com.mojang.authlib.GameProfile profile() {
            return this.profile;
        }

        public boolean listed() {
            return this.listed;
        }

        public int latency() {
            return this.latency;
        }

        public GameType gameMode() {
            return this.gameMode;
        }

        public Component displayName() {
            return this.displayName;
        }

        public int listOrder() {
            return this.listOrder;
        }

        public RemoteChatSession.Data chatSession() {
            return this.chatSession;
        }

        public boolean showHat() {
            return this.showHat;
        }
    }
}
