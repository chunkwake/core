package net.minecraft.commands;

/** NMS {@code ExecutionCommandSource} (CommandAPI). */
public interface ExecutionCommandSource<T extends ExecutionCommandSource<T>> {
    default T withCallback(CommandResultCallback callback) {
        return (T) this;
    }

    default void clearCallbacks() {
    }
}
