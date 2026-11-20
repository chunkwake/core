package ca.spottedleaf.concurrentutil.executor.thread;

/** Заглушка Moonrise {@code BalancedPrioritisedThreadPool}. */
public class BalancedPrioritisedThreadPool {
    public static final class OrderedStreamGroup {
        public static final class Queue implements java.util.concurrent.Executor {
            public Queue() {
            }

            @Override
            public void execute(Runnable command) {
                command.run();
            }
        }
    }
}
