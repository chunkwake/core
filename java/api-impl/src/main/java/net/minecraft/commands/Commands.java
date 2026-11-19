package net.minecraft.commands;

import com.mojang.brigadier.CommandDispatcher;

/**
 * NMS {@code Commands}: диспетчер команд сервера.
 *
 * <p>Плагины (NpcPlugin) берут его через
 * {@code CraftServer.getServer().getCommands().getDispatcher()} и
 * регистрируют в Brigadier свои вершины. Исполнение этих команд пока не
 * связано с командным слоем Chunkwake (он в Rust), но регистрация и
 * построение дерева работают по-настоящему.
 */
public class Commands {
    private final CommandDispatcher<Object> dispatcher = new CommandDispatcher<>();

    public Commands() {
    }

    @SuppressWarnings("rawtypes")
    public CommandDispatcher getDispatcher() {
        return this.dispatcher;
    }

    /** `Commands.createValidationContext(HolderLookup.Provider)`. */
    public static CommandBuildContext createValidationContext(
            net.minecraft.core.HolderLookup.Provider provider) {
        return new CommandBuildContext();
    }
}
