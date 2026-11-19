package net.minecraft.network.chat;

/** NMS {@code RemoteChatSession} (+ Data) для PlayerInfoUpdate. */
public class RemoteChatSession {
    public static final class Data {
        private final java.util.UUID sessionId;

        public Data(java.util.UUID sessionId) {
            this.sessionId = sessionId;
        }

        public java.util.UUID sessionId() {
            return this.sessionId;
        }
    }
}
