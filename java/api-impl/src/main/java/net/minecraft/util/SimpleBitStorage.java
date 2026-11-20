package net.minecraft.util;

/** NMS {@code SimpleBitStorage}: простая реализация {@link BitStorage}. */
public class SimpleBitStorage implements BitStorage {
    private final int bits;
    private final int size;
    private final long[] data;

    public SimpleBitStorage(int bits, int size) {
        this(bits, size, new long[words(bits, size)]);
    }

    public SimpleBitStorage(int bits, int size, long[] data) {
        if (bits < 0 || bits > 32) {
            throw new InitializationException("Invalid bit size " + bits);
        }
        int expected = words(bits, size);
        if (data.length < expected) {
            throw new InitializationException(
                    "Expected " + expected + " longs, got " + data.length);
        }
        this.bits = bits;
        this.size = size;
        this.data = data;
    }

    private static int words(int bits, int size) {
        if (bits == 0) {
            return 0;
        }
        return (int) (((long) size * bits + 63L) / 64L);
    }

    @Override
    public int get(int index) {
        if (this.bits == 0) {
            return 0;
        }
        long bit = (long) index * this.bits;
        int start = (int) (bit >> 6);
        int offset = (int) (bit & 63L);
        long value = this.data[start] >>> offset;
        if (offset + this.bits > 64) {
            value |= this.data[start + 1] << (64 - offset);
        }
        return (int) (value & ((1L << this.bits) - 1L));
    }

    @Override
    public void set(int index, int value) {
        if (this.bits == 0) {
            return;
        }
        long bit = (long) index * this.bits;
        int start = (int) (bit >> 6);
        int offset = (int) (bit & 63L);
        long mask = ((1L << this.bits) - 1L) << offset;
        this.data[start] = (this.data[start] & ~mask) | ((long) value << offset) & mask;
        if (offset + this.bits > 64) {
            int rest = offset + this.bits - 64;
            long highMask = (1L << rest) - 1L;
            this.data[start + 1] = (this.data[start + 1] & ~highMask)
                    | ((long) value >>> (64 - offset)) & highMask;
        }
    }

    @Override
    public long[] getRaw() {
        return this.data;
    }

    @Override
    public int getSize() {
        return this.size;
    }

    /** NMS {@code SimpleBitStorage.InitializationException}. */
    public static final class InitializationException extends IllegalStateException {
        public InitializationException(String message) {
            super(message);
        }
    }
}
