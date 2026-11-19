package net.minecraft.network.protocol.game;

/**
 * Заглушка NMS {@code ClientboundMoveEntityPacket} и его вариантов
 * (Pos/Rot/PosRot). NpcPlugin двигает NPC этими пакетами; в Chunkwake
 * движение собирается в Rust, поэтому нужны только классы.
 */
public class ClientboundMoveEntityPacket {
    public ClientboundMoveEntityPacket() {
    }

    public static class Pos extends ClientboundMoveEntityPacket {
        public Pos() {
        }
    }

    public static class Rot extends ClientboundMoveEntityPacket {
        public Rot() {
        }
    }

    public static class PosRot extends ClientboundMoveEntityPacket {
        public PosRot() {
        }
    }
}
