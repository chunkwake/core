package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import io.papermc.paper.threadedregions.scheduler.EntityScheduler;
import io.papermc.paper.threadedregions.scheduler.ScheduledTask;
import io.papermc.paper.threadedregions.scheduler.ScheduledTask.CancelledState;
import io.papermc.paper.threadedregions.scheduler.ScheduledTask.ExecutionState;
import java.io.StringReader;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import org.bukkit.plugin.IllegalPluginAccessException;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.PluginDescriptionFile;
import org.junit.jupiter.api.Test;

/** Планировщики Folia: те же проверки, состояния и отмена, что у Paper. */
class FoliaSchedulersTest {
    private final Scheduler ticks = new Scheduler();
    private final FoliaSchedulers folia = new FoliaSchedulers(ticks);

    private static Plugin plugin(boolean enabled) throws Exception {
        return Stub.of(Plugin.class, new PaperCommandsTest.FakePlugin(
                new PluginDescriptionFile(new StringReader("name: P\nversion: 1\nmain: p.P\n")), enabled));
    }

    private void tick(int n) {
        for (int i = 0; i < n; i++) {
            ticks.tick();
        }
    }

    @Test
    void fixedRateRepeatsUntilCancelledFromInside() throws Exception {
        Plugin p = plugin(true);
        AtomicInteger runs = new AtomicInteger();
        CancelledState[] fromInside = new CancelledState[1];
        ScheduledTask task = folia.global.runAtFixedRate(p, t -> {
            if (runs.incrementAndGet() == 3) {
                fromInside[0] = t.cancel();
            }
        }, 1, 2);
        assertTrue(task.isRepeatingTask());
        tick(20);
        assertEquals(3, runs.get());
        assertEquals(CancelledState.NEXT_RUNS_CANCELLED, fromInside[0]);
        assertEquals(ExecutionState.CANCELLED, task.getExecutionState());
        assertEquals(CancelledState.CANCELLED_ALREADY, task.cancel());
    }

    @Test
    void oneShotFinishesAndIsInvisibleToBukkitScheduler() throws Exception {
        Plugin p = plugin(true);
        AtomicInteger runs = new AtomicInteger();
        ScheduledTask task = folia.global.runDelayed(p, t -> runs.incrementAndGet(), 3);
        assertEquals(List.of(), ticks.getPendingTasks());
        ticks.cancelTasks(p); // BukkitScheduler не трогает задачи Folia
        tick(10);
        assertEquals(1, runs.get());
        assertEquals(ExecutionState.FINISHED, task.getExecutionState());
        assertEquals(CancelledState.ALREADY_EXECUTED, task.cancel());

        ScheduledTask idle = folia.global.runDelayed(p, t -> runs.incrementAndGet(), 3);
        assertEquals(CancelledState.CANCELLED_BY_CALLER, idle.cancel());
        tick(10);
        assertEquals(1, runs.get());
    }

    @Test
    void argumentChecksMatchPaper() throws Exception {
        Plugin p = plugin(true);
        assertThrows(IllegalArgumentException.class, () -> folia.global.runDelayed(p, t -> { }, 0));
        assertThrows(IllegalArgumentException.class, () -> folia.global.runAtFixedRate(p, t -> { }, 1, 0));
        assertThrows(IllegalArgumentException.class, () -> folia.async.runDelayed(p, t -> { }, -1, TimeUnit.SECONDS));
        assertThrows(NullPointerException.class, () -> folia.async.runDelayed(p, t -> { }, 1, null));
        Plugin off = plugin(false);
        assertThrows(IllegalPluginAccessException.class, () -> folia.global.run(off, t -> { }));
        assertThrows(IllegalPluginAccessException.class, () -> folia.async.runNow(off, t -> { }));
    }

    @Test
    void entityTasksRetireWithTheEntity() throws Exception {
        Plugin p = plugin(true);
        AtomicBoolean gone = new AtomicBoolean();
        EntityScheduler s = folia.entity(gone::get);
        AtomicInteger runs = new AtomicInteger();
        AtomicInteger retired = new AtomicInteger();
        ScheduledTask task = s.runAtFixedRate(p, t -> runs.incrementAndGet(), retired::incrementAndGet, 1, 1);
        tick(5);
        int before = runs.get();
        assertTrue(before >= 3, "runs " + before);
        gone.set(true);
        tick(5);
        assertEquals(before, runs.get());
        assertEquals(1, retired.get());
        assertEquals(ExecutionState.CANCELLED, task.getExecutionState());
        assertNull(s.run(p, t -> { }, null));
        assertEquals(false, s.execute(p, () -> { }, null, 1));
    }

    @Test
    void asyncFixedRateAndPluginDisable() throws Exception {
        Plugin p = plugin(true);
        CountDownLatch three = new CountDownLatch(3);
        ScheduledTask timer = folia.async.runAtFixedRate(p, t -> three.countDown(), 0, 10, TimeUnit.MILLISECONDS);
        assertTrue(three.await(5, TimeUnit.SECONDS));
        CountDownLatch now = new CountDownLatch(1);
        folia.async.runNow(p, t -> now.countDown());
        assertTrue(now.await(5, TimeUnit.SECONDS));

        ScheduledTask global = folia.global.runAtFixedRate(p, t -> { }, 1, 1);
        folia.cancelTasks(p);
        assertTrue(timer.isCancelled());
        assertTrue(global.isCancelled());
    }
}
