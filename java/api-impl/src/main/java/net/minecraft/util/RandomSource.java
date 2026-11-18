package net.minecraft.util;

import java.util.Random;

/** NMS {@code RandomSource} (тонкая обёртка над java.util.Random). */
public interface RandomSource {
    int nextInt();

    int nextInt(int bound);

    long nextLong();

    float nextFloat();

    double nextDouble();

    boolean nextBoolean();

    static RandomSource create() {
        return new Impl(new Random());
    }

    static RandomSource create(long seed) {
        return new Impl(new Random(seed));
    }

    final class Impl implements RandomSource {
        private final Random random;

        Impl(Random random) {
            this.random = random;
        }

        @Override
        public int nextInt() {
            return this.random.nextInt();
        }

        @Override
        public int nextInt(int bound) {
            return this.random.nextInt(bound);
        }

        @Override
        public long nextLong() {
            return this.random.nextLong();
        }

        @Override
        public float nextFloat() {
            return this.random.nextFloat();
        }

        @Override
        public double nextDouble() {
            return this.random.nextDouble();
        }

        @Override
        public boolean nextBoolean() {
            return this.random.nextBoolean();
        }
    }
}
