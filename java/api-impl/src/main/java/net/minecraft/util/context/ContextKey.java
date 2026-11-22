package net.minecraft.util.context;

/** NMS {@code ContextKey}: ключ параметра контекста. */
public class ContextKey<T> {
    private final String name;

    public ContextKey() {
        this(null);
    }

    public ContextKey(String name) {
        this.name = name;
    }

    @Override
    public String toString() {
        return this.name == null ? "ContextKey" : this.name;
    }
}
