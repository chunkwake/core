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
    private final io.papermc.paper.threadedregions.scheduler.EntityScheduler entityScheduler =
            EntitySchedulers.of(scheduler);
    private final PluginManagerImpl plugins = new PluginManagerImpl(this);
    private final SimpleCommandMap commands = new SimpleCommandMap(api, new LinkedHashMap<>());
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

    /** Планировщик задач сущностей: один на сервер, как поток Folia. */
    io.papermc.paper.threadedregions.scheduler.EntityScheduler entityScheduler() {
        return entityScheduler;
    }

    void disablePlugins() {
        plugins.disablePlugins();
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
        // Загрузка плагинов останавливает сервер на всё время `onEnable`: тик
        // ещё не пошёл, игроки ждут. Без сообщений это выглядело как зависание,
        // поэтому каждая фаза объявляется заранее — видно, на чём именно стоит.
        logger.info("plugins: " + jars.length + " jar(s) in " + dir + ", loading...");
        long started = System.nanoTime();
        List<JavaPlugin> loaded = new ArrayList<>();
        for (int i = 0; i < jars.length; i++) {
            File jar = jars[i];
            String label = jar.getName();
            logger.info("plugins: [" + (i + 1) + "/" + jars.length + "] " + label);
            try {
                long one = System.nanoTime();
                PluginDescriptionFile desc = PluginLoader.describe(jar);
                if (plugins.plugins.containsKey(desc.getName())) {
                    logger.warning("Duplicate plugin " + desc.getName() + " in " + jar.getName());
                    continue;
                }
                PluginLoader loader = new PluginLoader(this, jar, desc, getClass().getClassLoader());
                // Библиотеки из `plugin.yml` качаются из сети: на первом запуске
                // это десятки секунд, и без строки в журнале это не отличить от
                // зависания.
                int before = loader.declaredLibraryCount();
                if (before > 0) {
                    logger.info("plugins: " + desc.getName() + " declares " + before
                            + " library(ies), resolving...");
                }
                loader.addDeclaredLibraries(logger);
                JavaPlugin plugin = loader.create();
                plugins.plugins.put(desc.getName(), plugin);
                plugin.onLoad();
                loaded.add(plugin);
                logger.info("plugins: " + desc.getName() + " loaded in " + ms(one) + " ms");
            } catch (Throwable t) {
                logger.log(Level.SEVERE, "Could not load " + jar.getName(), t);
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
        publishCommands();
        logger.info("plugins: " + enabled + " of " + loaded.size() + " enabled in "
                + ms(started) + " ms");
        return enabled;
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
    public Logger getLogger() { return logger; }
    public PluginManager getPluginManager() { return plugins.api; }
    public BukkitScheduler getScheduler() { return scheduler.api; }
    public CommandMap getCommandMap() { return commands; }
    public ConsoleCommandSender getConsoleSender() { return console; }
    public boolean isPrimaryThread() { return Thread.currentThread() == mainThread; }
    public int getMaxPlayers() { return 20; }
    public List<World> getWorlds() { return List.of(world); }

    /**
     * Каталог мира (в 26.3 `Server.getWorldContainer()` устарел в пользу этого):
     * BlueMap читает отсюда `level.dat` и `region/`.
     */
    public java.nio.file.Path getLevelDirectory() {
        // BlueMap берёт этот путь как КАТАЛОГ МИРА и прибавляет к нему
        // `region/` или `dimensions/minecraft/<измерение>`. Поэтому здесь
        // каталог мира, а не родительский каталог сервера.
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
            logger.log(Level.SEVERE, "lifecycle[" + owner + "]: command registration failed", t);
        }
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
        var command = new LifecycleCommands.BrigadierCommand(name, "Registered by " + fallbackPrefix, usage, node);
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
        public java.nio.file.Path getWorldPath() { return Bridge.server.levelDirectory; }

        public int getSeaLevel() { return 63; }

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
