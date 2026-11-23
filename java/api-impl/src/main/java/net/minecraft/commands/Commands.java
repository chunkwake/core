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

    /** {@code Commands.literal(String)} (Emotecraft: /emote). */
    public static com.mojang.brigadier.builder.LiteralArgumentBuilder<Object> literal(String name) {
        return com.mojang.brigadier.builder.LiteralArgumentBuilder.literal(name);
    }

    /** {@code Commands.argument(String, ArgumentType)}. */
    public static com.mojang.brigadier.builder.RequiredArgumentBuilder<Object, ?> argument(
            String name, com.mojang.brigadier.arguments.ArgumentType<?> type) {
        return com.mojang.brigadier.builder.RequiredArgumentBuilder.argument(name, type);
    }

    /** {@code Commands.CommandSelection}: где доступна команда. */
    public enum CommandSelection {
        ALL,
        DEDICATED,
        INTEGRATED
    }

    /** `Commands.createValidationContext(HolderLookup.Provider)`. */
    public static CommandBuildContext createValidationContext(
            net.minecraft.core.HolderLookup.Provider provider) {
        return CommandBuildContext.simple(provider,
                net.minecraft.world.flag.FeatureFlagSet.of());
    }
}
