package dev.chunkwake.bridge;

import io.papermc.paper.plugin.configuration.PluginMeta;
import io.papermc.paper.plugin.lifecycle.event.LifecycleEvent;
import io.papermc.paper.plugin.lifecycle.event.handler.LifecycleEventHandler;
import io.papermc.paper.plugin.lifecycle.event.registrar.ReloadableRegistrarEvent;
import io.papermc.paper.command.brigadier.Commands;
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
    private final FoliaSchedulers folia = new FoliaSchedulers(scheduler);
    final PluginManagerImpl plugins = new PluginManagerImpl(this);
    private final SimpleCommandMap commands = new SimpleCommandMap(api, new LinkedHashMap<>());
    /** `/help`: темы по командам, пересобираются после загрузки плагинов. */
    private final HelpMapImpl help = new HelpMapImpl();
    /** Каталог плагинов: его заново читает `/reload`. */
    private File pluginsDir;
    /** Права `bukkit.*` зарегистрированы. */
    private boolean corePermissions;
    /** ViaVersion из `plugins/` как библиотека (см. {@link ViaLoader}). */
    private final ViaLoader via = new ViaLoader();
    /** Загрузчики плагинов, которые видят классы друг друга. */
    private final PluginClassGroup classGroup = new PluginClassGroup(logger);
    private final Map<UUID, PlayerImpl> online = new LinkedHashMap<>();
    final ConsoleCommandSender console = Stub.of(ConsoleCommandSender.class, new Console(this));
    final World world = Stub.of(World.class, new WorldImpl());
    private final Thread mainThread = Thread.currentThread();
    /** Каталог мира: его читают плагины (BlueMap разбирает регионы сам). */
    private final java.nio.file.Path levelDirectory;

    ChunkwakeServer(java.nio.file.Path levelDirectory) {
        this.levelDirectory = levelDirectory;
    }

    Scheduler scheduler() {
        return scheduler;
    }

    /** Планировщики Folia: регион один — главный тик плагинов. */
    FoliaSchedulers folia() {
        return folia;
    }

    /** Каналы плагинов: регистрация как в Bukkit, доставка — через Rust. */
    private final org.bukkit.plugin.messaging.StandardMessenger messenger =
            new org.bukkit.plugin.messaging.StandardMessenger();
    private final org.bukkit.plugin.SimpleServicesManager services = new org.bukkit.plugin.SimpleServicesManager();
    private final org.bukkit.UnsafeValues unsafe = Stub.of(org.bukkit.UnsafeValues.class, new Unsafe());
    /** {@code Server.isStopping}: с начала выключения плагинов. */
    private volatile boolean stopping;

    void disablePlugins() {
        stopping = true;
        plugins.disablePlugins();
        via.stop(logger);
    }

    public org.bukkit.plugin.messaging.Messenger getMessenger() { return messenger; }
    public org.bukkit.plugin.ServicesManager getServicesManager() { return services; }
    public boolean isStopping() { return stopping; }
    public org.bukkit.UnsafeValues getUnsafe() { return unsafe; }

    /** {@code Server.sendPluginMessage}: всем игрокам, кто слушает канал. */
    public void sendPluginMessage(org.bukkit.plugin.Plugin source, String channel, byte[] message) {
        org.bukkit.plugin.messaging.StandardMessenger.validatePluginMessage(messenger, source, channel, message);
        for (PlayerImpl p : List.copyOf(online.values())) {
            p.sendPluginMessage(source, channel, message);
        }
    }

    public java.util.Set<String> getListeningPluginChannels() {
        java.util.Set<String> out = new java.util.HashSet<>();
        for (PlayerImpl p : List.copyOf(online.values())) {
            out.addAll(p.getListeningPluginChannels());
        }
        return out;
    }

    /** Плагин выключен: снять его каналы и сервисы, как {@code SimplePluginManager}. */
    void pluginDisabled(org.bukkit.plugin.Plugin plugin) {
        messenger.unregisterIncomingPluginChannel(plugin);
        messenger.unregisterOutgoingPluginChannel(plugin);
        services.unregisterAll(plugin);
    }

    /** {@code UnsafeValues}: только версии, их спрашивают ViaVersion и ProtocolLib. */
    public static final class Unsafe {
        static final int PROTOCOL = 777;
        public int getProtocolVersion() { return PROTOCOL; }
        public int getDataVersion() { return 5023; }
    }

    /** Загружает jar из каталога и включает плагины. Возвращает число включённых. */
    int loadAndEnable(File dir) {
        pluginsDir = dir;
        if (Bukkit.getServer() == null) {
            Bukkit.setServer(api);
        }
        silenceTimings();
        dir.mkdirs();
        File[] jars = dir.listFiles((d, n) -> n.endsWith(".jar"));
        if (jars == null) {
            return 0;
        }
        java.util.Arrays.sort(jars);
        // Via — библиотекой, до остальных: её загрузчик станет им родителем.
        jars = via.claim(jars, logger).toArray(File[]::new);
        via.start(dir, this, logger);
        if (via.host != null) {
            commands.register("viaversion", via.command());
        }
        registerBuiltins();
        // Загрузка плагинов останавливает сервер на всё время `onEnable`: тик
        // ещё не пошёл, игроки ждут. Без сообщений это выглядело как зависание,
        // поэтому каждая фаза объявляется заранее — видно, на чём именно стоит.
        logger.info("plugins: " + jars.length + " jar(s) in " + dir + ", loading...");
        long started = System.nanoTime();
        List<PluginOrder.Candidate> candidates = new ArrayList<>();
        for (File jar : jars) {
            try {
                candidates.add(new PluginOrder.Candidate(jar, PluginLoader.describe(jar)));
            } catch (Throwable t) {
                logger.log(Level.SEVERE, "Could not load " + jar.getName() + ": " + t.getMessage());
            }
        }
        // Как Paper: зависимости — раньше зависящих, `loadbefore` — наоборот.
        List<PluginOrder.Candidate> order = PluginOrder.sort(candidates, plugins.plugins.keySet(), logger);
        // Как на Paper: реестры ванили подняты до первого класса плагина.
        VanillaBootstrap.await();
        List<JavaPlugin> loaded = new ArrayList<>();
        // Имена и `provides` загруженных: по ним проверяются `depend`.
        java.util.Set<String> loadedNames = new java.util.HashSet<>(plugins.plugins.keySet());
        for (int i = 0; i < order.size(); i++) {
            File jar = order.get(i).jar();
            PluginDescriptionFile desc = order.get(i).description();
            String label = jar.getName();
            logger.info("plugins: [" + (i + 1) + "/" + order.size() + "] " + label);
            try {
                long one = System.nanoTime();
                if (plugins.plugins.containsKey(desc.getName())) {
                    logger.warning("Duplicate plugin " + desc.getName() + " in " + jar.getName());
                    continue;
                }
                List<String> missing = PluginOrder.missing(desc, loadedNames);
                if (!missing.isEmpty()) {
                    // Зависимость была, но не загрузилась: без неё плагин упадёт
                    // на первом же её классе.
                    logger.severe("Could not load '" + jar.getName() + "': dependencies failed to load: "
                            + missing);
                    continue;
                }
                PluginLoader loader = new PluginLoader(this, jar, desc, via.parent(getClass().getClassLoader()),
                        classGroup);
                classGroup.add(loader);
                // Библиотеки из `plugin.yml` качаются из сети: на первом запуске
                // это десятки секунд, и без строки в журнале это не отличить от
                // зависания.
                int before = loader.declaredLibraryCount();
                if (before > 0) {
                    logger.info("plugins: " + desc.getName() + " declares " + before
                            + " library(ies), resolving...");
                }
                loader.addDeclaredLibraries(logger);
                // `paper-plugin.yml`: `loader:` добавляет свои библиотеки, `bootstrapper:`
                // регистрирует команды до создания плагина и создаёт его сам.
                loader.runPaperLoader(logger);
                loader.runBootstrapper();
                JavaPlugin plugin;
                try {
                    plugin = loader.create();
                } catch (Throwable t) {
                    classGroup.remove(loader);
                    throw t;
                }
                plugins.plugins.put(desc.getName(), plugin);
                loadedNames.add(desc.getName());
                loadedNames.addAll(desc.getProvides());
                plugin.onLoad();
                loaded.add(plugin);
                logger.info("plugins: " + desc.getName() + " loaded in " + ms(one) + " ms");
            } catch (Throwable t) {
                if (!Internals.report(logger, jar.getName(), "loading", t)) {
                    logger.log(Level.SEVERE, "Could not load " + jar.getName(), t);
                }
            }
        }
        int enabled = 0;
        for (JavaPlugin plugin : loaded) {
            long one = System.nanoTime();
            logger.info("plugins: enabling " + plugin.getName() + "...");
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
            // `onEnable` бывает долгим (BlueMap грузит ресурсы): время видно,
            // и по нему понятно, кого ждать при следующем запуске.
            logger.info("plugins: " + plugin.getName()
                    + (plugin.isEnabled() ? " enabled" : " failed") + " in " + ms(one) + " ms");
        }
        // `CraftServer.enablePlugins(POSTWORLD)`: `/help` — запасной, после плагинов.
        commands.setFallbackCommands();
        help.rebuild(commands.getCommands());
        publishCommands();
        logger.info("plugins: " + enabled + " of " + loaded.size() + " enabled in "
                + ms(started) + " ms");
        plugins.callEvent(new org.bukkit.event.server.ServerLoadEvent(loadType));
        return enabled;
    }

    /** Тип для {@code ServerLoadEvent}: запуск или `/reload`. */
    private org.bukkit.event.server.ServerLoadEvent.LoadType loadType =
            org.bukkit.event.server.ServerLoadEvent.LoadType.STARTUP;

    /**
     * Права {@code bukkit.command.*} и команды, которые Paper регистрирует сам
     * ({@code PaperCommands}): {@code /plugins}, {@code /version}, {@code /mspt}.
     * {@code /reload} и {@code /timings} добавляет конструктор {@code SimpleCommandMap}.
     */
    private void registerBuiltins() {
        if (!corePermissions) {
            corePermissions = true;
            Registries.bootstrap();
            org.bukkit.util.permissions.DefaultPermissions.registerCorePermissions();
            // Paper: `/mspt` только для операторов.
            if (plugins.getPermission("bukkit.command.mspt") == null) {
                plugins.addPermission(new org.bukkit.permissions.Permission("bukkit.command.mspt",
                        "Allows the user to view server tick times", org.bukkit.permissions.PermissionDefault.OP));
            }
        }
        for (Command c : PaperCommands.all(() -> Bridge.world("mspt"))) {
            commands.register("bukkit", c);
        }
    }

    /**
     * {@code Bukkit.reload}: выключить плагины, закрыть их загрузчики и загрузить
     * каталог заново. Via остаётся: она поднята библиотекой, а не плагином.
     */
    public void reload() {
        if (Lifecycle.registered) {
            throw new IllegalStateException(
                    "A lifecycle event handler has been registered which makes reloading plugins not possible");
        }
        List<org.bukkit.plugin.Plugin> own = new ArrayList<>();
        for (org.bukkit.plugin.Plugin p : plugins.plugins.values()) {
            if (p instanceof JavaPlugin) {
                own.add(p);
            }
        }
        java.util.Collections.reverse(own);
        for (org.bukkit.plugin.Plugin p : own) {
            plugins.disablePlugin(p);
        }
        for (org.bukkit.plugin.Plugin p : own) {
            plugins.plugins.remove(p.getName());
            if (p.getClass().getClassLoader() instanceof PluginLoader loader) {
                classGroup.remove(loader);
                try {
                    loader.close();
                } catch (java.io.IOException e) {
                    logger.log(Level.WARNING, "Could not close the class loader of " + p.getName(), e);
                }
            }
        }
        plugins.clearPermissions();
        corePermissions = false;
        commands.clearCommands();
        help.clear();
        loadType = org.bukkit.event.server.ServerLoadEvent.LoadType.RELOAD;
        loadAndEnable(pluginsDir);
    }

    /** `/reload permissions`: файла `permissions.yml` у нас нет — перечитывать нечего. */
    public void reloadPermissions() {}

    /** `/reload commands`: алиасов из `commands.yml` у нас нет. */
    public boolean reloadCommandAliases() { return true; }

    /** Датапаки при `/reload` не перезагружаются. */
    public void reloadData() {
        logger.info("Data packs are not reloaded by Chunkwake; restart the server to apply changes");
    }

    private static long ms(long from) {
        return (System.nanoTime() - from) / 1_000_000;
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

    /**
     * Метки команд для дерева `commands` у клиента: строки `метка\tdefault`
     * через '\n'. Метки — все ключи {@code CommandMap}, с `bukkit:pl` и т. п., как
     * у Paper; `default` — кому команда видна (см. {@link #access}).
     */
    private void publishCommands() {
        StringBuilder sb = new StringBuilder();
        for (var e : commands.getKnownCommands().entrySet()) {
            if (!sb.isEmpty()) sb.append('\n');
            sb.append(e.getKey()).append('\t').append(access(e.getValue()));
        }
        Bridge.setCommands(sb.toString());
    }

    /**
     * Кому видна команда без выдачи прав плагином: по default её прав, как
     * {@code testPermissionSilent} у {@code PermissibleBase}. Незарегистрированное
     * право — {@code Permission.DEFAULT_PERMISSION} (операторы). Права через `;` —
     * хватает любого.
     */
    String access(Command c) {
        String perm = c.getPermission();
        if (perm == null || perm.isEmpty()) {
            return "true";
        }
        boolean ops = false;
        boolean notOps = false;
        for (String name : perm.split(";")) {
            org.bukkit.permissions.Permission p = plugins.getPermission(name);
            switch (p == null ? org.bukkit.permissions.Permission.DEFAULT_PERMISSION : p.getDefault()) {
                case TRUE -> {
                    return "true";
                }
                case OP -> ops = true;
                case NOT_OP -> notOps = true;
                case FALSE -> {}
            }
        }
        return ops && notOps ? "true" : ops ? "op" : notOps ? "notop" : "false";
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
        // `CraftPlayer.sendSupportedChannels`: клиент узнаёт входящие каналы плагинов.
        p.sendSupportedChannels(messenger.getIncomingChannels());
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
        if (kind == Bridge.EVENT_REDSTONE) {
            // Без игрока, пачкой через `;`: x y z старая новая → новая сила по
            // каждому (пусто — плагин не менял).
            String[] items = arg.split(";", -1);
            StringBuilder out = new StringBuilder();
            for (int i = 0; i < items.length; i++) {
                String[] f = items[i].split(" ");
                int now = Integer.parseInt(f[4]);
                var e = new org.bukkit.event.block.BlockRedstoneEvent(
                        world.getBlockAt(Integer.parseInt(f[0]), Integer.parseInt(f[1]), Integer.parseInt(f[2])),
                        Integer.parseInt(f[3]), now);
                api.getPluginManager().callEvent(e);
                if (i > 0) {
                    out.append(';');
                }
                if (e.getNewCurrent() != now) {
                    out.append(e.getNewCurrent());
                }
            }
            return out.toString();
        }
        if (kind >= Bridge.EVENT_MOVE_ITEM && kind <= Bridge.EVENT_DISPENSE
                || kind == Bridge.EVENT_CAMPFIRE_START || kind == Bridge.EVENT_COOK) {
            return BlockEvents.fire(api.getPluginManager(), world, kind, arg);
        }
        if (kind == Bridge.EVENT_MOB_DEATH || kind == Bridge.EVENT_CREATURE_SPAWN || kind == Bridge.EVENT_BREED) {
            return MobImpl.fire(api.getPluginManager(), kind, arg);
        }
        if (kind == Bridge.EVENT_PISTON) {
            String[] f = arg.split(" ");
            var piston = world.getBlockAt(Integer.parseInt(f[0]), Integer.parseInt(f[1]), Integer.parseInt(f[2]));
            var face = org.bukkit.block.BlockFace.valueOf(f[3].toUpperCase(java.util.Locale.ROOT));
            List<org.bukkit.block.Block> moved = new java.util.ArrayList<>();
            if (f.length > 5) {
                for (String b : f[5].split(";")) {
                    String[] c = b.split(",");
                    moved.add(world.getBlockAt(Integer.parseInt(c[0]), Integer.parseInt(c[1]), Integer.parseInt(c[2])));
                }
            }
            org.bukkit.event.block.BlockPistonEvent e = f[4].equals("1")
                    ? new org.bukkit.event.block.BlockPistonExtendEvent(piston, moved, face)
                    : new org.bukkit.event.block.BlockPistonRetractEvent(piston, moved, face);
            api.getPluginManager().callEvent(e);
            return e.isCancelled() ? "c" : "";
        }
        PlayerImpl p = online.get(id);
        if (p == null) {
            return "";
        }
        if (kind == Bridge.EVENT_TAB_COMPLETE) {
            List<String> out = commands.tabComplete(p.api, arg);
            return out == null ? "" : String.join("\n", out);
        }
        if (kind == Bridge.EVENT_MOB_DAMAGE || kind == Bridge.EVENT_INTERACT_ENTITY || kind == Bridge.EVENT_SHEAR) {
            return MobImpl.fire(api.getPluginManager(), p, kind, arg);
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

    public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder h, int size) {
        return PluginInventory.create(h, org.bukkit.event.inventory.InventoryType.CHEST, size, null);
    }
    public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder h, int size, String title) {
        return PluginInventory.create(h, org.bukkit.event.inventory.InventoryType.CHEST, size, legacy(title));
    }
    public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder h, int size, Component title) {
        return PluginInventory.create(h, org.bukkit.event.inventory.InventoryType.CHEST, size, title);
    }
    public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder h, org.bukkit.event.inventory.InventoryType t) {
        return PluginInventory.create(h, t, t.getDefaultSize(), null);
    }
    public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder h, org.bukkit.event.inventory.InventoryType t, String title) {
        return PluginInventory.create(h, t, t.getDefaultSize(), legacy(title));
    }
    public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder h, org.bukkit.event.inventory.InventoryType t, Component title) {
        return PluginInventory.create(h, t, t.getDefaultSize(), title);
    }

    public org.bukkit.block.data.BlockData createBlockData(org.bukkit.Material m) {
        return BlockDataImpl.parse(Bridge.world("defaultdata " + m.getKey()));
    }

    public org.bukkit.block.data.BlockData createBlockData(String data) {
        String name = data.contains("[") ? data.substring(0, data.indexOf('[')) : data;
        if (!name.contains(":")) {
            name = "minecraft:" + name;
        }
        org.bukkit.block.data.BlockData base = BlockDataImpl.parse(Bridge.world("defaultdata " + name));
        return data.contains("[") ? base.merge(BlockDataImpl.parse(name + data.substring(data.indexOf('[')))) : base;
    }

    public org.bukkit.block.data.BlockData createBlockData(org.bukkit.Material m, String data) {
        return createBlockData(m.getKey() + (data == null ? "" : data));
    }
    /** Сообщение об отказе в правах — как `messages.no-permission` Paper по умолчанию. */
    public Component permissionMessage() {
        return Component.text("I'm sorry, but you do not have permission to perform this command. "
                + "Please contact the server administrators if you believe that this is in error.", NamedTextColor.RED);
    }
    public String getVersion() { return VERSION + "-chunkwake (MC: " + VERSION + ")"; }
    public String getBukkitVersion() { return VERSION + "-R0.1-SNAPSHOT"; }
    public String getMinecraftVersion() { return VERSION; }
    /** TPS за 1, 5 и 15 минут: у Rust одно окно, оно и отдаётся трижды. */
    public double[] getTPS() {
        double tps = tickNumber("TPS:", 20.0);
        return new double[] {tps, tps, tps};
    }
    public double getAverageTickTime() { return tickNumber("MSPT:", 0.0); }
    public org.bukkit.help.HelpMap getHelpMap() { return help; }

    /** Число после метки в строке Rust `TPS: … MSPT: … interval: …`. */
    private static double tickNumber(String label, double fallback) {
        String[] f = Bridge.world("tps").trim().split(" ");
        for (int i = 0; i + 1 < f.length; i++) {
            if (f[i].equals(label)) {
                try {
                    return Double.parseDouble(f[i + 1]);
                } catch (NumberFormatException e) {
                    return fallback;
                }
            }
        }
        return fallback;
    }
    public Logger getLogger() { return logger; }
    public PluginManager getPluginManager() { return plugins.api; }
    public BukkitScheduler getScheduler() { return scheduler.api; }
    public CommandMap getCommandMap() { return commands; }
    public ConsoleCommandSender getConsoleSender() { return console; }
    public boolean isPrimaryThread() { return Thread.currentThread() == mainThread; }
    public io.papermc.paper.threadedregions.scheduler.GlobalRegionScheduler getGlobalRegionScheduler() { return folia.global; }
    public io.papermc.paper.threadedregions.scheduler.RegionScheduler getRegionScheduler() { return folia.region; }
    public io.papermc.paper.threadedregions.scheduler.AsyncScheduler getAsyncScheduler() { return folia.async; }
    // Регион один: им владеет главный поток плагинов, как тиковый поток Paper.
    public boolean isGlobalTickThread() { return isPrimaryThread(); }
    public boolean isOwnedByCurrentRegion(World w, io.papermc.paper.math.Position p) { return isPrimaryThread(); }
    public boolean isOwnedByCurrentRegion(World w, io.papermc.paper.math.Position p, int r) { return isPrimaryThread(); }
    public boolean isOwnedByCurrentRegion(org.bukkit.Location l) { return isPrimaryThread(); }
    public boolean isOwnedByCurrentRegion(org.bukkit.Location l, int r) { return isPrimaryThread(); }
    public boolean isOwnedByCurrentRegion(World w, int cx, int cz) { return isPrimaryThread(); }
    public boolean isOwnedByCurrentRegion(World w, int cx, int cz, int r) { return isPrimaryThread(); }
    public boolean isOwnedByCurrentRegion(World w, int x0, int z0, int x1, int z1) { return isPrimaryThread(); }
    public boolean isOwnedByCurrentRegion(org.bukkit.entity.Entity e) { return isPrimaryThread(); }
    public int getMaxPlayers() { return intProperty("max-players", 20); }
    public boolean getOnlineMode() { return property("online-mode", "true").equals("true"); }
    public int getPort() { return intProperty("server-port", 25565); }
    public String getIp() { return property("server-ip", ""); }
    public String getMotd() { return property("motd", "A Minecraft Server"); }
    public int getViewDistance() { return intProperty("view-distance", 10); }
    public int getSimulationDistance() { return intProperty("simulation-distance", 10); }
    public boolean getAllowNether() { return property("allow-nether", "true").equals("true"); }
    public boolean getAllowFlight() { return property("allow-flight", "false").equals("true"); }
    public boolean isHardcore() { return property("hardcore", "false").equals("true"); }
    public boolean hasWhitelist() { return property("white-list", "false").equals("true"); }

    /** `server.properties` рабочего каталога (JVM живёт в процессе сервера). */
    private java.util.Properties properties;

    private synchronized String property(String key, String fallback) {
        if (properties == null) {
            properties = new java.util.Properties();
            try (var in = java.nio.file.Files.newBufferedReader(java.nio.file.Path.of("server.properties"))) {
                properties.load(in);
            } catch (java.io.IOException | RuntimeException e) {
                logger.fine("server.properties: " + e);
            }
        }
        return properties.getProperty(key, fallback).trim();
    }

    private int intProperty(String key, int fallback) {
        try {
            return Integer.parseInt(property(key, String.valueOf(fallback)));
        } catch (NumberFormatException e) {
            return fallback;
        }
    }
    public List<World> getWorlds() { return List.of(world); }

    /**
     * Каталог мира (в 26.3 `Server.getWorldContainer()` устарел в пользу этого):
     * BlueMap читает отсюда `level.dat` и `region/`.
     */
    public java.nio.file.Path getLevelDirectory() {
        // BlueMap берёт этот путь как КАТАЛОГ МИРА и прибавляет к нему
        // `region/` или `dimensions/minecraft/<измерение>`. Поэтому здесь
        // каталог мира, а не родительский каталог сервера.
        Bridge.log(3, "api: getLevelDirectory -> " + levelDirectory);
        return levelDirectory;
    }

    public File getWorldContainer() { return levelDirectory.getParent().toFile(); }

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

    /**
     * Событие {@code LifecycleEvents.COMMANDS}: плагин получает регистратор и
     * складывает туда свои команды.
     *
     * <p>Paper зовёт обработчик в фазе загрузки плагинов; у нас этой фазы нет,
     * поэтому событие наступает в момент подписки — плагин получает регистратор
     * сразу, а команды попадают в игру тем же порядком, что и из {@code plugin.yml}.
     */
    void runLifecycleCommands(String owner, Object configuration, LifecycleEventHandler<?> handler) {
        LifecycleCommands registrar = new LifecycleCommands(this);
        PluginMeta meta = plugins.plugins.get(owner) != null
                ? plugins.plugins.get(owner).getPluginMeta()
                : null;
        registrar.ownedBy(meta);
        var event = new CommandsEvent(registrar);
        try {
            callHandler(handler, event);
            Bridge.log(3, "lifecycle[" + owner + "]: " + registrar.registered()
                    + " command(s) registered");
        } catch (Throwable t) {
            if (!Internals.report(logger, owner, "command registration", t)) {
                logger.log(Level.SEVERE, "lifecycle[" + owner + "]: command registration failed", t);
            }
        }
    }

    /** {@code DATAPACK_DISCOVERY}: теги паков плагина становятся видны {@code getTag}. */
    void runLifecycleDatapacks(String owner, LifecycleEventHandler<?> handler) {
        try {
            callHandler(handler, new Tags.DiscoveryEvent(new Tags.Registrar(owner)));
        } catch (Throwable t) {
            logger.log(Level.SEVERE, "lifecycle[" + owner + "]: datapack discovery failed", t);
        }
    }

    /** Теги блоков и предметов: ванильные из Rust плюс теги датапаков плагинов. */
    public <T extends org.bukkit.Keyed> org.bukkit.Tag<T> getTag(String registry, org.bukkit.NamespacedKey tag,
            Class<T> clazz) {
        return Tags.get(registry, tag, clazz);
    }

    /** Обработчик типизирован по событию, поэтому зовём через приведение. */
    @SuppressWarnings("unchecked")
    private static void callHandler(LifecycleEventHandler<?> handler, LifecycleEvent event) {
        ((LifecycleEventHandler<LifecycleEvent>) handler).run(event);
    }

    /** Событие с регистратором команд: единственное, что мы диспетчеризуем. */
    private static final class CommandsEvent implements ReloadableRegistrarEvent<Commands> {
        private final LifecycleCommands registrar;

        CommandsEvent(LifecycleCommands registrar) {
            this.registrar = registrar;
        }

        @Override
        public Commands registrar() {
            return registrar;
        }

        @Override
        public Cause cause() {
            return Cause.INITIAL;
        }
    }

    /**
     * Команда из lifecycle-регистратора: {@link LifecycleCommands} уже создал
     * brigadier-узел, здесь он становится командой нашего {@code CommandMap}.
     */
    void registerLifecycleCommand(String fallbackPrefix, String name,
            com.mojang.brigadier.tree.LiteralCommandNode<io.papermc.paper.command.brigadier.CommandSourceStack> node,
            PluginMeta owner) {
        String usage = node.getUsageText().isEmpty() ? "/" + name : node.getUsageText();
        var command = new LifecycleCommands.BrigadierCommand(name, "Registered by " + fallbackPrefix, usage, node,
                owner != null ? owner.getName() : fallbackPrefix);
        if (commands.register(fallbackPrefix, command)) {
            Bridge.log(3, "lifecycle: /" + name + " from " + fallbackPrefix);
        }
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
        public String getName() { return Bridge.server.levelDirectory.getFileName().toString(); }

        /** `minecraft:overworld`: по ключу BlueMap выбирает измерение. */
        public org.bukkit.NamespacedKey getKey() {
            return new org.bukkit.NamespacedKey("minecraft", "overworld");
        }

        public org.bukkit.World.Environment getEnvironment() {
            return org.bukkit.World.Environment.NORMAL;
        }

        public long getSeed() {
            String seed = Bridge.world("seed").trim();
            return seed.isEmpty() ? 0L : Long.parseLong(seed);
        }

        /** Каталог мира: плагины читают из него регионы и `level.dat`. */
        public java.nio.file.Path getWorldPath() {
            Bridge.log(3, "api: World.getWorldPath -> " + Bridge.server.levelDirectory);
            return Bridge.server.levelDirectory;
        }

        public int getSeaLevel() { return 63; }

        // --- Геймрулы: те же правила, что у `/gamerule` (общие на сервер) ---

        public <T> T getGameRuleValue(org.bukkit.GameRule<T> rule) {
            return rule instanceof RegistryValues.GameRuleImpl<?> r ? rule.getType().cast(
                    RegistryValues.GameRules.value(r)) : null;
        }

        public <T> T getGameRuleDefault(org.bukkit.GameRule<T> rule) {
            return rule == null ? null : rule.getDefaultValue();
        }

        public <T> boolean setGameRule(org.bukkit.GameRule<T> rule, T value) {
            return rule instanceof RegistryValues.GameRuleImpl<?> r && value != null
                    && RegistryValues.GameRules.set(r, value);
        }

        /** Имена как у {@code GameRule.getName()}: {@code minecraft:keep_inventory}. */
        public String[] getGameRules() {
            return RegistryValues.GameRules.names().stream().map(n -> "minecraft:" + n).toArray(String[]::new);
        }

        public boolean isGameRule(String name) {
            return rule(name) != null;
        }

        public String getGameRuleValue(String name) {
            org.bukkit.GameRule<?> r = rule(name);
            Object v = r == null ? null : getGameRuleValue(r);
            return v == null ? null : v.toString();
        }

        @SuppressWarnings("unchecked")
        public boolean setGameRuleValue(String name, String value) {
            org.bukkit.GameRule<Object> r = (org.bukkit.GameRule<Object>) rule(name);
            if (r == null || value == null) {
                return false;
            }
            Object parsed;
            if ((Class<?>) r.getType() == Integer.class) {
                try {
                    parsed = Integer.parseInt(value);
                } catch (NumberFormatException e) {
                    return false;
                }
            } else if (value.equals("true") || value.equals("false")) {
                parsed = Boolean.parseBoolean(value);
            } else {
                return false;
            }
            return setGameRule(r, parsed);
        }

        private static org.bukkit.GameRule<?> rule(String name) {
            org.bukkit.NamespacedKey k = name == null ? null : org.bukkit.NamespacedKey.fromString(name);
            return k == null ? null : org.bukkit.Registry.GAME_RULE.get(k);
        }

        public org.bukkit.Location getSpawnLocation() {
            return new org.bukkit.Location(Bridge.server.world, 0, 64, 0);
        }
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

        /**
         * `World.save()`: просит сервер записать мир на диск.
         *
         * BlueMap зовёт его при включении, чтобы свойства мира точно были в
         * файле. Запись идёт в блокирующем пуле сервера (как автосейв), здесь
         * только просьба: иначе плагин ждал бы диск на своём потоке.
         */
        public void save() {
            Bridge.world("save");
        }

        public void save(boolean force) {
            save();
        }

        /** Автосохранение у нас всегда включено: раз в 30 с и по просьбе. */
        public boolean isAutoSave() { return true; }

        public void setAutoSave(boolean value) {
            // Выключить автосейв плагину нельзя: мир потерял бы данные при
            // аварии. Значение принимается молча, как в ваниле без правок.
        }

        public org.bukkit.entity.Entity spawnEntity(org.bukkit.Location l, org.bukkit.entity.EntityType type) {
            String s = MobImpl.species(type);
            if (s == null) {
                throw new UnsupportedOperationException("chunkwake: spawnEntity " + type);
            }
            return MobImpl.spawn(l, s);
        }

        public org.bukkit.entity.Entity spawnEntity(org.bukkit.Location l, org.bukkit.entity.EntityType type,
                boolean randomize) {
            return spawnEntity(l, type);
        }

        public org.bukkit.entity.Entity spawnEntity(org.bukkit.Location l, org.bukkit.entity.EntityType type,
                org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason reason) {
            return spawnEntity(l, type);
        }

        public <T extends org.bukkit.entity.Entity> T spawn(org.bukkit.Location l, Class<T> type) {
            return spawn(l, type, null);
        }

        public <T extends org.bukkit.entity.Entity> T spawn(org.bukkit.Location l, Class<T> type,
                org.bukkit.util.Consumer<? super T> init) {
            java.util.function.Consumer<? super T> c = init == null ? null : init::accept;
            return spawn(l, type, c);
        }

        public <T extends org.bukkit.entity.Entity> T spawn(org.bukkit.Location l, Class<T> type,
                java.util.function.Consumer<? super T> init) {
            String s = MobImpl.species(type);
            if (s == null) {
                throw new UnsupportedOperationException("chunkwake: spawn " + type.getSimpleName());
            }
            T e = type.cast(MobImpl.spawn(l, s));
            if (init != null) {
                init.accept(e);
            }
            return e;
        }

        public java.util.List<org.bukkit.entity.Entity> getEntities() {
            java.util.List<org.bukkit.entity.Entity> out = new java.util.ArrayList<>(Bridge.server.api.getOnlinePlayers());
            out.addAll(MobImpl.all());
            return out;
        }

        public java.util.List<org.bukkit.entity.LivingEntity> getLivingEntities() {
            java.util.List<org.bukkit.entity.LivingEntity> out = new java.util.ArrayList<>(Bridge.server.api.getOnlinePlayers());
            out.addAll(MobImpl.all());
            return out;
        }

        public <T extends org.bukkit.entity.Entity> java.util.Collection<T> getEntitiesByClass(Class<T> type) {
            java.util.List<T> out = new java.util.ArrayList<>();
            for (var e : getEntities()) {
                if (type.isInstance(e)) {
                    out.add(type.cast(e));
                }
            }
            return out;
        }

        public java.util.Collection<org.bukkit.entity.Entity> getNearbyEntities(org.bukkit.Location l, double x,
                double y, double z) {
            java.util.List<org.bukkit.entity.Entity> out = new java.util.ArrayList<>();
            for (var e : getEntities()) {
                var at = e.getLocation();
                if (Math.abs(at.getX() - l.getX()) <= x && Math.abs(at.getY() - l.getY()) <= y
                        && Math.abs(at.getZ() - l.getZ()) <= z) {
                    out.add(e);
                }
            }
            return out;
        }

        /** Предмет на земле без разброса. */
        public org.bukkit.entity.Item dropItem(org.bukkit.Location l, org.bukkit.inventory.ItemStack s) {
            return drop(l, s, false);
        }

        /** Предмет с разбросом, как из сломанного блока. */
        public org.bukkit.entity.Item dropItemNaturally(org.bukkit.Location l, org.bukkit.inventory.ItemStack s) {
            return drop(l, s, true);
        }

        private org.bukkit.entity.Item drop(org.bukkit.Location l, org.bukkit.inventory.ItemStack s, boolean natural) {
            String line = StackImpl.line(s);
            if (line == null) {
                throw new IllegalArgumentException("Cannot drop air");
            }
            Bridge.world("dropitem " + l.getX() + " " + l.getY() + " " + l.getZ() + " " + (natural ? 1 : 0) + " " + line);
            return Stub.of(org.bukkit.entity.Item.class, new BlockEvents.GroundItem(l.clone(), s.clone()));
        }
    }
}
