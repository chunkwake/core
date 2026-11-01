package dev.chunkwake.bridge;

import io.papermc.paper.threadedregions.scheduler.EntityScheduler;
import io.papermc.paper.threadedregions.scheduler.ScheduledTask;
import java.util.function.Consumer;
import org.bukkit.plugin.Plugin;

/**
 * {@link EntityScheduler} для Folia-совместимых плагинов.
 *
 * На Folia задачи сущности выполняются в потоке региона этой сущности. У нас
 * поток один — тот же, что у {@link org.bukkit.scheduler.BukkitScheduler}, —
 * поэтому задачи просто уходят в общий планировщик синхронными: для плагина
 * разница не видна. Аргумент «сущность исчезла» игнорируется: сущности у нас
 * не удаляются вне тика.
 */
final class EntitySchedulers implements EntityScheduler {

    private final Scheduler scheduler;

    private EntitySchedulers(Scheduler scheduler) {
        this.scheduler = scheduler;
    }

    static EntityScheduler of(Scheduler scheduler) {
        return Stub.of(EntityScheduler.class, new EntitySchedulers(scheduler));
    }

    @Override
    public boolean execute(Plugin plugin, Runnable run, Runnable retired, long delay) {
        try {
            scheduler.runTaskLater(plugin, run, Math.max(delay, 0));
            return true;
        } catch (RuntimeException e) {
            // Плагин уже выключен: Paper в этом случае тоже возвращает false.
            return false;
        }
    }

    @Override
    public ScheduledTask run(Plugin plugin, Consumer<ScheduledTask> onRun, Runnable retired) {
        return schedule(plugin, onRun, 0, 0);
    }

    @Override
    public ScheduledTask runDelayed(Plugin plugin, Consumer<ScheduledTask> onRun, Runnable retired,
            long delay) {
        return schedule(plugin, onRun, Math.max(delay, 0), 0);
    }

    @Override
    public ScheduledTask runAtFixedRate(Plugin plugin, Consumer<ScheduledTask> onRun, Runnable retired,
            long initialDelay, long period) {
        return schedule(plugin, onRun, Math.max(initialDelay, 0), Math.max(period, 1));
    }

    /**
     * Задача, которая передаёт себя в {@code onRun} и умеет отменяться.
     *
     * `ScheduledTask` — интерфейс с `equals`/`hashCode` от прокси, поэтому
     * заглушка возвращает саму задачу планировщика только для отмены и
     * состояния, а `onRun` получает её же.
     */
    private ScheduledTask schedule(Plugin plugin, Consumer<ScheduledTask> onRun, long delay, long period) {
        Held held = new Held(plugin);
        org.bukkit.scheduler.BukkitTask task = period > 0
                ? scheduler.runTaskTimer(plugin, () -> {
                    if (!held.done.compareAndSet(false, true)) {
                        return;
                    }
                    onRun.accept(held.api);
                }, delay, period)
                : scheduler.runTaskLater(plugin, () -> {
                    if (!held.done.compareAndSet(false, true)) {
                        return;
                    }
                    onRun.accept(held.api);
                }, delay);
        held.task = task;
        return held.api;
    }

    /** Задача плюс её вид для {@link ScheduledTask}. */
    private final class Held {
        final java.util.concurrent.atomic.AtomicBoolean done = new java.util.concurrent.atomic.AtomicBoolean();
        final ScheduledTask api;
        volatile org.bukkit.scheduler.BukkitTask task;
        private final boolean repeating;
        private final Plugin plugin;

        Held(Plugin plugin) {
            this.plugin = plugin;
            this.repeating = true;
            this.api = Stub.of(ScheduledTask.class, new Object() {
                public Plugin getOwningPlugin() {
                    return Held.this.plugin;
                }

                public boolean isRepeatingTask() {
                    return Held.this.repeating;
                }

                public ScheduledTask.CancelledState cancel() {
                    if (Held.this.task != null) {
                        Held.this.task.cancel();
                    }
                    Held.this.done.set(true);
                    return ScheduledTask.CancelledState.CANCELLED_BY_CALLER;
                }

                public ScheduledTask.ExecutionState getExecutionState() {
                    if (Held.this.done.get()) {
                        return ScheduledTask.ExecutionState.CANCELLED;
                    }
                    return ScheduledTask.ExecutionState.IDLE;
                }

                @Override
                public String toString() {
                    return "EntityScheduledTask[" + (Held.this.task == null ? 0 : Held.this.task.getTaskId()) + "]";
                }
            });
        }
    }
}
