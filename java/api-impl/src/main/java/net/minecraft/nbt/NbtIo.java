package net.minecraft.nbt;

import java.io.DataInput;
import java.io.DataOutput;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;

/** Рабочий NMS {@code NbtIo}: бинарный NBT (без сжатия). */
public final class NbtIo {
    private NbtIo() {
    }

    public static CompoundTag read(DataInput input, NbtAccounter accounter) throws IOException {
        byte id = input.readByte();
        if (id == 0) {
            return new CompoundTag();
        }
        String name = input.readUTF();
        if (id != 10) {
            throw new IOException("Root tag must be a compound, got " + id + " (" + name + ")");
        }
        return readCompound(input, accounter);
    }

    public static void write(CompoundTag tag, DataOutput output) throws IOException {
        output.writeByte(10);
        output.writeUTF("");
        writeCompound(tag, output, new NbtAccounter.Limited(Long.MAX_VALUE));
    }

    public static CompoundTag readCompressed(InputStream input, NbtAccounter accounter) throws IOException {
        try (java.util.zip.GZIPInputStream gzip = new java.util.zip.GZIPInputStream(input)) {
            return read(new java.io.DataInputStream(gzip), accounter);
        }
    }

    public static void writeCompressed(CompoundTag tag, OutputStream output) throws IOException {
        try (java.util.zip.GZIPOutputStream gzip = new java.util.zip.GZIPOutputStream(output)) {
            java.io.DataOutputStream data = new java.io.DataOutputStream(gzip);
            write(tag, data);
            data.flush();
        }
    }

    private static CompoundTag readCompound(DataInput input, NbtAccounter accounter) throws IOException {
        CompoundTag tag = new CompoundTag();
        while (true) {
            byte id = input.readByte();
            if (id == 0) {
                return tag;
            }
            String name = input.readUTF();
            tag.put(name, readPayload(id, input, accounter));
        }
    }

    private static Tag readPayload(byte id, DataInput input, NbtAccounter accounter) throws IOException {
        return switch (id) {
            case 1 -> ByteTag.valueOf(input.readByte());
            case 2 -> ShortTag.valueOf(input.readShort());
            case 3 -> IntTag.valueOf(input.readInt());
            case 4 -> LongTag.valueOf(input.readLong());
            case 5 -> FloatTag.valueOf(input.readFloat());
            case 6 -> DoubleTag.valueOf(input.readDouble());
            case 7 -> {
                int length = input.readInt();
                accounter.accountBytes(length);
                byte[] bytes = new byte[length];
                input.readFully(bytes);
                yield new net.minecraft.nbt.ByteArrayTag(bytes);
            }
            case 8 -> StringTag.valueOf(input.readUTF());
            case 9 -> {
                byte element = input.readByte();
                int length = input.readInt();
                ListTag list = new ListTag();
                for (int i = 0; i < length; i++) {
                    list.add(readPayload(element, input, accounter));
                }
                yield list;
            }
            case 10 -> readCompound(input, accounter);
            case 11 -> {
                int length = input.readInt();
                long[] values = new long[length];
                for (int i = 0; i < length; i++) {
                    values[i] = input.readLong();
                }
                yield new net.minecraft.nbt.LongArrayTag(values);
            }
            default -> throw new IOException("Unknown NBT tag id " + id);
        };
    }

    private static void writeCompound(CompoundTag tag, DataOutput output, NbtAccounter accounter)
            throws IOException {
        for (String key : tag.keySet()) {
            Tag value = tag.get(key);
            if (value == null) {
                continue;
            }
            output.writeByte(value.getId());
            output.writeUTF(key);
            writePayload(value, output, accounter);
        }
        output.writeByte(0);
    }

    private static void writePayload(Tag value, DataOutput output, NbtAccounter accounter)
            throws IOException {
        if (value instanceof ByteTag t) {
            output.writeByte(t.getAsByte());
        } else if (value instanceof ShortTag t) {
            output.writeShort(t.getAsShort());
        } else if (value instanceof IntTag t) {
            output.writeInt(t.getAsInt());
        } else if (value instanceof LongTag t) {
            output.writeLong(t.getAsLong());
        } else if (value instanceof FloatTag t) {
            output.writeFloat(t.getAsFloat());
        } else if (value instanceof DoubleTag t) {
            output.writeDouble(t.getAsDouble());
        } else if (value instanceof StringTag t) {
            output.writeUTF(t.getAsString());
        } else if (value instanceof ByteArrayTag t) {
            output.writeInt(t.getAsByteArray().length);
            output.write(t.getAsByteArray());
        } else if (value instanceof ListTag list) {
            byte element = list.getElementType();
            output.writeByte(element);
            output.writeInt(list.size());
            for (Tag element0 : list) {
                writePayload(element0, output, accounter);
            }
        } else if (value instanceof CompoundTag compound) {
            writeCompound(compound, output, accounter);
        } else if (value instanceof LongArrayTag t) {
            output.writeInt(t.getAsLongArray().length);
            for (long element : t.getAsLongArray()) {
                output.writeLong(element);
            }
        }
    }
}
