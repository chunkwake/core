package net.minecraft.commands;

/**
 * NMS {@code CommandBuildContext}: контекст сборки аргументов команд.
 *
 * <p>Нужен плагинам, которые строят Brigadier-команды через аргументы,
 * зависящие от реестров (`ComponentArgument.textComponent(context)`).
 * Реестры у нас подняты (`VanillaBootstrap`), поэтому пустого контекста
 * достаточно: сами аргументы в Chunkwake не парсятся NMS-кодом.
 */
public class CommandBuildContext {
    public CommandBuildContext() {
    }
}
