package dev.chunkwake.bridge;

import java.util.Queue;
import java.util.UUID;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Consumer;
import org.bukkit.World;

/**
 * Регионный тик: несколько независимых петель по регионам чанков.
 *
 * <p>Регион — квадрат 8×8 чанков (как у Folia). Каждый воркер — отдельный
 * поток со своей очередью; регион всегда попадает в один и тот же воркер,
 * поэтому задачи одного региона выполняются последовательно, а разных —
 * параллельно. Внутри задачи {@link #owns} возвращает {@code true} только
 * для региона, которому задача принадлежит.
 */
final class RegionTicker implements AutoCloseable {
    /** 8×8 чанков на регион. */
    static final int SHIFT = 3;
    private static final long TICK_NANOS = 50_000_000L;

    record Key(UUID world, int x, int z) {
        static Key of(World world, int chunkX, int chunkZ) {
            return new Key(world.getUID(), chunkX >> SHIFT, chunkZ >> SHIFT);
        }

        static Key ofBlock(World world, int blockX, int blockZ) {
            return of(world, blockX >> 4, blockZ >> 4);
        }
    }

    private final Worker[] workers;
    /** Регион закрепляется за воркером при первой задаче (round-robin). */
    private final java.util.Map<Key, Integer> assigned = new java.util.concurrent.ConcurrentHashMap<>();
    private final java.util.concurrent.atomic.AtomicInteger nextWorker =
            new java.util.concurrent.atomic.AtomicInteger();
    private final ThreadLocal<Key> current = new ThreadLocal<>();
    private final AtomicInteger live = new AtomicInteger();
    private volatile boolean running = true;

    RegionTicker(int threads) {
        this.workers = new Worker[Math.max(2, threads)];
        for (int i = 0; i < this.workers.length; i++) {
            this.workers[i] = new Worker(i);
            this.workers[i].start();
        }
    }

    int threads() {
        return this.workers.length;
    }

    /** Число задач, реально выполняющихся прямо сейчас (для тестов). */
    int runningTasks() {
        return this.live.get();
    }

    /** Планирует задачу в регионе; {@code delayTicks} — задержка в тиках. */
    RegionJob schedule(Key key, long delayTicks, long periodTicks, Consumer<RegionJob> body) {
        RegionJob job = new RegionJob(key, body, periodTicks);
        job.due = Math.max(1, delayTicks);
        int index = this.assigned.computeIfAbsent(key,
                k -> Math.floorMod(this.nextWorker.getAndIncrement(), this.workers.length));
        this.workers[index].queue.add(job);
        return job;
    }

    /** Принадлежит ли вызывающий поток этому региону прямо сейчас. */
    boolean owns(Key key) {
        return key.equals(this.current.get());
    }

    @Override
    public void close() {
        this.running = false;
        for (Worker worker : this.workers) {
            worker.interrupt();
        }
    }

    final class RegionJob {
        final Key key;
        final Consumer<RegionJob> body;
        final long periodTicks;
        long due;
        volatile boolean cancelled;
        volatile int state = 0;

        RegionJob(Key key, Consumer<RegionJob> body, long periodTicks) {
            this.key = key;
            this.body = body;
            this.periodTicks = periodTicks;
        }

        public boolean isCancelled() {
            return this.cancelled;
        }

        public void cancel() {
            this.cancelled = true;
        }

        boolean alive() {
            return !this.cancelled && this.periodTicks > 0;
        }

        void fire() {
            if (this.cancelled) {
                return;
            }
            int busy = RegionTicker.this.live.incrementAndGet();
            try {
                this.body.accept(this);
            } finally {
                RegionTicker.this.live.decrementAndGet();
                if (busy < 0) {
                    RegionTicker.this.live.set(0);
                }
            }
        }
    }

    private final class Worker extends Thread {
        private final Queue<RegionJob> queue = new ConcurrentLinkedQueue<>();
        private long tick;

        Worker(int index) {
            super("Folia Region Worker #" + index);
            setDaemon(true);
        }

        @Override
        public void run() {
            long next = System.nanoTime();
            while (RegionTicker.this.running) {
                next += TICK_NANOS;
                long wait = next - System.nanoTime();
                if (wait > 0) {
                    try {
                        Thread.sleep(Math.max(0, wait / 1_000_000L), (int) (wait % 1_000_000L));
                    } catch (InterruptedException stop) {
                        return;
                    }
                }
                this.tick++;
                for (RegionJob job : this.queue) {
                    if (job.due > this.tick) {
                        continue;
                    }
                    if (job.cancelled) {
                        this.queue.remove(job);
                        continue;
                    }
                    if (!this.queue.remove(job)) {
                        continue;
                    }
                    RegionTicker.this.current.set(job.key);
                    try {
                        job.fire();
                    } finally {
                        RegionTicker.this.current.remove();
                    }
                    if (job.alive()) {
                        job.due = this.tick + job.periodTicks;
                        this.queue.add(job);
                    }
                }
            }
        }
    }
}
