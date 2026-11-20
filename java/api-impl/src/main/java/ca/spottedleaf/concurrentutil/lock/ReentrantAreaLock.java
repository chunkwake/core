package ca.spottedleaf.concurrentutil.lock;

/** Заглушка Moonrise {@code ReentrantAreaLock}: блокировки не нужны без Moonrise. */
public class ReentrantAreaLock {
    public static final class Node {
        Node() {
        }
    }

    public ReentrantAreaLock() {
    }

    public Node lock(int x, int z) {
        return new Node();
    }

    public void unlock(Node node) {
    }
}
