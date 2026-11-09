package dev.chunkwake.bridge;

import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import org.bukkit.ChatColor;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.command.PluginIdentifiableCommand;
import org.bukkit.help.GenericCommandHelpTopic;
import org.bukkit.help.HelpMap;
import org.bukkit.help.HelpTopic;
import org.bukkit.help.HelpTopicComparator;
import org.bukkit.help.HelpTopicFactory;
import org.bukkit.help.IndexHelpTopic;

/**
 * {@link HelpMap} по образцу CraftBukkit {@code SimpleHelpMap}: тема на каждую
 * команду из {@code CommandMap}, алиасы, индекс по плагину и общий индекс.
 * Без {@code help.yml}: своих тем и поправок у сервера нет.
 */
final class HelpMapImpl implements HelpMap {
    private final Map<String, HelpTopic> topics = new TreeMap<>(HelpTopicComparator.topicNameComparatorInstance());
    private final Map<Class<?>, HelpTopicFactory<Command>> factories = new HashMap<>();
    private HelpTopic index = index(List.of());

    @Override
    public synchronized HelpTopic getHelpTopic(String name) {
        return name.isEmpty() ? index : topics.get(name);
    }

    @Override
    public synchronized Collection<HelpTopic> getHelpTopics() {
        return List.copyOf(topics.values());
    }

    @Override
    public synchronized void addTopic(HelpTopic topic) {
        topics.putIfAbsent(topic.getName(), topic);
    }

    @Override
    public synchronized void clear() {
        topics.clear();
        index = index(List.of());
    }

    @Override
    @SuppressWarnings("unchecked")
    public void registerHelpTopicFactory(Class<?> commandClass, HelpTopicFactory<?> factory) {
        if (!Command.class.isAssignableFrom(commandClass) && !org.bukkit.command.CommandExecutor.class.isAssignableFrom(commandClass)) {
            throw new IllegalArgumentException("commandClass must implement either Command or CommandExecutor!");
        }
        factories.put(commandClass, (HelpTopicFactory<Command>) factory);
    }

    @Override
    public List<String> getIgnoredPlugins() {
        return List.of();
    }

    /** Пересобрать темы по командам: после загрузки плагинов и {@code /reload}. */
    synchronized void rebuild(Collection<Command> commands) {
        topics.clear();
        Map<String, List<HelpTopic>> byPlugin = new TreeMap<>(String.CASE_INSENSITIVE_ORDER);
        List<Command> unique = new ArrayList<>();
        for (Command c : commands) {
            if (!unique.contains(c)) {
                unique.add(c);
            }
        }
        for (Command c : unique) {
            HelpTopic topic = topicFor(c);
            if (topic == null) {
                continue;
            }
            topics.putIfAbsent(topic.getName(), topic);
            byPlugin.computeIfAbsent(owner(c), k -> new ArrayList<>()).add(topic);
        }
        // Алиасы — после команд: имя команды важнее чужого алиаса.
        List<HelpTopic> aliases = new ArrayList<>();
        for (Command c : unique) {
            for (String alias : c.getAliases()) {
                String name = "/" + alias;
                if (!topics.containsKey(name)) {
                    HelpTopic topic = new Alias(name, "/" + c.getLabel(), this);
                    topics.put(name, topic);
                    aliases.add(topic);
                }
            }
        }
        List<HelpTopic> master = new ArrayList<>(topics.values());
        if (!aliases.isEmpty()) {
            topics.put("Aliases", new IndexHelpTopic("Aliases", "Lists command aliases", null, aliases));
        }
        for (var e : byPlugin.entrySet()) {
            String plugin = e.getKey();
            topics.putIfAbsent(plugin, new IndexHelpTopic(plugin, "All commands for " + plugin, null, e.getValue(),
                    "Below is a list of all " + plugin + " commands:"));
        }
        index = index(master);
    }

    private HelpTopic topicFor(Command c) {
        for (Class<?> type = c.getClass(); type != null; type = type.getSuperclass()) {
            HelpTopicFactory<Command> f = factories.get(type);
            if (f != null) {
                return f.createTopic(c);
            }
        }
        if (c instanceof org.bukkit.command.PluginCommand pc) {
            HelpTopicFactory<Command> f = factories.get(pc.getExecutor().getClass());
            if (f != null) {
                return f.createTopic(c);
            }
        }
        return new GenericCommandHelpTopic(c);
    }

    /** Индекс в {@code /help}: плагин команды или «Bukkit» для встроенных. */
    private static String owner(Command c) {
        if (c instanceof PluginIdentifiableCommand p) {
            return p.getPlugin().getName();
        }
        if (c instanceof LifecycleCommands.BrigadierCommand b) {
            return b.owner;
        }
        return "Bukkit";
    }

    private static HelpTopic index(Collection<HelpTopic> all) {
        List<HelpTopic> sorted = new ArrayList<>(all);
        sorted.sort(HelpTopicComparator.helpTopicComparatorInstance());
        return new IndexHelpTopic("Index", null, null, sorted, "Use /help [n] to get page n of help.");
    }

    /** CraftBukkit {@code CommandAliasHelpTopic}. */
    static final class Alias extends HelpTopic {
        private final String aliasFor;
        private final HelpMap map;

        Alias(String alias, String aliasFor, HelpMap map) {
            this.aliasFor = aliasFor.startsWith("/") ? aliasFor : "/" + aliasFor;
            this.map = map;
            this.name = alias.startsWith("/") ? alias : "/" + alias;
            this.shortText = ChatColor.YELLOW + "Alias for " + ChatColor.WHITE + this.aliasFor;
        }

        @Override
        public String getFullText(CommandSender forWho) {
            StringBuilder sb = new StringBuilder(shortText);
            HelpTopic target = map.getHelpTopic(aliasFor);
            if (target != null) {
                sb.append("\n").append(target.getFullText(forWho));
            }
            return sb.toString();
        }

        @Override
        public boolean canSee(CommandSender sender) {
            if (amendedPermission != null) {
                return sender.hasPermission(amendedPermission);
            }
            HelpTopic target = map.getHelpTopic(aliasFor);
            return target != null && target.canSee(sender);
        }
    }
}
