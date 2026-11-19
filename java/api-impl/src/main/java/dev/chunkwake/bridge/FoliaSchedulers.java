package dev.chunkwake.bridge;

import io.papermc.paper.threadedregions.scheduler.AsyncScheduler;
import io.papermc.paper.threadedregions.scheduler.EntityScheduler;
import io.papermc.paper.threadedregions.scheduler.GlobalRegionScheduler;
import io.papermc.paper.threadedregions.scheduler.RegionScheduler;
import io.papermc.paper.threadedregions.scheduler.ScheduledTask;
import java.util.Objects;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.BooleanSupplier;
import java.util.function.Consumer;
import java.util.logging.Level;
import org.bukkit.World;
import org.bukkit.plugin.IllegalPluginAccessException;
import org.bukkit.plugin.Plugin;

/**
 * Планировщики Folia ({@link GlobalRegionScheduler}, {@link RegionScheduler},
 * {@link AsyncScheduler}, {@link EntityScheduler}) с поведением Paper для
 * однопоточного сервера ({@code FoliaGlobalRegionScheduler},
 * {@code FallbackRegionScheduler}, {@code FoliaAsyncScheduler},
 * {@code FoliaEntityScheduler}).
 *
 * <p>Регион у нас один — главный тик плагинов, поэтому региональные задачи
 * идут в глобальный планировщик, как у Paper. Тиковые задачи живут в общей
 * очереди {@link Scheduler}, но скрыты от {@code BukkitScheduler}
 * ({@code getPendingTasks}, {@code cancelTasks}), как и в Paper.
 */
final class FoliaSchedulers {
    private final Scheduler ticks;
    /** Невыполненные задачи всех видов — для {@link #cancelTasks}. */
    private final Set<Task> live = ConcurrentHashMap.newKeySet();
    private final ScheduledExecutorService timer = Executors.newSingleThreadScheduledExecutor(r -> {
        Thread t = new Thread(r, "Folia Async Scheduler Timer");
        t.setDaemon(true);
        return t;
    });
    private final ExecutorService workers = Executors.newThreadPerTaskExecutor(
            Thread.ofVirtual().name("Folia Async Scheduler Thread #", 0).factory());

    /** Регионные задачи (живут отдельно от {@link Task}). */
    private final Set<RegionTask> regionLive = ConcurrentHashMap.newKeySet();

    /** Настоящие регионные петли (8x8 чанков на регион). */
    final RegionTicker ticker = new RegionTicker(
            Math.max(2, Math.min(4, Runtime.getRuntime().availableProcessors() / 4)));

    final GlobalRegionScheduler global = new Global();
    final RegionScheduler region = new Region();
    final AsyncScheduler async = new Async();

    FoliaSchedulers(Scheduler ticks) {
        this.ticks = ticks;
    }

    /** Планировщик сущности; {@code retired} — сущность исчезла (тогда вместо задачи вызывается {@code retired}). */
    boolean ownsRegion(World world, int blockX, int blockZ) {
        return this.ticker.owns(RegionTicker.Key.ofBlock(world, blockX, blockZ));
    }

    boolean ownsRegionChunk(World world, int chunkX, int chunkZ) {
        return this.ticker.owns(RegionTicker.Key.of(world, chunkX, chunkZ));
    }

    EntityScheduler entity(BooleanSupplier retired) {
        return new Entity(retired);
    }

    /** Отмена всех задач плагина при его выключении (как Paper после {@code onDisable}). */
    void cancelTasks(Plugin plugin) {
        for (Task t : live) {
            if (t.plugin == plugin) {
                t.cancel();
            }
        }
        for (RegionTask t : regionLive) {
            if (t.plugin == plugin) {
                t.cancel();
            }
        }
    }

    private static void checkEnabled(Plugin plugin) {
        if (!plugin.isEnabled()) {
            throw new IllegalPluginAccessException("Plugin attempted to register task while disabled");
        }
    }

    /**
     * {@link ScheduledTask} с машиной состояний Paper: IDLE → RUNNING →
     * IDLE/FINISHED, отмена — CANCELLED или CANCELLED_RUNNING у идущей
     * повторяющейся задачи.
     */
    abstract class Task implements ScheduledTask {
        static final int IDLE = 0, RUNNING = 1, FINISHED = 2, CANCELLED = 3, CANCELLED_RUNNING = 4;
        final AtomicInteger state = new AtomicInteger(IDLE);
        final Plugin plugin;
        final Consumer<ScheduledTask> body;
        final long period;

        Task(Plugin plugin, Consumer<ScheduledTask> body, long period) {
            this.plugin = plugin;
            this.body = body;
            this.period = period;
            live.add(this);
        }

        /** Остановить следующий запуск в исполнителе. */
        abstract void unschedule();

        abstract String kind();

        /** Один запуск; {@code true} — задача повторится. */
        final boolean runOnce() {
            if (!state.compareAndSet(IDLE, RUNNING)) {
                return false;
            }
            try {
                body.accept(this);
            } catch (Throwable t) {
                plugin.getLogger().log(Level.WARNING, kind() + " task for "
                        + plugin.getPluginMeta().getDisplayName() + " generated an exception", t);
            }
            if (period <= 0) {
                state.set(FINISHED);
                live.remove(this);
                return false;
            }
            if (state.compareAndExchange(RUNNING, IDLE) == CANCELLED_RUNNING) {
                state.set(CANCELLED);
                live.remove(this);
                return false;
            }
            return true;
        }

        @Override public Plugin getOwningPlugin() { return plugin; }
        @Override public boolean isRepeatingTask() { return period > 0; }

        @Override
        public CancelledState cancel() {
            for (;;) {
                int s = state.get();
                switch (s) {
                    case IDLE -> {
                        if (state.compareAndSet(IDLE, CANCELLED)) {
                            live.remove(this);
                            unschedule();
                            return CancelledState.CANCELLED_BY_CALLER;
                        }
                    }
                    case RUNNING -> {
                        if (period <= 0) {
                            return CancelledState.RUNNING;
                        }
                        if (state.compareAndSet(RUNNING, CANCELLED_RUNNING)) {
                            return CancelledState.NEXT_RUNS_CANCELLED;
                        }
                    }
                    case CANCELLED_RUNNING -> { return CancelledState.NEXT_RUNS_CANCELLED_ALREADY; }
                    case FINISHED -> { return CancelledState.ALREADY_EXECUTED; }
                    case CANCELLED -> { return CancelledState.CANCELLED_ALREADY; }
                    default -> throw new IllegalStateException("Unknown state: " + s);
                }
            }
        }

        @Override
        public ExecutionState getExecutionState() {
            return switch (state.get()) {
                case IDLE -> ExecutionState.IDLE;
                case RUNNING -> ExecutionState.RUNNING;
                case CANCELLED_RUNNING -> ExecutionState.CANCELLED_RUNNING;
                case FINISHED -> ExecutionState.FINISHED;
                default -> ExecutionState.CANCELLED;
            };
        }

        @Override
        public String toString() {
            return kind() + "ScheduledTask{" + plugin.getName() + ", " + getExecutionState() + "}";
        }
    }

    /** Задача главного тика: глобальная, региональная или сущности. */
    private class TickTask extends Task {
        private final String kind;
        volatile Scheduler.Task handle;

        TickTask(Plugin plugin, Consumer<ScheduledTask> body, long period, String kind) {
            super(plugin, body, period);
            this.kind = kind;
        }

        TickTask start(long delay) {
            handle = ticks.scheduleHidden(plugin, () -> {
                if (!tickRun()) {
                    handle.cancel();
                }
            }, delay, period);
            if (!plugin.isEnabled()) {
                cancel();
            }
            return this;
        }

        boolean tickRun() {
            return runOnce();
        }

        @Override
        void unschedule() {
            Scheduler.Task h = handle;
            if (h != null) {
                h.cancel();
            }
        }

        @Override String kind() { return kind; }
    }

    private final class Global implements GlobalRegionScheduler {
        @Override
        public void execute(Plugin plugin, Runnable run) {
            Objects.requireNonNull(plugin, "Plugin may not be null");
            Objects.requireNonNull(run, "Runnable may not be null");
            run(plugin, t -> run.run());
        }

        @Override
        public ScheduledTask run(Plugin plugin, Consumer<ScheduledTask> task) {
            return runDelayed(plugin, task, 1);
        }

        @Override
        public ScheduledTask runDelayed(Plugin plugin, Consumer<ScheduledTask> task, long delay) {
            Objects.requireNonNull(plugin, "Plugin may not be null");
            Objects.requireNonNull(task, "Task may not be null");
            if (delay <= 0) {
                throw new IllegalArgumentException("Delay ticks may not be <= 0");
            }
            checkEnabled(plugin);
            return new TickTask(plugin, task, -1, "Global").start(delay);
        }

        @Override
        public ScheduledTask runAtFixedRate(Plugin plugin, Consumer<ScheduledTask> task, long initialDelay,
                long period) {
            Objects.requireNonNull(plugin, "Plugin may not be null");
            Objects.requireNonNull(task, "Task may not be null");
            if (initialDelay <= 0) {
                throw new IllegalArgumentException("Initial delay ticks may not be <= 0");
            }
            if (period <= 0) {
                throw new IllegalArgumentException("Period ticks may not be <= 0");
            }
            checkEnabled(plugin);
            return new TickTask(plugin, task, period, "Global").start(initialDelay);
        }

        @Override
        public void cancelTasks(Plugin plugin) {
            Objects.requireNonNull(plugin, "Plugin may not be null");
            for (Task t : live) {
                if (t.plugin == plugin && t instanceof TickTask tt && tt.kind.equals("Global")) {
                    t.cancel();
                }
            }
        }
    }

    /** Как {@code FallbackRegionScheduler}: регион один, всё уходит в глобальный планировщик. */
    private final class Region implements RegionScheduler {
        @Override
        public void execute(Plugin plugin, World world, int chunkX, int chunkZ, Runnable run) {
            Objects.requireNonNull(plugin, "Plugin may not be null");
            Objects.requireNonNull(run, "Runnable may not be null");
            Objects.requireNonNull(world, "World may not be null");
            checkEnabled(plugin);
            schedule(plugin, world, chunkX, chunkZ, 1, 0, t -> run.run());
        }

        @Override
        public ScheduledTask run(Plugin plugin, World world, int chunkX, int chunkZ,
                Consumer<ScheduledTask> task) {
            Objects.requireNonNull(plugin, "Plugin may not be null");
            Objects.requireNonNull(task, "Task may not be null");
            Objects.requireNonNull(world, "World may not be null");
            checkEnabled(plugin);
            return schedule(plugin, world, chunkX, chunkZ, 1, 0, task);
        }

        @Override
        public ScheduledTask runDelayed(Plugin plugin, World world, int chunkX, int chunkZ,
                Consumer<ScheduledTask> task, long delay) {
            Objects.requireNonNull(plugin, "Plugin may not be null");
            Objects.requireNonNull(task, "Task may not be null");
            Objects.requireNonNull(world, "World may not be null");
            if (delay <= 0) {
                throw new IllegalArgumentException("Delay ticks may not be <= 0");
            }
            checkEnabled(plugin);
            return schedule(plugin, world, chunkX, chunkZ, delay, 0, task);
        }

        @Override
        public ScheduledTask runAtFixedRate(Plugin plugin, World world, int chunkX, int chunkZ,
                Consumer<ScheduledTask> task, long initialDelay, long period) {
            Objects.requireNonNull(plugin, "Plugin may not be null");
            Objects.requireNonNull(task, "Task may not be null");
            Objects.requireNonNull(world, "World may not be null");
            if (initialDelay <= 0) {
                throw new IllegalArgumentException("Initial delay ticks may not be <= 0");
            }
            if (period <= 0) {
                throw new IllegalArgumentException("Period ticks may not be <= 0");
            }
            checkEnabled(plugin);
            return schedule(plugin, world, chunkX, chunkZ, initialDelay, period, task);
        }

        private ScheduledTask schedule(Plugin plugin, World world, int chunkX, int chunkZ,
                long delay, long period, Consumer<ScheduledTask> task) {
            RegionTicker.Key key = RegionTicker.Key.of(world, chunkX, chunkZ);
            RegionTask handle = new RegionTask(plugin, period > 0);
            RegionTicker.RegionJob job = ticker.schedule(key, delay, period, j -> {
                if (handle.begin()) {
                    try {
                        task.accept(handle);
                    } finally {
                        handle.end();
                    }
                }
            });
            handle.attach(job);
            return handle;
        }
    }

    /** {@link ScheduledTask} поверх регионной задачи. */
    final class RegionTask implements ScheduledTask {
        private final Plugin plugin;
        private final boolean repeating;
        private final java.util.concurrent.atomic.AtomicInteger state =
                new java.util.concurrent.atomic.AtomicInteger(0);
        private volatile RegionTicker.RegionJob job;

        RegionTask(Plugin plugin, boolean repeating) {
            this.plugin = plugin;
            this.repeating = repeating;
            regionLive.add(this);
        }

        void attach(RegionTicker.RegionJob job) {
            this.job = job;
        }

        boolean begin() {
            return this.state.compareAndSet(0, 1);
        }

        void end() {
            if (!this.repeating) {
                this.state.set(2);
                regionLive.remove(this);
            } else if (this.state.get() == 1) {
                this.state.set(0);
            }
        }

        @Override
        public Plugin getOwningPlugin() {
            return this.plugin;
        }

        @Override
        public boolean isRepeatingTask() {
            return this.repeating;
        }

        @Override
        public CancelledState cancel() {
            RegionTicker.RegionJob j = this.job;
            if (j != null) {
                j.cancel();
            }
            int previous = this.state.getAndUpdate(v -> v == 1 ? 4 : 3);
            regionLive.remove(this);
            if (previous == 3 || previous == 4) {
                return CancelledState.CANCELLED_ALREADY;
            }
            return previous == 1 ? CancelledState.NEXT_RUNS_CANCELLED
                    : CancelledState.CANCELLED_BY_CALLER;
        }

        @Override
        public ExecutionState getExecutionState() {
            return switch (this.state.get()) {
                case 1 -> ExecutionState.RUNNING;
                case 2 -> ExecutionState.FINISHED;
                case 3 -> ExecutionState.CANCELLED;
                case 4 -> ExecutionState.CANCELLED_RUNNING;
                default -> ExecutionState.IDLE;
            };
        }
    }

    /** Асинхронная задача: таймер отсчитывает время, тело — в виртуальном потоке. */
    private final class AsyncTask extends Task {
        private final long periodNanos;
        private long due;
        private volatile Future<?> pending;

        AsyncTask(Plugin plugin, Consumer<ScheduledTask> body, long periodNanos) {
            super(plugin, body, periodNanos);
            this.periodNanos = periodNanos;
        }

        AsyncTask start(long delayNanos) {
            due = System.nanoTime() + delayNanos;
            arm();
            return this;
        }

        private void arm() {
            long wait = Math.max(0, due - System.nanoTime());
            pending = timer.schedule(() -> {
                pending = workers.submit(this::fire);
            }, wait, TimeUnit.NANOSECONDS);
        }

        private void fire() {
            if (runOnce()) {
                due += periodNanos;
                arm();
            }
        }

        @Override
        void unschedule() {
            Future<?> f = pending;
            if (f != null) {
                f.cancel(false);
            }
        }

        @Override String kind() { return "Async"; }
    }

    private final class Async implements AsyncScheduler {
        @Override
        public ScheduledTask runNow(Plugin plugin, Consumer<ScheduledTask> task) {
            Objects.requireNonNull(plugin, "Plugin may not be null");
            Objects.requireNonNull(task, "Task may not be null");
            checkEnabled(plugin);
            AsyncTask t = new AsyncTask(plugin, task, -1);
            t.pending = workers.submit(t::fire);
            return t;
        }

        @Override
        public ScheduledTask runDelayed(Plugin plugin, Consumer<ScheduledTask> task, long delay, TimeUnit unit) {
            Objects.requireNonNull(plugin, "Plugin may not be null");
            Objects.requireNonNull(task, "Task may not be null");
            Objects.requireNonNull(unit, "Time unit may not be null");
            if (delay < 0) {
                throw new IllegalArgumentException("Delay may not be < 0");
            }
            checkEnabled(plugin);
            return new AsyncTask(plugin, task, -1).start(unit.toNanos(delay));
        }

        @Override
        public ScheduledTask runAtFixedRate(Plugin plugin, Consumer<ScheduledTask> task, long initialDelay,
                long period, TimeUnit unit) {
            Objects.requireNonNull(plugin, "Plugin may not be null");
            Objects.requireNonNull(task, "Task may not be null");
            Objects.requireNonNull(unit, "Time unit may not be null");
            if (initialDelay < 0) {
                throw new IllegalArgumentException("Initial delay may not be < 0");
            }
            if (period <= 0) {
                throw new IllegalArgumentException("Period may not be <= 0");
            }
            checkEnabled(plugin);
            return new AsyncTask(plugin, task, Math.max(1, unit.toNanos(period))).start(unit.toNanos(initialDelay));
        }

        @Override
        public void cancelTasks(Plugin plugin) {
            Objects.requireNonNull(plugin, "Plugin may not be null");
            for (Task t : live) {
                if (t.plugin == plugin && t instanceof AsyncTask) {
                    t.cancel();
                }
            }
        }
    }

    /**
     * Задачи сущности. Пока сущность жива, задача идёт в главном тике; если
     * сущность исчезла — вместо задачи один раз вызывается {@code retired},
     * а планирование на исчезнувшей сущности возвращает {@code null}/{@code false}.
     */
    private final class Entity implements EntityScheduler {
        private final BooleanSupplier isRetired;

        Entity(BooleanSupplier isRetired) {
            this.isRetired = isRetired;
        }

        private final class EntityTask extends TickTask {
            private final Runnable retired;

            EntityTask(Plugin plugin, Consumer<ScheduledTask> body, Runnable retired, long period) {
                super(plugin, body, period, "Entity");
                this.retired = retired;
            }

            @Override
            boolean tickRun() {
                if (isRetired.getAsBoolean()) {
                    if (state.compareAndSet(IDLE, CANCELLED)) {
                        live.remove(this);
                        if (retired != null) {
                            try {
                                retired.run();
                            } catch (Throwable t) {
                                plugin.getLogger().log(Level.WARNING, "Entity retired task for "
                                        + plugin.getPluginMeta().getDisplayName() + " generated an exception", t);
                            }
                        }
                    }
                    return false;
                }
                return runOnce();
            }
        }

        @Override
        public boolean execute(Plugin plugin, Runnable run, Runnable retired, long delay) {
            Objects.requireNonNull(plugin, "Plugin may not be null");
            Objects.requireNonNull(run, "Runnable may not be null");
            if (isRetired.getAsBoolean() || !plugin.isEnabled()) {
                return false;
            }
            new EntityTask(plugin, t -> run.run(), retired, -1).start(Math.max(delay, 1));
            return true;
        }

        @Override
        public ScheduledTask run(Plugin plugin, Consumer<ScheduledTask> task, Runnable retired) {
            return runDelayed(plugin, task, retired, 1);
        }

        @Override
        public ScheduledTask runDelayed(Plugin plugin, Consumer<ScheduledTask> task, Runnable retired, long delay) {
            Objects.requireNonNull(plugin, "Plugin may not be null");
            Objects.requireNonNull(task, "Task may not be null");
            if (delay <= 0) {
                throw new IllegalArgumentException("Delay ticks may not be <= 0");
            }
            checkEnabled(plugin);
            if (isRetired.getAsBoolean()) {
                return null;
            }
            return new EntityTask(plugin, task, retired, -1).start(delay);
        }

        @Override
        public ScheduledTask runAtFixedRate(Plugin plugin, Consumer<ScheduledTask> task, Runnable retired,
                long initialDelay, long period) {
            Objects.requireNonNull(plugin, "Plugin may not be null");
            Objects.requireNonNull(task, "Task may not be null");
            if (initialDelay <= 0) {
                throw new IllegalArgumentException("Initial delay ticks may not be <= 0");
            }
            if (period <= 0) {
                throw new IllegalArgumentException("Period ticks may not be <= 0");
            }
            checkEnabled(plugin);
            if (isRetired.getAsBoolean()) {
                return null;
            }
            return new EntityTask(plugin, task, retired, period).start(initialDelay);
        }
    }
}
