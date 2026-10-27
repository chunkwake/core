package dev.chunkwake.bridge;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Consumer;
import java.util.logging.Level;
import org.bukkit.plugin.Plugin;
import org.bukkit.scheduler.BukkitRunnable;
import org.bukkit.scheduler.BukkitScheduler;
import org.bukkit.scheduler.BukkitTask;

/**
 * {@link BukkitScheduler}: синхронные задачи выполняются в потоке плагинов по
 * тикам от Rust (20/с), асинхронные — в виртуальных потоках. Пока задач нет,
 * флаг в общей памяти сброшен и Rust не вызывает тик вовсе.
 */
final class Scheduler {
    final BukkitScheduler api = Stub.of(BukkitScheduler.class, this);
    private final AtomicInteger ids = new AtomicInteger();
    /** Добавления из любых потоков; разбираются в тике. */
    private final ConcurrentLinkedQueue<Task> incoming = new ConcurrentLinkedQueue<>();
    private final List<Task> sync = new ArrayList<>();
    private final ExecutorService async = Executors.newVirtualThreadPerTaskExecutor();
    private long now;

    final class Task implements BukkitTask {
        final int id = ids.incrementAndGet();
        final Plugin owner;
        final Runnable body;
        final boolean isSync;
        final long period;
        long due;
        volatile boolean cancelled;

        Task(Plugin owner, Runnable body, boolean isSync, long delay, long period) {
            this.owner = owner;
            this.body = body;
            this.isSync = isSync;
            this.period = period;
            this.due = Math.max(delay, 0);
        }

        @Override public int getTaskId() { return id; }
        @Override public Plugin getOwner() { return owner; }
        @Override public boolean isSync() { return isSync; }
        @Override public boolean isCancelled() { return cancelled; }
        @Override public void cancel() { cancelled = true; }

        void run() {
            try {
                body.run();
            } catch (Throwable t) {
                owner.getLogger().log(Level.WARNING, "Task #" + id + " generated an exception", t);
            }
        }
    }

    private Task schedule(Plugin plugin, Runnable body, boolean isSync, long delay, long period) {
        if (plugin == null || !plugin.isEnabled()) {
            throw new IllegalArgumentException("Plugin attempted to register task while disabled");
        }
        Task t = new Task(plugin, body, isSync, delay, period);
        if (isSync) {
            incoming.add(t);
            Bridge.setFlag(Bridge.FLAG_HAS_TASKS, 1);
        } else {
            async.submit(() -> runAsync(t));
        }
        return t;
    }

    private void runAsync(Task t) {
        try {
            Thread.sleep(t.due * 50);
            do {
                if (t.cancelled) {
                    return;
                }
                t.run();
                if (t.period > 0) {
                    Thread.sleep(t.period * 50);
                }
            } while (t.period > 0);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
    }

    /** Тик сервера: задачи, срок которых наступил, по порядку. */
    void tick() {
        now++;
        for (Task t; (t = incoming.poll()) != null; ) {
            t.due += now;
            sync.add(t);
        }
        List<Task> ready = new ArrayList<>();
        for (Iterator<Task> it = sync.iterator(); it.hasNext(); ) {
            Task t = it.next();
            if (t.cancelled) {
                it.remove();
            } else if (t.due <= now) {
                ready.add(t);
            }
        }
        for (Task t : ready) {
            if (t.cancelled) {
                continue;
            }
            t.run();
            if (t.period > 0) {
                t.due = now + t.period;
            } else {
                t.cancelled = true;
            }
        }
        sync.removeIf(t -> t.cancelled);
        Bridge.setFlag(Bridge.FLAG_HAS_TASKS, sync.isEmpty() && incoming.isEmpty() ? 0 : 1);
    }

    // --- BukkitScheduler ---

    public BukkitTask runTask(Plugin p, Runnable r) { return schedule(p, r, true, 0, 0); }
    public BukkitTask runTask(Plugin p, BukkitRunnable r) { return runTask(p, (Runnable) r); }
    public BukkitTask runTaskLater(Plugin p, Runnable r, long d) { return schedule(p, r, true, d, 0); }
    public BukkitTask runTaskLater(Plugin p, BukkitRunnable r, long d) { return runTaskLater(p, (Runnable) r, d); }
    public BukkitTask runTaskTimer(Plugin p, Runnable r, long d, long per) { return schedule(p, r, true, d, Math.max(per, 1)); }
    public BukkitTask runTaskTimer(Plugin p, BukkitRunnable r, long d, long per) { return runTaskTimer(p, (Runnable) r, d, per); }
    public BukkitTask runTaskAsynchronously(Plugin p, Runnable r) { return schedule(p, r, false, 0, 0); }
    public BukkitTask runTaskAsynchronously(Plugin p, BukkitRunnable r) { return runTaskAsynchronously(p, (Runnable) r); }
    public BukkitTask runTaskLaterAsynchronously(Plugin p, Runnable r, long d) { return schedule(p, r, false, d, 0); }
    public BukkitTask runTaskLaterAsynchronously(Plugin p, BukkitRunnable r, long d) { return runTaskLaterAsynchronously(p, (Runnable) r, d); }
    public BukkitTask runTaskTimerAsynchronously(Plugin p, Runnable r, long d, long per) { return schedule(p, r, false, d, Math.max(per, 1)); }
    public BukkitTask runTaskTimerAsynchronously(Plugin p, BukkitRunnable r, long d, long per) { return runTaskTimerAsynchronously(p, (Runnable) r, d, per); }

    public void runTask(Plugin p, Consumer<? super BukkitTask> c) { withSelf(p, c, true, 0, 0); }
    public void runTaskLater(Plugin p, Consumer<? super BukkitTask> c, long d) { withSelf(p, c, true, d, 0); }
    public void runTaskTimer(Plugin p, Consumer<? super BukkitTask> c, long d, long per) { withSelf(p, c, true, d, Math.max(per, 1)); }
    public void runTaskAsynchronously(Plugin p, Consumer<? super BukkitTask> c) { withSelf(p, c, false, 0, 0); }
    public void runTaskLaterAsynchronously(Plugin p, Consumer<? super BukkitTask> c, long d) { withSelf(p, c, false, d, 0); }
    public void runTaskTimerAsynchronously(Plugin p, Consumer<? super BukkitTask> c, long d, long per) { withSelf(p, c, false, d, Math.max(per, 1)); }

    private void withSelf(Plugin p, Consumer<? super BukkitTask> c, boolean isSync, long d, long per) {
        BukkitTask[] self = new BukkitTask[1];
        self[0] = schedule(p, () -> c.accept(self[0]), isSync, d, per);
    }

    public int scheduleSyncDelayedTask(Plugin p, Runnable r, long d) { return runTaskLater(p, r, d).getTaskId(); }
    public int scheduleSyncDelayedTask(Plugin p, Runnable r) { return runTask(p, r).getTaskId(); }
    public int scheduleSyncRepeatingTask(Plugin p, Runnable r, long d, long per) { return runTaskTimer(p, r, d, per).getTaskId(); }
    public int scheduleAsyncDelayedTask(Plugin p, Runnable r, long d) { return runTaskLaterAsynchronously(p, r, d).getTaskId(); }
    public int scheduleAsyncDelayedTask(Plugin p, Runnable r) { return runTaskAsynchronously(p, r).getTaskId(); }
    public int scheduleAsyncRepeatingTask(Plugin p, Runnable r, long d, long per) { return runTaskTimerAsynchronously(p, r, d, per).getTaskId(); }

    public <T> Future<T> callSyncMethod(Plugin p, Callable<T> task) {
        CompletableFuture<T> f = new CompletableFuture<>();
        runTask(p, () -> {
            try {
                f.complete(task.call());
            } catch (Throwable t) {
                f.completeExceptionally(t);
            }
        });
        return f;
    }

    public void cancelTask(int id) {
        incoming.forEach(t -> { if (t.id == id) t.cancelled = true; });
        sync.forEach(t -> { if (t.id == id) t.cancelled = true; });
    }

    public void cancelTasks(Plugin p) {
        incoming.forEach(t -> { if (t.owner == p) t.cancelled = true; });
        sync.forEach(t -> { if (t.owner == p) t.cancelled = true; });
    }

    public boolean isQueued(int id) {
        return sync.stream().anyMatch(t -> t.id == id && !t.cancelled)
                || incoming.stream().anyMatch(t -> t.id == id && !t.cancelled);
    }

    public boolean isCurrentlyRunning(int id) {
        return false;
    }

    public List<BukkitTask> getPendingTasks() {
        List<BukkitTask> out = new ArrayList<>(sync);
        out.addAll(incoming);
        return out;
    }

    public java.util.concurrent.Executor getMainThreadExecutor(Plugin p) {
        return r -> runTask(p, r);
    }
}
