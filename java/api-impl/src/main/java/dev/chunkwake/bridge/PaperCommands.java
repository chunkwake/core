package dev.chunkwake.bridge;

import com.google.common.collect.Lists;
import io.papermc.paper.plugin.configuration.PluginMeta;
import java.text.DecimalFormat;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;
import java.util.TreeMap;
import java.util.function.Supplier;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.JoinConfiguration;
import net.kyori.adventure.text.TextComponent;
import net.kyori.adventure.text.event.ClickEvent;
import net.kyori.adventure.text.format.NamedTextColor;
import net.kyori.adventure.text.format.TextColor;
import net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer;
import org.bukkit.Bukkit;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.command.defaults.BukkitCommand;
import org.bukkit.plugin.Plugin;
import org.bukkit.util.StringUtil;

/**
 * Команды, которые Paper регистрирует сам ({@code io.papermc.paper.command.PaperCommands}):
 * {@code /plugins}, {@code /version}, {@code /mspt}. Вывод повторяет Paper 26.3 —
 * тексты, цвета, клики и наведение сверены с его байткодом.
 */
final class PaperCommands {
    private PaperCommands() {}

    static List<Command> all(Supplier<String> mspt) {
        return List.of(new Plugins(), new Version(), new Mspt(mspt));
    }

    // --- /plugins ---

    /** {@code PaperPluginsCommand}. */
    static final class Plugins extends BukkitCommand {
        private static final TextColor INFO_COLOR = TextColor.color(52, 159, 218);
        private static final Component SERVER_PLUGIN_INFO = Component.text("ℹ What is a server plugin?", INFO_COLOR)
                .append(plain("Server plugins can add new behavior to your server!\n"
                        + "You can find new plugins on Paper's plugin repository, Hangar.\n\n"
                        + "https://hangar.papermc.io/\n"));
        private static final Component LEGACY_PLUGIN_INFO = Component.text("ℹ What is a legacy plugin?", INFO_COLOR)
                .append(plain("A legacy plugin is a plugin that was made on\n"
                        + "very old unsupported versions of the game.\n\n"
                        + "It is encouraged that you replace this plugin,\n"
                        + "as they might not work in the future and may cause\n"
                        + "performance issues.\n"));
        private static final Component LEGACY_PLUGIN_STAR = Component.text('*', TextColor.color(255, 212, 42))
                .hoverEvent(LEGACY_PLUGIN_INFO);
        private static final Component INFO_ICON_SERVER_PLUGIN = Component.text("ℹ ", INFO_COLOR)
                .hoverEvent(SERVER_PLUGIN_INFO)
                .clickEvent(ClickEvent.openUrl("https://docs.papermc.io/paper/adding-plugins"));
        private static final Component PLUGIN_TICK = Component.text("- ", NamedTextColor.DARK_GRAY);
        private static final Component PLUGIN_TICK_EMPTY = Component.text(" ");

        Plugins() {
            super("plugins");
            setDescription("Gets a list of plugins running on the server");
            setUsage("/plugins");
            setPermission("bukkit.command.plugins");
            setAliases(List.of("pl"));
        }

        @Override
        public boolean execute(CommandSender sender, String label, String[] args) {
            if (!testPermission(sender)) {
                return true;
            }
            TreeMap<String, Plugin> paper = new TreeMap<>(String.CASE_INSENSITIVE_ORDER);
            TreeMap<String, Plugin> bukkit = new TreeMap<>(String.CASE_INSENSITIVE_ORDER);
            for (Plugin p : Bukkit.getPluginManager().getPlugins()) {
                String name = p.getPluginMeta().getDisplayName();
                (isPaper(p) ? paper : bukkit).put(name, p);
            }
            for (Component line : render(paper, bukkit)) {
                sender.sendMessage(line);
            }
            return true;
        }

        /** Строки ответа: заголовок, затем секции Paper и Bukkit. */
        static List<Component> render(TreeMap<String, Plugin> paper, TreeMap<String, Plugin> bukkit) {
            List<Component> out = new ArrayList<>();
            int total = paper.size() + bukkit.size();
            boolean both = !paper.isEmpty() && !bukkit.isEmpty();
            out.add(Component.text()
                    .append(INFO_ICON_SERVER_PLUGIN)
                    .append(Component.text("Server Plugins (%s):".formatted(total), NamedTextColor.WHITE))
                    .build());
            if (!paper.isEmpty()) {
                out.add(header("Paper Plugins", 0x0288D1, paper.size(), both));
                out.addAll(lines(paper));
            }
            if (!bukkit.isEmpty()) {
                out.add(header("Bukkit Plugins", 0xED8106, bukkit.size(), both));
                out.addAll(lines(bukkit));
            }
            return out;
        }

        private static List<Component> lines(TreeMap<String, Plugin> plugins) {
            List<Component> names = new ArrayList<>(plugins.size());
            for (Plugin p : plugins.values()) {
                names.add(entry(p));
            }
            List<Component> out = new ArrayList<>();
            boolean first = true;
            for (List<Component> part : Lists.partition(names, 10)) {
                Component line = Component.space();
                if (first) {
                    line = line.append(PLUGIN_TICK);
                    first = false;
                } else {
                    line = PLUGIN_TICK_EMPTY;
                }
                out.add(line.append(Component.join(JoinConfiguration.commas(true), part)));
            }
            return out;
        }

        private static Component entry(Plugin p) {
            TextComponent.Builder b = Component.text();
            if (!isPaper(p) && p.getDescription().getAPIVersion() == null) {
                b.append(LEGACY_PLUGIN_STAR);
            }
            String name = p.getPluginMeta().getName();
            b.append(Component.text(name, p.isEnabled() ? NamedTextColor.GREEN : NamedTextColor.RED)
                    .clickEvent(ClickEvent.runCommand("/version " + name)));
            return b.build();
        }

        private static Component header(String title, int color, int count, boolean showSize) {
            TextComponent.Builder b = Component.text().color(TextColor.color(color)).append(Component.text(title));
            if (showSize) {
                b.appendSpace().append(Component.text("(" + count + ")"));
            }
            return b.append(Component.text(":")).build();
        }

        private static Component plain(String text) {
            TextComponent.Builder b = Component.text();
            for (String line : text.split("\n")) {
                b.append(Component.newline()).append(Component.text(line, NamedTextColor.WHITE));
            }
            return b.build();
        }

        static boolean isPaper(Plugin p) {
            return p.getClass().getClassLoader() instanceof PluginLoader l && l.paperPlugin;
        }
    }

    // --- /version ---

    /** {@code PaperVersionCommand}: без проверки обновлений по сети. */
    static final class Version extends BukkitCommand {
        private static final Component NOT_RUNNING = Component.text()
                .append(Component.text("This server is not running any plugin by that name."))
                .appendNewline()
                .append(Component.text("Use /plugins to get a list of plugins.")
                        .clickEvent(ClickEvent.suggestCommand("/plugins")))
                .build();
        private static final JoinConfiguration NAMES = JoinConfiguration.separators(
                Component.text(", ", NamedTextColor.WHITE), Component.text(", and ", NamedTextColor.WHITE));

        Version() {
            super("version");
            setDescription("Gets the version of this server including any plugins in use");
            setUsage("/version [plugin name]");
            setPermission("bukkit.command.version");
            setAliases(List.of("ver", "about"));
        }

        @Override
        public boolean execute(CommandSender sender, String label, String[] args) {
            if (!testPermission(sender)) {
                return true;
            }
            if (args.length == 0) {
                sender.sendMessage(serverVersion());
                return true;
            }
            String name = String.join(" ", args).toLowerCase(Locale.ROOT);
            Plugin plugin = find(name);
            sender.sendMessage(plugin == null ? NOT_RUNNING : pluginInfo(plugin));
            return true;
        }

        /** Точное имя, иначе любой плагин, в имени которого есть строка. */
        static Plugin find(String lower) {
            Plugin exact = Bukkit.getPluginManager().getPlugin(lower);
            if (exact != null) {
                return exact;
            }
            return Arrays.stream(Bukkit.getPluginManager().getPlugins())
                    .filter(p -> p.getName().toLowerCase(Locale.ROOT).contains(lower))
                    .findAny().orElse(null);
        }

        static Component serverVersion() {
            Component version = Component.textOfChildren(
                    Component.text(Bukkit.getVersionMessage(), NamedTextColor.WHITE),
                    Component.newline(),
                    Component.text("You are running a development version without access to version information",
                            NamedTextColor.YELLOW));
            return version.hoverEvent(Component.translatable("chat.copy.click", NamedTextColor.WHITE))
                    .clickEvent(ClickEvent.copyToClipboard(PlainTextComponentSerializer.plainText().serialize(version)));
        }

        static Component pluginInfo(Plugin plugin) {
            PluginMeta meta = plugin.getPluginMeta();
            TextComponent.Builder b = Component.text()
                    .append(Component.text(meta.getName()))
                    .append(Component.text(" version "))
                    .append(Component.text(meta.getVersion(), NamedTextColor.GREEN)
                            .hoverEvent(Component.translatable("chat.copy.click"))
                            .clickEvent(ClickEvent.copyToClipboard(meta.getVersion())));
            if (meta.getDescription() != null) {
                b.appendNewline().append(Component.text(meta.getDescription()));
            }
            if (meta.getWebsite() != null) {
                Component site = Component.text(meta.getWebsite(), NamedTextColor.GREEN)
                        .clickEvent(ClickEvent.openUrl(meta.getWebsite()));
                b.appendNewline().append(Component.text("Website: ").append(site));
            }
            if (!meta.getAuthors().isEmpty()) {
                String title = meta.getAuthors().size() == 1 ? "Author: " : "Authors: ";
                b.appendNewline().append(Component.text(title).append(names(meta.getAuthors())));
            }
            if (!meta.getContributors().isEmpty()) {
                b.appendNewline().append(Component.text("Contributors: ").append(names(meta.getContributors())));
            }
            return b.build();
        }

        private static Component names(List<String> names) {
            return Component.join(NAMES, names.stream().map(Component::text).toList()).color(NamedTextColor.GREEN);
        }

        @Override
        public List<String> tabComplete(CommandSender sender, String alias, String[] args) {
            if (args.length != 1) {
                return List.of();
            }
            List<String> out = new ArrayList<>();
            for (Plugin p : Bukkit.getPluginManager().getPlugins()) {
                if (StringUtil.startsWithIgnoreCase(p.getName(), args[0])) {
                    out.add(p.getName());
                }
            }
            return out;
        }
    }

    // --- /mspt ---

    /** {@code PaperMSPTCommand}: работа тика за 5 с, 10 с и 1 мин — из Rust. */
    static final class Mspt extends BukkitCommand {
        private static final Component SLASH = Component.text("/");
        private final Supplier<String> source;

        Mspt(Supplier<String> source) {
            super("mspt");
            this.source = source;
            setDescription("View server tick times");
            setUsage("/mspt");
            setPermission("bukkit.command.mspt");
        }

        @Override
        public boolean execute(CommandSender sender, String label, String[] args) {
            if (!testPermission(sender)) {
                return true;
            }
            for (Component line : render(source.get())) {
                sender.sendMessage(line);
            }
            return true;
        }

        /** Ответ Rust: девять чисел (avg min max за 5 с, 10 с, 1 мин), `?` — нет замеров. */
        static List<Component> render(String reply) {
            String[] f = reply.trim().split(" ");
            List<Component> times = new ArrayList<>(9);
            for (int i = 0; i < 9; i++) {
                times.add(i < f.length ? color(f[i]) : Component.text("?"));
            }
            Component head = Component.text()
                    .content("Server tick times ").color(NamedTextColor.GOLD)
                    .append(Component.text().color(NamedTextColor.YELLOW).append(
                            Component.text("("),
                            Component.text("avg", NamedTextColor.GRAY),
                            Component.text("/"),
                            Component.text("min", NamedTextColor.GRAY),
                            Component.text("/"),
                            Component.text("max", NamedTextColor.GRAY),
                            Component.text(")")))
                    .append(Component.text(" from last 5s"),
                            Component.text(",", NamedTextColor.GRAY),
                            Component.text(" 10s"),
                            Component.text(",", NamedTextColor.GRAY),
                            Component.text(" 1m"),
                            Component.text(":", NamedTextColor.YELLOW))
                    .build();
            Component comma = Component.text(", ", NamedTextColor.YELLOW);
            Component values = Component.text()
                    .content("◴ ").color(NamedTextColor.GOLD)
                    .append(Component.text().color(NamedTextColor.GRAY).append(
                            times.get(0), SLASH, times.get(1), SLASH, times.get(2), comma,
                            times.get(3), SLASH, times.get(4), SLASH, times.get(5), comma,
                            times.get(6), SLASH, times.get(7), SLASH, times.get(8)))
                    .build();
            return List.of(head, values);
        }

        /** {@code getColor}: от 50 мс — красный, от 40 — жёлтый, иначе зелёный. */
        private static Component color(String raw) {
            double ms;
            try {
                ms = Double.parseDouble(raw);
            } catch (NumberFormatException e) {
                return Component.text("?");
            }
            String text = new DecimalFormat("########0.0").format(ms);
            return Component.text(text, ms >= 50 ? NamedTextColor.RED
                    : ms >= 40 ? NamedTextColor.YELLOW : NamedTextColor.GREEN);
        }
    }
}
