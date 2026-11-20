package net.minecraft.commands;

/** NMS {@code CommandResultCallback} (CommandAPI). */
public interface CommandResultCallback {
    void onResult(boolean success, int result);

    CommandResultCallback EMPTY = (success, result) -> {
    };
}
