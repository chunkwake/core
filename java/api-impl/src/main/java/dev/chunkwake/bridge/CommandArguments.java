package dev.chunkwake.bridge;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import io.papermc.paper.command.brigadier.CommandSourceStack;
import io.papermc.paper.command.brigadier.argument.resolvers.PlayerProfileListResolver;
import io.papermc.paper.command.brigadier.argument.resolvers.selector.PlayerSelectorArgumentResolver;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Comparator;
import java.util.List;
import org.bukkit.Bukkit;
import org.bukkit.Location;
import org.bukkit.entity.Player;

/**
 * {@code VanillaArgumentProvider.player/players/playerProfiles} для Chunkwake.
 *
 * <p>Резолверы работают через Bukkit API ({@link CommandSourceStack#getSender()},
 * {@link CommandSourceStack#getLocation()}), без NMS. Поддерживаются имена игроков
 * и базовые селекторы {@code @a/@p/@r/@s} с {@code limit=}/{@code distance=}.
 */
public final class CommandArguments {
    private CommandArguments() {}

    /** {@code VanillaArgumentProvider.player()}. */
    public static ArgumentType<PlayerSelectorArgumentResolver> player() {
        return Selector.INSTANCE;
    }

    /** {@code VanillaArgumentProvider.players()}. */
    public static ArgumentType<PlayerSelectorArgumentResolver> players() {
        return Selector.INSTANCE;
    }

    /** {@code VanillaArgumentProvider.playerProfiles()}. */
    public static ArgumentType<PlayerProfileListResolver> playerProfiles() {
        return Profiles.INSTANCE;
    }

    private static String readToken(StringReader reader) throws CommandSyntaxException {
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
        if (reader.getCursor() == start) {
            throw CommandSyntaxException.BUILT_IN_EXCEPTIONS.dispatcherUnknownArgument()
                    .createWithContext(reader);
        }
        return reader.getString().substring(start, reader.getCursor());
    }

    private static String base(String token) {
        int b = token.indexOf('[');
        return b < 0 ? token : token.substring(0, b);
    }

    private static String options(String token) {
        int b = token.indexOf('[');
        if (b < 0 || !token.endsWith("]")) {
            return "";
        }
        return token.substring(b + 1, token.length() - 1);
    }

    private static List<Player> resolve(String token, CommandSourceStack source, boolean throwUnknown)
            throws CommandSyntaxException {
        String base = base(token);
        String options = options(token);
        List<Player> out = new ArrayList<>();
        switch (base) {
            case "@a" -> out.addAll(Bukkit.getOnlinePlayers());
            case "@s" -> {
                if (source.getSender() instanceof Player p) {
                    out.add(p);
                }
            }
            case "@p", "@r" -> {
                Location at = source.getLocation();
                Player sender = source.getSender() instanceof Player p ? p : null;
                List<Player> all = new ArrayList<>(Bukkit.getOnlinePlayers());
                if (at != null) {
                    all.sort(Comparator.comparingDouble(p -> p.getLocation().distanceSquared(at)));
                } else if (sender != null) {
                    all.remove(sender);
                    all.add(0, sender);
                }
                if (!all.isEmpty()) {
                    out.add(base.equals("@r")
                            ? all.get(java.util.concurrent.ThreadLocalRandom.current().nextInt(all.size()))
                            : all.get(0));
                }
            }
            default -> {
                Player p = Bukkit.getPlayerExact(base);
                if (p != null) {
                    out.add(p);
                }
            }
        }
        // distance=..N и limit=N (как у ванильных селекторов).
        Location at = source.getLocation();
        for (String part : options.split(",")) {
            String opt = part.trim();
            if (opt.startsWith("distance=") && at != null) {
                String range = opt.substring("distance=".length());
                int dots = range.indexOf("..");
                double min = Double.NaN;
                double max = Double.NaN;
                if (dots < 0) {
                    max = parse(range);
                } else {
                    min = parse(range.substring(0, dots));
                    max = parse(range.substring(dots + 2));
                }
                double lo = min;
                double hi = max;
                out.removeIf(p -> {
                    double d = p.getLocation().distance(at);
                    return (!Double.isNaN(lo) && d < lo) || (!Double.isNaN(hi) && d > hi);
                });
            } else if (opt.startsWith("limit=")) {
                int limit = (int) parse(opt.substring("limit=".length()));
                if (limit > 0 && out.size() > limit) {
                    out.subList(limit, out.size()).clear();
                }
            }
        }
        if (out.isEmpty() && throwUnknown && !base.startsWith("@")) {
            throw CommandSyntaxException.BUILT_IN_EXCEPTIONS.dispatcherUnknownArgument()
                    .create();
        }
        return out;
    }

    private static double parse(String v) {
        try {
            return Double.parseDouble(v);
        } catch (NumberFormatException e) {
            return Double.NaN;
        }
    }

    private enum Selector implements ArgumentType<PlayerSelectorArgumentResolver> {
        INSTANCE;

        @Override
        public PlayerSelectorArgumentResolver parse(StringReader reader) throws CommandSyntaxException {
            String token = readToken(reader);
            return source -> resolve(token, source, true);
        }

        @Override
        public Collection<String> getExamples() {
            return List.of("@a", "@p", "Notch");
        }
    }

    private enum Profiles implements ArgumentType<PlayerProfileListResolver> {
        INSTANCE;

        @Override
        public PlayerProfileListResolver parse(StringReader reader) throws CommandSyntaxException {
            String token = readToken(reader);
            return source -> {
                List<com.destroystokyo.paper.profile.PlayerProfile> out = new ArrayList<>();
                for (Player p : resolve(token, source, false)) {
                    out.add(p.getPlayerProfile());
                }
                if (out.isEmpty() && !token.startsWith("@")) {
                    out.add(Bukkit.createProfile(token));
                }
                return out;
            };
        }

        @Override
        public Collection<String> getExamples() {
            return List.of("@a", "Notch");
        }
    }
}
