package dev.chunkwake.bridge;

import java.io.File;
import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.logging.Level;
import java.util.logging.Logger;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;
import net.kyori.adventure.text.serializer.gson.GsonComponentSerializer;
import net.kyori.adventure.text.serializer.legacy.LegacyComponentSerializer;
import org.bukkit.Bukkit;
import org.bukkit.Server;
import org.bukkit.World;
import org.bukkit.command.Command;
import org.bukkit.command.CommandException;
import org.bukkit.command.CommandMap;
import org.bukkit.command.CommandSender;
import org.bukkit.command.ConsoleCommandSender;
import org.bukkit.command.PluginCommand;
import org.bukkit.command.PluginCommandYamlParser;
import org.bukkit.command.SimpleCommandMap;
import org.bukkit.entity.Player;
import org.bukkit.event.block.BlockBreakEvent;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.event.player.PlayerQuitEvent;
import org.bukkit.plugin.PluginDescriptionFile;
import org.bukkit.plugin.PluginManager;
import org.bukkit.plugin.java.JavaPlugin;
import org.bukkit.scheduler.BukkitScheduler;

/** {@link Server} поверх Rust-ядра. Всё вызывается в потоке плагинов. */
final class ChunkwakeServer {
    static final String VERSION = "26.3";

    final Server api = Stub.of(Server.class, this);
    final Logger logger = Logger.getLogger("Chunkwake");
    private final Scheduler scheduler = new Scheduler();
    private final PluginManagerImpl plugins = new PluginManagerImpl(this);
    private final SimpleCommandMap commands = new SimpleCommandMap(api, new LinkedHashMap<>());
    private final Map<UUID, PlayerImpl> online = new LinkedHashMap<>();
    final ConsoleCommandSender console = Stub.of(ConsoleCommandSender.class, new Console(this));
    final World world = Stub.of(World.class, new WorldImpl());
    private final Thread mainThread = Thread.currentThread();

    Scheduler scheduler() {
        return scheduler;
    }

    /** Загружает jar из каталога и включает плагины. Возвращает число включённых. */
    int loadAndEnable(File dir) {
        Bukkit.setServer(api);
        silenceTimings();
        dir.mkdirs();
        File[] jars = dir.listFiles((d, n) -> n.endsWith(".jar"));
        if (jars == null) {
            return 0;
        }
        java.util.Arrays.sort(jars);
        List<JavaPlugin> loaded = new ArrayList<>();
        for (File jar : jars) {
            try {
                PluginDescriptionFile desc = PluginLoader.describe(jar);
                if (plugins.plugins.containsKey(desc.getName())) {
                    logger.warning("Duplicate plugin " + desc.getName() + " in " + jar.getName());
                    continue;
                }
                PluginLoader loader = new PluginLoader(this, jar, desc, getClass().getClassLoader());
                JavaPlugin plugin = loader.create();
                plugins.plugins.put(desc.getName(), plugin);
                logger.info("Loading " + desc.getFullName());
                plugin.onLoad();
                loaded.add(plugin);
            } catch (Throwable t) {
                logger.log(Level.SEVERE, "Could not load " + jar.getName(), t);
            }
        }
        int enabled = 0;
        for (JavaPlugin plugin : loaded) {
            commands.registerAll(plugin.getName().toLowerCase(java.util.Locale.ROOT),
                    PluginCommandYamlParser.parse(plugin));
            // Права из plugin.yml — до onEnable (`CraftServer.enablePlugin`).
            for (org.bukkit.permissions.Permission perm : plugin.getDescription().getPermissions()) {
                try {
                    plugins.addPermission(perm);
                } catch (IllegalArgumentException e) {
                    logger.log(Level.WARNING, "Plugin " + plugin.getName() + " tried to register permission '"
                            + perm.getName() + "' but it's already registered", e);
                }
            }
            plugins.enablePlugin(plugin);
            if (plugin.isEnabled()) {
                enabled++;
            }
        }
        publishCommands();
        return enabled;
    }

    /** paper-api при первом вызове команды шлёт предупреждение «timings в no-op режиме». */
    private void silenceTimings() {
        try {
            var f = co.aikar.timings.Timings.class.getDeclaredField("warnedAboutDeprecationOnEnable");
            f.setAccessible(true);
            f.setBoolean(null, true);
        } catch (ReflectiveOperationException | RuntimeException e) {
            logger.fine("timings: " + e);
        }
    }

    /** Имена команд для дерева `commands` у клиента (через '\n'). */
    private void publishCommands() {
        StringBuilder sb = new StringBuilder();
        for (Command c : commands.getCommands()) {
            for (String n : names(c)) {
                if (!sb.isEmpty()) sb.append('\n');
                sb.append(n);
            }
        }
        Bridge.setCommands(sb.toString());
    }

    private static List<String> names(Command c) {
        List<String> out = new ArrayList<>();
        out.add(c.getName());
        out.addAll(c.getAliases());
        return out;
    }

    // --- вызовы из Rust ---

    void join(UUID id, String name) {
        PlayerImpl p = new PlayerImpl(this, id, name);
        online.put(id, p);
        Component msg = Component.translatable("multiplayer.player.joined", NamedTextColor.YELLOW,
                Component.text(name));
        PlayerJoinEvent e = new PlayerJoinEvent(p.api, msg);
        plugins.callEvent(e);
        if (e.joinMessage() != null) {
            announce(e.joinMessage());
        }
    }

    void quit(UUID id) {
        PlayerImpl p = online.get(id);
        if (p == null) {
            return;
        }
        Component msg = Component.translatable("multiplayer.player.left", NamedTextColor.YELLOW,
                Component.text(p.name));
        PlayerQuitEvent e = new PlayerQuitEvent(p.api, msg, PlayerQuitEvent.QuitReason.DISCONNECTED);
        plugins.callEvent(e);
        online.remove(id);
        p.online = false;
        if (e.quitMessage() != null) {
            announce(e.quitMessage());
        }
    }

    /** true — событие отменено, блок надо вернуть. */
    boolean blockBreak(UUID id, int x, int y, int z, String key) {
        PlayerImpl p = online.get(id);
        if (p == null) {
            return false;
        }
        BlockImpl b = new BlockImpl(world, x, y, z, key);
        BlockBreakEvent e = new BlockBreakEvent(b.api, p.api);
        plugins.callEvent(e);
        return e.isCancelled();
    }

    /** Событие с ответом из Rust (формат — {@link Events}). */
    String event(int kind, UUID id, String arg) {
        PlayerImpl p = online.get(id);
        if (p == null) {
            return "";
        }
        if (kind == Bridge.EVENT_TAB_COMPLETE) {
            List<String> out = commands.tabComplete(p.api, arg);
            return out == null ? "" : String.join("\n", out);
        }
        return Events.fire(api.getPluginManager(), p, kind, arg);
    }

    /** true — команда известна (выполнена или упала с сообщением). */
    boolean command(UUID id, String line) {
        CommandSender sender = id.getMostSignificantBits() == 0 && id.getLeastSignificantBits() == 0
                ? console : online.containsKey(id) ? online.get(id).api : null;
        if (sender == null) {
            return false;
        }
        String label = line.split(" ", 2)[0];
        if (commands.getCommand(label) == null) {
            return false;
        }
        try {
            commands.dispatch(sender, line);
        } catch (CommandException ex) {
            logger.log(Level.SEVERE, "Error executing /" + line, ex);
            sender.sendMessage(Component.text("An internal error occurred while attempting to perform this command",
                    NamedTextColor.RED));
        }
        return true;
    }

    void announce(Component c) {
        Bridge.broadcast(GsonComponentSerializer.gson().serialize(c));
        logger.info(plain(c));
    }

    /** Ключи ванилы, которые сервер сам шлёт: текст для консоли. */
    private static final Map<String, String> CONSOLE_TEXT = Map.of(
            "multiplayer.player.joined", "%s joined the game",
            "multiplayer.player.left", "%s left the game");

    private static final net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer PLAIN =
            net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.builder()
                    .flattener(net.kyori.adventure.text.flattener.ComponentFlattener.basic().toBuilder()
                            .complexMapper(net.kyori.adventure.text.TranslatableComponent.class, (tc, out) -> {
                                String fmt = CONSOLE_TEXT.get(tc.key());
                                if (fmt == null) {
                                    out.accept(Component.text(tc.fallback() != null ? tc.fallback() : tc.key()));
                                    return;
                                }
                                Object[] args = tc.arguments().stream()
                                        .map(a -> PLAIN().serialize(a.asComponent())).toArray();
                                out.accept(Component.text(String.format(fmt, args)));
                            })
                            .build())
                    .build();

    private static net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer PLAIN() {
        return PLAIN;
    }

    /** Компонент как текст для консоли. */
    static String plain(Component c) {
        return PLAIN.serialize(c);
    }

    static Component legacy(String s) {
        return LegacyComponentSerializer.legacySection().deserialize(s);
    }

    // --- Server ---

    public String getName() { return "Chunkwake"; }
    /** Сообщение об отказе в правах — как `messages.no-permission` Paper по умолчанию. */
    public Component permissionMessage() {
        return Component.text("I'm sorry, but you do not have permission to perform this command. "
                + "Please contact the server administrators if you believe that this is in error.", NamedTextColor.RED);
    }
    public String getVersion() { return VERSION + "-chunkwake (MC: " + VERSION + ")"; }
    public String getBukkitVersion() { return VERSION + "-R0.1-SNAPSHOT"; }
    public String getMinecraftVersion() { return VERSION; }
    public Logger getLogger() { return logger; }
    public PluginManager getPluginManager() { return plugins.api; }
    public BukkitScheduler getScheduler() { return scheduler.api; }
    public CommandMap getCommandMap() { return commands; }
    public ConsoleCommandSender getConsoleSender() { return console; }
    public boolean isPrimaryThread() { return Thread.currentThread() == mainThread; }
    public int getMaxPlayers() { return 20; }
    public List<World> getWorlds() { return List.of(world); }

    public World getWorld(String name) {
        return world.getName().equals(name) ? world : null;
    }

    public Collection<? extends Player> getOnlinePlayers() {
        return online.values().stream().map(p -> p.api).toList();
    }

    public Player getPlayer(UUID id) {
        PlayerImpl p = online.get(id);
        return p == null ? null : p.api;
    }

    public Player getPlayerExact(String name) {
        for (PlayerImpl p : online.values()) {
            if (p.name.equalsIgnoreCase(name)) return p.api;
        }
        return null;
    }

    public Player getPlayer(String name) {
        Player exact = getPlayerExact(name);
        if (exact != null) return exact;
        String lower = name.toLowerCase(java.util.Locale.ROOT);
        for (PlayerImpl p : online.values()) {
            if (p.name.toLowerCase(java.util.Locale.ROOT).startsWith(lower)) return p.api;
        }
        return null;
    }

    public PluginCommand getPluginCommand(String name) {
        return commands.getCommand(name) instanceof PluginCommand pc ? pc : null;
    }

    public boolean dispatchCommand(CommandSender sender, String line) {
        return commands.dispatch(sender, line);
    }

    public int broadcastMessage(String message) {
        announce(legacy(message));
        return online.size();
    }

    public int broadcast(Component message) {
        announce(message);
        return online.size();
    }

    /** Консоль: сообщения уходят в лог. */
    static final class Console {
        private final ChunkwakeServer server;

        Console(ChunkwakeServer server) {
            this.server = server;
        }

        public String getName() { return "CONSOLE"; }
        public Server getServer() { return server.api; }
        public boolean isOp() { return true; }
        public boolean hasPermission(String p) { return true; }
        public boolean hasPermission(org.bukkit.permissions.Permission p) { return true; }
        public boolean isPermissionSet(String p) { return true; }
        public boolean isConversing() { return false; }
        public Component name() { return Component.text("CONSOLE"); }

        public void sendMessage(String m) {
            server.logger.info(m);
        }

        public void sendMessage(String... ms) {
            for (String m : ms) sendMessage(m);
        }

        public void sendMessage(Component c) {
            server.logger.info(plain(c));
        }
    }

    /** Единственный мир (MVP). */
    static final class WorldImpl {
        public String getName() { return "world"; }
        public org.bukkit.block.Block getBlockAt(int x, int y, int z) {
            return new BlockImpl(Bridge.server.world, x, y, z, null).api;
        }
        public org.bukkit.block.Block getBlockAt(org.bukkit.Location l) {
            return getBlockAt(l.getBlockX(), l.getBlockY(), l.getBlockZ());
        }
        public org.bukkit.Material getType(int x, int y, int z) { return getBlockAt(x, y, z).getType(); }
        public void setType(int x, int y, int z, org.bukkit.Material m) { getBlockAt(x, y, z).setType(m); }
        public UUID getUID() { return new UUID(0, 1); }
        public int getMinHeight() { return -64; }
        public int getMaxHeight() { return 320; }
    }
}
