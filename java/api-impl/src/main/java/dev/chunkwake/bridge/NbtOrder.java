package dev.chunkwake.bridge;

import java.io.ByteArrayOutputStream;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

/** Reorders root NBT keys to the order emitted by Paper's ItemStack.CODEC. */
final class NbtOrder {
    private NbtOrder() {
    }

    private record Entry(String name, byte[] bytes) {
    }

    static byte[] reorderRoot(byte[] nbt) {
        try {
            Reader in = new Reader(nbt);
            if (in.u8() != 10) {
                return nbt;
            }
            int nameLen = in.u16();
            in.skip(nameLen);
            List<Entry> entries = new ArrayList<>();
            while (true) {
                int type = in.u8();
                if (type == 0) {
                    break;
                }
                int start = in.pos - 1;
                int len = in.u16();
                String name = new String(nbt, in.pos, len, java.nio.charset.StandardCharsets.UTF_8);
                in.skip(len);
                in.skipPayload(type);
                byte[] bytes = java.util.Arrays.copyOfRange(nbt, start, in.pos);
                entries.add(new Entry(name, bytes));
            }
            List<String> order = List.of("DataVersion", "id", "count", "components");
            entries.sort(Comparator.comparingInt(e -> {
                int index = order.indexOf(e.name());
                return index < 0 ? order.size() : index;
            }));
            ByteArrayOutputStream out = new ByteArrayOutputStream(nbt.length);
            out.write(10);
            out.write(0);
            out.write(0);
            for (Entry entry : entries) {
                out.writeBytes(entry.bytes());
            }
            out.write(0);
            return out.toByteArray();
        } catch (RuntimeException error) {
            // Unknown NBT shape: keep the original bytes.
            return nbt;
        }
    }

    private static final class Reader {
        private final byte[] data;
        private int pos;

        Reader(byte[] data) {
            this.data = data;
        }

        int u8() {
            if (this.pos >= this.data.length) {
                throw new IllegalArgumentException("nbt eof");
            }
            return this.data[this.pos++] & 0xFF;
        }

        int u16() {
            return (u8() << 8) | u8();
        }

        int i32() {
            return (u8() << 24) | (u8() << 16) | (u8() << 8) | u8();
        }

        void skip(int n) {
            if (n < 0 || this.pos + n > this.data.length) {
                throw new IllegalArgumentException("nbt skip");
            }
            this.pos += n;
        }

        void skipPayload(int type) {
            switch (type) {
                case 1 -> skip(1);
                case 2 -> skip(2);
                case 3, 5 -> skip(4);
                case 4, 6 -> skip(8);
                case 7 -> skip(i32());
                case 8 -> skip(u16());
                case 9 -> {
                    int element = u8();
                    int count = i32();
                    for (int i = 0; i < count; i++) {
                        skipPayload(element);
                    }
                }
                case 10 -> {
                    while (true) {
                        int child = u8();
                        if (child == 0) {
                            break;
                        }
                        skip(u16());
                        skipPayload(child);
                    }
                }
                case 11 -> {
                    int count = i32();
                    skip(Math.multiplyExact(count, 4));
                }
                case 12 -> {
                    int count = i32();
                    skip(Math.multiplyExact(count, 8));
                }
                default -> throw new IllegalArgumentException("nbt tag " + type);
            }
        }
    }
}
