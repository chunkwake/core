package net.minecraft;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.TimeUnit;

/** NMS {@code TracingExecutor}. */
public final class TracingExecutor implements java.util.concurrent.Executor {
    private final ExecutorService service;

    public TracingExecutor(ExecutorService service) {
        this.service = service;
    }

    public ExecutorService service() {
        return this.service;
    }

    public TracingExecutor forName(String name) {
        return this;
    }

    @Override
    public void execute(Runnable runnable) {
        this.service.execute(runnable);
    }

    public void shutdownAndAwait(long timeout, TimeUnit unit) {
        this.service.shutdown();
    }
}
