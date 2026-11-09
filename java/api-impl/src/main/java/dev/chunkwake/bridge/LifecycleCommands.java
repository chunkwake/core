package dev.chunkwake.bridge;

import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.context.CommandContext;
import com.mojang.brigadier.tree.LiteralCommandNode;
import io.papermc.paper.command.brigadier.BasicCommand;
import io.papermc.paper.command.brigadier.CommandSourceStack;
import io.papermc.paper.command.brigadier.Commands;
import io.papermc.paper.plugin.configuration.PluginMeta;
import java.util.Collection;
import java.util.LinkedHashSet;
import java.util.Map;
import java.util.Set;
import org.bukkit.command.CommandSender;
import org.bukkit.command.defaults.BukkitCommand;
import org.bukkit.entity.Entity;
import org.bukkit.entity.Player;
import org.bukkit.Location;

/**
 * Регистратор команд paper-api ({@code LifecycleEvents.COMMANDS}) поверх нашего
 * {@link org.bukkit.command.CommandMap}.
 *
 * <p>Paper отдаёт плагину этот регистратор в событии жизненного цикла; команды
 * оттуда попадают в brigadier-дерево сервера. У нас нет ни дерева, ни
 * диспетчеризации событий, поэтому регистратор сразу превращает каждую команду
 * в обычную {@link BukkitCommand} — иначе {@code /bluemap}, {@code /spark} и
 * прочее просто не существуют, хотя плагин зарегистрировал их без ошибок.
 *
 * <p>Выполнение: brigadier-узел несёт {@code exec}({@link CommandSourceStack}).
 * Строка разбирается жадно — аргументы уходят команде так, как их написали;
 * вложенные подкоманды ({@code /bluemap reload}) исполняются по пути узла.
 */
final class LifecycleCommands implements Commands {

    private final ChunkwakeServer server;
    private final CommandDispatcher<CommandSourceStack> dispatcher = new CommandDispatcher<>();
    private PluginMeta plugin;
    /** Сколько команд ушло в игру: попадает в журнал для проверки. */
    private int registered;

    LifecycleCommands(ChunkwakeServer server) {
        this.server = server;
    }

    int registered() {
        return registered;
    }

    /** Чья регистрация сейчас идёт: команде нужен владелец для прав и префикса. */
    void ownedBy(PluginMeta meta) {
        this.plugin = meta;
    }

    @Override
    public CommandDispatcher<CommandSourceStack> getDispatcher() {
        return dispatcher;
    }

    @Override
    public Set<String> register(LiteralCommandNode<CommandSourceStack> node, String fallbackPrefix,
            Collection<String> aliases) {
        return register(null, node, fallbackPrefix, aliases);
    }

    @Override
    public Set<String> register(PluginMeta meta, LiteralCommandNode<CommandSourceStack> node, String fallbackPrefix,
            Collection<String> aliases) {
        PluginMeta owner = meta != null ? meta : plugin;
        String prefix = fallbackPrefix != null && !fallbackPrefix.isEmpty()
                ? fallbackPrefix
                : owner != null ? owner.getName().toLowerCase(java.util.Locale.ROOT) : "chunkwake";
        dispatcher.getRoot().addChild(node);
        Set<String> registered = new LinkedHashSet<>();
        registered.add(node.getName());
        registered.addAll(aliases);
        for (String name : registered) {
            server.registerLifecycleCommand(prefix, name, node, owner);
            this.registered++;
        }
        return registered;
    }

    @Override
    public Set<String> registerWithFlags(PluginMeta meta, LiteralCommandNode<CommandSourceStack> node,
            String fallbackPrefix, Collection<String> aliases,
            Set<io.papermc.paper.command.brigadier.CommandRegistrationFlag> flags) {
        return register(meta, node, fallbackPrefix, aliases);
    }

    @Override
    public Set<String> register(PluginMeta meta, String name, String fallbackPrefix, Collection<String> aliases,
            BasicCommand command) {
        return registerWithFlags(meta, name, fallbackPrefix, aliases, command, Set.of());
    }

    @Override
    public Set<String> register(String name, String fallbackPrefix, Collection<String> aliases,
            BasicCommand command) {
        return registerWithFlags(null, name, fallbackPrefix, aliases, command, Set.of());
    }

    /** {@code BasicCommand} как brigadier-узел: дальше путь исполнения общий. */
    Set<String> registerWithFlags(PluginMeta meta, String name, String fallbackPrefix, Collection<String> aliases,
            BasicCommand command, Set<io.papermc.paper.command.brigadier.CommandRegistrationFlag> flags) {
        var builder = Commands.literal(name);
        builder.executes(ctx -> {
            command.execute(ctx.getSource(), arguments(ctx.getInput()));
            return 1;
        });
        LiteralCommandNode<CommandSourceStack> node = dispatcher.register(builder);
        return register(meta, node, fallbackPrefix, aliases);
    }

    /** Аргументы из входной строки: всё после первой команды, разбитое по пробелам. */
    private static String[] arguments(String input) {
        if (input == null) {
            return new String[0];
        }
        String[] parts = input.trim().split("\\s+");
        return parts.length <= 1 ? new String[0] : java.util.Arrays.copyOfRange(parts, 1, parts.length);
    }

    /** Текст как brigadier-сообщение (у него нет зависимости от adventure). */
    private static com.mojang.brigadier.Message string(String text) {
        return () -> text;
    }

    /** Источник команды для brigadier: отправитель и его позиция. */
    static CommandSourceStack source(CommandSender sender) {
        Location at = sender instanceof Entity e ? e.getLocation() : new Location(null, 0, 0, 0);
        return new Source(sender, at, sender instanceof Entity e ? e : null);
    }

    private record Source(CommandSender getSender, Location getLocation, Entity getExecutor)
            implements CommandSourceStack {

        @Override
        public Player getPlayerOrThrow() throws com.mojang.brigadier.exceptions.CommandSyntaxException {
            if (getSender instanceof Player p) {
                return p;
            }
            throw new com.mojang.brigadier.exceptions.SimpleCommandExceptionType(
                    string("You must be a player to run this command")).create();
        }

        @Override
        public Entity getEntityOrThrow() throws com.mojang.brigadier.exceptions.CommandSyntaxException {
            if (getExecutor != null) {
                return getExecutor;
            }
            throw new com.mojang.brigadier.exceptions.SimpleCommandExceptionType(
                    string("You must be a player to run this command")).create();
        }

        @Override
        public CommandSourceStack withLocation(Location location) {
            return new Source(getSender, location, getExecutor);
        }

        @Override
        public CommandSourceStack withExecutor(Entity executor) {
            return new Source(getSender, getLocation, executor);
        }
    }

    /** Команда бумажного регистратора как обычная команда сервера. */
    static final class BrigadierCommand extends BukkitCommand {
        private final LiteralCommandNode<CommandSourceStack> node;
        /** Плагин-владелец: по нему `/help <плагин>` собирает его команды. */
        final String owner;

        BrigadierCommand(String name, String description, String usage,
                LiteralCommandNode<CommandSourceStack> node, String owner) {
            super(name, description, usage, java.util.List.of());
            this.node = node;
            this.owner = owner;
        }

        @Override
        public boolean execute(CommandSender sender, String label, String[] args) {
            String line = args.length == 0 ? label : label + " " + String.join(" ", args);
            return LifecycleCommands.run(node, sender, line);
        }
    }

    /**
     * Исполняет brigadier-узел за отправителя: сначала ищет вложенный узел по
     * аргументам (так работает {@code /bluemap reload}), затем зовёт его
     * {@code exec}. Неизвестная подкоманда — ошибка, как в ваниле.
     */
    static boolean run(LiteralCommandNode<CommandSourceStack> node, CommandSender sender, String line) {
        CommandSourceStack source = source(sender);
        String[] parts = line.trim().split("\\s+");
        LiteralCommandNode<CommandSourceStack> current = node;
        int i = 1;
        while (i < parts.length) {
            var next = current.getChild(parts[i]);
            if (next == null) {
                break;
            }
            if (next instanceof LiteralCommandNode<CommandSourceStack> literal) {
                current = literal;
            }
            i++;
        }
        var exec = current.getCommand();
        if (exec == null) {
            sender.sendMessage(net.kyori.adventure.text.Component.text("Usage: " + node.getUsageText()));
            return false;
        }
        // Контекст несёт исходную строку и узел: плагины читают `getInput()`,
        // а аргументы у нас не разбираются — команда получает строку целиком.
        CommandContext<CommandSourceStack> ctx = new CommandContext<>(source, line, Map.of(), exec, current,
                java.util.List.of(),
                new com.mojang.brigadier.context.StringRange(0, line.length()), null, null, false);
        try {
            exec.run(ctx);
            return true;
        } catch (com.mojang.brigadier.exceptions.CommandSyntaxException e) {
            sender.sendMessage(net.kyori.adventure.text.Component.text(e.getMessage()));
            return true;
        } catch (RuntimeException e) {
            sender.sendMessage(net.kyori.adventure.text.Component.text(
                    "An internal error occurred while attempting to perform this command"));
            throw e;
        }
    }
}
