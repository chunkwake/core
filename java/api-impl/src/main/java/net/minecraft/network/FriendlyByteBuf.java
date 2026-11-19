package net.minecraft.network;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.WrappedByteBuf;

/**
 * NMS {@code FriendlyByteBuf}/{@code PacketDataSerializer}: реальный
 * {@link ByteBuf}, чтобы плагины (Axiom) могли передавать его в методы,
 * ожидающие {@code io.netty.buffer.ByteBuf}.
 */
public class FriendlyByteBuf extends WrappedByteBuf {
    public FriendlyByteBuf(ByteBuf buf) {
        super(buf);
    }

    public ByteBuf source() {
        return this.buf;
    }

    public FriendlyByteBuf writeByte(int value) {
        this.buf.writeByte(value);
        return this;
    }

    public FriendlyByteBuf writeBoolean(boolean value) {
        this.buf.writeBoolean(value);
        return this;
    }

    public FriendlyByteBuf writeShort(int value) {
        this.buf.writeShort(value);
        return this;
    }

    public FriendlyByteBuf writeInt(int value) {
        this.buf.writeInt(value);
        return this;
    }

    public FriendlyByteBuf writeLong(long value) {
        this.buf.writeLong(value);
        return this;
    }

    public FriendlyByteBuf writeFloat(float value) {
        this.buf.writeFloat(value);
        return this;
    }

    public FriendlyByteBuf writeDouble(double value) {
        this.buf.writeDouble(value);
        return this;
    }

    /** `FriendlyByteBuf.writeVarIntArray` (DecentHolograms). */
    public void writeVarIntArray(int[] values) {
        this.writeVarInt(values.length);
        for (int value : values) {
            this.writeVarInt(value);
        }
    }

    public FriendlyByteBuf writeVarInt(int value) {
        while ((value & ~0x7F) != 0) {
            this.buf.writeByte((value & 0x7F) | 0x80);
            value >>>= 7;
        }
        this.buf.writeByte(value);
        return this;
    }

    public FriendlyByteBuf writeVarLong(long value) {
        while ((value & ~0x7FL) != 0L) {
            this.buf.writeByte((int) (value & 0x7F) | 0x80);
            value >>>= 7;
        }
        this.buf.writeByte((int) value);
        return this;
    }

    public FriendlyByteBuf writeUtf(String value) {
        byte[] bytes = value.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        this.writeVarInt(bytes.length);
        this.buf.writeBytes(bytes);
        return this;
    }

    public FriendlyByteBuf writeBytes(byte[] bytes) {
        this.buf.writeBytes(bytes);
        return this;
    }

    public FriendlyByteBuf writeUUID(java.util.UUID uuid) {
        this.buf.writeLong(uuid.getMostSignificantBits());
        this.buf.writeLong(uuid.getLeastSignificantBits());
        return this;
    }

    public int readVarInt() {
        int value = 0;
        int shift = 0;
        while (true) {
            byte b = this.buf.readByte();
            value |= (b & 0x7F) << shift;
            if ((b & 0x80) == 0) {
                return value;
            }
            shift += 7;
        }
    }

    public long readVarLong() {
        long value = 0L;
        int shift = 0;
        while (true) {
            byte b = this.buf.readByte();
            value |= (long) (b & 0x7F) << shift;
            if ((b & 0x80) == 0) {
                return value;
            }
            shift += 7;
        }
    }

    public String readUtf() {
        byte[] bytes = new byte[this.readVarInt()];
        this.buf.readBytes(bytes);
        return new String(bytes, java.nio.charset.StandardCharsets.UTF_8);
    }

    public java.util.UUID readUUID() {
        return new java.util.UUID(this.buf.readLong(), this.buf.readLong());
    }

}
