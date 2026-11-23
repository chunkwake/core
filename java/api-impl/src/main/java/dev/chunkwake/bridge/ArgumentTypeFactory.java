package dev.chunkwake.bridge;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import io.papermc.paper.command.brigadier.CommandSourceStack;
import io.papermc.paper.command.brigadier.argument.resolvers.selector.EntitySelectorArgumentResolver;
import io.papermc.paper.command.brigadier.argument.resolvers.selector.PlayerSelectorArgumentResolver;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import org.bukkit.Bukkit;
import org.bukkit.entity.Entity;
import org.bukkit.entity.Player;

/**
 * Реальные {@link ArgumentType} для {@code VanillaArgumentProvider} (Paper
 * {@code ArgumentTypes.*}).
 *
 * <p>Раньше провайдер возвращал {@code null}, и любой плагин, строящий
 * команду через Paper/Brigadier (zMenu), падал на
 * {@code Preconditions.checkNotNull("Argument type cannot be null")}.
 *
 * <p>Селекторы игроков/сущностей разбирают один токен (имя или {@code @...})
 * и на исполнении возвращают подходящих онлайн-игроков. Остальные типы
 * (blockState, itemStack, ...) пока разбираются как слово: команда строится,
 * а конкретное значение проверяется уже обработчиком плагина.
 */
public final class ArgumentTypeFactory {
    private ArgumentTypeFactory() {
    }

    @SuppressWarnings("unchecked")
    public static <T> ArgumentType<T> byName(String name) {
        return (ArgumentType<T>) switch (name) {
            case "player", "players" -> new SelectorType(true);
            case "entity", "entities" -> new SelectorType(false);
            case "itemStack", "item" -> new ItemStackType();
            default -> new WordType();
        };
    }

    /** Один токен без кавычек: имя игрока, селектор или обычное слово. */
    private static final class WordType implements ArgumentType<String> {
        @Override
        public String parse(StringReader reader) throws CommandSyntaxException {
            return reader.readUnquotedString();
        }

        @Override
        public Collection<String> getExamples() {
            return List.of("value");
        }
    }

    /** {@code ArgumentTypes.itemStack()}: Skript парсит item stack через Brigadier. */
    private static final class ItemStackType implements ArgumentType<org.bukkit.inventory.ItemStack> {
        @Override
        public org.bukkit.inventory.ItemStack parse(StringReader reader) throws CommandSyntaxException {
            int start = reader.getCursor();
            int depth = 0;
            while (reader.canRead()) {
                char c = reader.peek();
                if (c == '[') {
                    depth++;
                } else if (c == ']' && depth > 0) {
                    depth--;
                } else if (Character.isWhitespace(c) && depth == 0) {
                    break;
                }
                reader.skip();
            }
            String token = reader.getString().substring(start, reader.getCursor());
            if (token.isEmpty()) {
                throw CommandSyntaxException.BUILT_IN_EXCEPTIONS.dispatcherUnknownArgument()
                        .createWithContext(reader);
            }
            int bracket = token.indexOf('[');
            String id = bracket < 0 ? token : token.substring(0, bracket);
            org.bukkit.Material material = org.bukkit.Material.matchMaterial(id);
            if (material == null || !material.isItem()) {
                throw CommandSyntaxException.BUILT_IN_EXCEPTIONS.dispatcherUnknownArgument()
                        .createWithContext(reader);
            }
            return new org.bukkit.inventory.ItemStack(material);
        }

        @Override
        public Collection<String> getExamples() {
            return List.of("minecraft:stone", "minecraft:diamond_sword");
        }
    }

    private static final class SelectorType implements ArgumentType<Object> {
        private final boolean players;

        SelectorType(boolean players) {
            this.players = players;
        }

        @Override
        public Object parse(StringReader reader) throws CommandSyntaxException {
            String token = reader.readUnquotedString();
            if (this.players) {
                return (PlayerSelectorArgumentResolver) source -> resolvePlayers(token);
            }
            return (EntitySelectorArgumentResolver) source -> resolveEntities(token);
        }

        @Override
        public Collection<String> getExamples() {
            return this.players ? List.of("Player", "@a") : List.of("@e", "@a");
        }
    }

    private static List<Player> resolvePlayers(String token) {
        if (token.startsWith("@")) {
            return new ArrayList<>(Bukkit.getOnlinePlayers());
        }
        Player player = Bukkit.getPlayerExact(token);
        return player == null ? List.of() : List.of(player);
    }

    private static List<Entity> resolveEntities(String token) {
        return new ArrayList<>(resolvePlayers(token));
    }

    /** Для отладки: какие типы уже отдаём по-настоящему. */
    public static String describe(String name) {
        return switch (name) {
            case "player", "players", "entity", "entities" -> "selector";
            default -> "word";
        };
    }
}
