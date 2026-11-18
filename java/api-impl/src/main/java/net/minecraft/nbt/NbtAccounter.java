package net.minecraft.nbt;

import java.util.concurrent.atomic.AtomicLong;

/** NMS {@code NbtAccounter}: лимит распаковки NBT. */
public interface NbtAccounter {
    long UNLIMITED = Long.MAX_VALUE;

    void accountBytes(long bytes);

    long getUsage();

    static NbtAccounter create(long limit) {
        return new Limited(limit);
    }

    static NbtAccounter unlimitedHeap() {
        return new Limited(UNLIMITED);
    }

    final class Limited implements NbtAccounter {
        private final long limit;
        private final AtomicLong usage = new AtomicLong();

        Limited(long limit) {
            this.limit = limit;
        }

        @Override
        public void accountBytes(long bytes) {
            long total = this.usage.addAndGet(Math.max(0, bytes));
            if (total > this.limit) {
                throw new RuntimeException("Tried to read NBT tag that was too big; tried to allocate: "
                        + total + " bytes where max allowed: " + this.limit);
            }
        }

        @Override
        public long getUsage() {
            return this.usage.get();
        }
    }
}
