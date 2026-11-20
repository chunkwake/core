package ca.spottedleaf.moonrise.patches.chunk_system.scheduling;

import ca.spottedleaf.concurrentutil.executor.thread.BalancedPrioritisedThreadPool;
import ca.spottedleaf.concurrentutil.lock.ReentrantAreaLock;
import net.minecraft.server.level.ServerLevel;

/** Минимальная заглушка Moonrise {@code ChunkTaskScheduler} (squaremap). */
public class ChunkTaskScheduler {
    public final ReentrantAreaLock schedulingLockArea = new ReentrantAreaLock();
    public final ChunkHolderManager chunkHolderManager;
    public final BalancedPrioritisedThreadPool.OrderedStreamGroup.Queue loadExecutor =
            new BalancedPrioritisedThreadPool.OrderedStreamGroup.Queue();

    public ChunkTaskScheduler(ServerLevel level) {
        this.chunkHolderManager = new ChunkHolderManager(level);
    }

    public void execute(Runnable runnable) {
        runnable.run();
    }
}
