package net.minecraft.network.protocol.game;

/**
 * Заглушка NMS {@code ClientboundMoveEntityPacket} и его вариантов
 * (Pos/Rot/PosRot). NpcPlugin двигает NPC этими пакетами; в Chunkwake
 * движение собирается в Rust, поэтому нужны только классы.
 */
public class ClientboundMoveEntityPacket {
    /** Поля, которые ищет CraftEngine. */
    private int entityId;
    private short xa;
    private short ya;
    private short za;
    private byte yRot;
    private byte xRot;
    private boolean onGround;
    private boolean hasPos;
    /** Поле, которое ищет CraftEngine (getHasRot). */
    private boolean hasRot;

    public ClientboundMoveEntityPacket() {
    }

    public int getEntityId() { return this.entityId; }
    public void setEntityId(int entityId) { this.entityId = entityId; }
    public short getXa() { return this.xa; }
    public void setXa(short xa) { this.xa = xa; }
    public short getYa() { return this.ya; }
    public void setYa(short ya) { this.ya = ya; }
    public short getZa() { return this.za; }
    public void setZa(short za) { this.za = za; }
    public byte getYRot() { return this.yRot; }
    public void setYRot(byte yRot) { this.yRot = yRot; }
    public byte getXRot() { return this.xRot; }
    public void setXRot(byte xRot) { this.xRot = xRot; }
    public boolean getOnGround() { return this.onGround; }
    public void setOnGround(boolean onGround) { this.onGround = onGround; }
    public boolean getHasRot() { return this.hasRot; }
    public void setHasRot(boolean hasRot) { this.hasRot = hasRot; }
    public boolean getHasPos() { return this.hasPos; }
    public void setHasPos(boolean hasPos) { this.hasPos = hasPos; }

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
