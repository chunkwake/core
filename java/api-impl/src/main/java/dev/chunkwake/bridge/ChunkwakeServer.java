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
import org.bukkit.plugin.SimplePluginManager;
import org.bukkit.plugin.java.JavaPlugin;
import org.bukkit.scheduler.BukkitScheduler;

/** {@link Server} поверх Rust-ядра. Всё вызывается в потоке плагинов. */
final class ChunkwakeServer {
    static final String VERSION = "26.3";

    final Server api = new org.bukkit.craftbukkit.CraftServer(this);
    final Logger logger = Logger.getLogger("Chunkwake");
    private final Scheduler scheduler = new Scheduler();
    private final FoliaSchedulers folia = new FoliaSchedulers(scheduler);
    final PluginManagerImpl plugins = new PluginManagerImpl(this);
    private final SimpleCommandMap commands = new SimpleCommandMap(api, new LinkedHashMap<>());
    private final SpigotImpl spigot = new SpigotImpl();
    /**
     * Настоящий {@link SimplePluginManager}: Paper-овский фасад, который
     * делегирует всё в {@link #plugins}, но хранит поля прав
     * (`permissions`, `defaultPerms`, `permSubs`, `defSubs`), которые
     * подменяет LuckPerms (`PermissibleInjector`/`InjectorPermissionMap`).
     */
    final SimplePluginManager simple = new SimplePluginManager(api, commands);
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
    /** Оверворлд: события без мира (тик блоков и мобов) идут сюда. */
    final WorldImpl overworld = new WorldImpl(this, WorldImpl.OVERWORLD);
    final World world = overworld.api;
    /** Миры по ключу измерения, порядок как у Paper: оверворлд, Ад, Край. */
    private volatile Map<String, WorldImpl> worlds;
    private final Metadata metadata = new Metadata();
    private final Thread mainThread = Thread.currentThread();
    /** Каталог мира: его читают плагины (BlueMap разбирает регионы сам). */
    private final java.nio.file.Path levelDirectory;

    ChunkwakeServer(java.nio.file.Path levelDirectory) {
        this.levelDirectory = levelDirectory;
        // Paper: SimplePluginManager — фасад, реальная работа в paperPluginManager.
        this.simple.paperPluginManager = this.plugins.api;
        // Как CraftServer: создание сервера делает его доступным через
        // `Bukkit.getServer()`. Важно для `PermissibleBase` игрока (он сразу
        // читает default-права через сервер) и для тестов.
        if (Bukkit.getServer() == null) {
            Bukkit.setServer(api);
        }
        // `CraftServer.commandMap`: Lamp/revxrsal (FancyHolograms и др.)
        // читает это поле рефлексией из `Bukkit.getServer().getClass()`.
        if (api instanceof org.bukkit.craftbukkit.CraftServer craft) {
            craft.bindCommandMap(this.commands);
        }
    }

    java.nio.file.Path levelDirectory() {
        return levelDirectory;
    }

    Metadata metadata() {
        return metadata;
    }

    /** Миры сервера: оверворлд и загруженные Rust измерения (без Rust — только оверворлд). */
    Map<String, WorldImpl> worlds() {
        Map<String, WorldImpl> w = worlds;
        if (w != null) {
            return w;
        }
        synchronized (this) {
            if (worlds == null) {
                Map<String, WorldImpl> out = new LinkedHashMap<>();
                out.put(WorldImpl.OVERWORLD, overworld);
                if (Bridge.table != null) {
                    for (String key : Bridge.world("worlds").split("\n")) {
                        if (!key.isBlank() && !out.containsKey(key.trim())) {
                            out.put(key.trim(), new WorldImpl(this, key.trim()));
                        }
                    }
                }
                worlds = java.util.Collections.unmodifiableMap(out);
            }
            return worlds;
        }
    }

    /** Мир по ключу измерения; неизвестный — оверворлд. */
    WorldImpl world(String key) {
        WorldImpl w = key == null ? null : worlds().get(key);
        return w != null ? w : overworld;
    }

    /**
     * { MetadataStore} сервера, как у { CraftServer}: игроки по имени
     * (как { PlayerMetadataStore}), сущности и миры по UUID, блоки по месту.
     * Выключение плагина снимает его значения ({ invalidateAll}).
     */
    static final class Metadata {
        final org.bukkit.metadata.MetadataStoreBase<org.bukkit.OfflinePlayer> players =
                new org.bukkit.metadata.MetadataStoreBase<>() {
                    @Override
                    protected String disambiguate(org.bukkit.OfflinePlayer p, String key) {
                        return p.getName().toLowerCase(java.util.Locale.ROOT) + ":" + key;
                    }
                };
        final org.bukkit.metadata.MetadataStoreBase<org.bukkit.entity.Entity> entities =
                new org.bukkit.metadata.MetadataStoreBase<>() {
                    @Override
                    protected String disambiguate(org.bukkit.entity.Entity e, String key) {
                        return e.getUniqueId() + ":" + key;
                    }
                };
        final org.bukkit.metadata.MetadataStoreBase<World> worlds =
                new org.bukkit.metadata.MetadataStoreBase<>() {
                    @Override
                    protected String disambiguate(World w, String key) {
                        return w.getUID() + ":" + key;
                    }
                };
        final org.bukkit.metadata.MetadataStoreBase<org.bukkit.block.Block> blocks =
                new org.bukkit.metadata.MetadataStoreBase<>() {
                    @Override
                    protected String disambiguate(org.bukkit.block.Block b, String key) {
                        return b.getWorld().getUID() + ":" + b.getX() + ":" + b.getY() + ":" + b.getZ() + ":" + key;
                    }
                };

        void removeAll(org.bukkit.plugin.Plugin plugin) {
            players.removeAll(plugin);
            entities.removeAll(plugin);
            worlds.removeAll(plugin);
            blocks.removeAll(plugin);
        }
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
    @SuppressWarnings("deprecation")
    public static final class Unsafe {
        static final int PROTOCOL = 777;
        /**
         * Paper {@code UnsafeValues.get(RegistryKey, NamespacedKey)}:
         * Attribute.valueOf и статические поля Attribute идут сюда.
         */
        @SuppressWarnings("unchecked")
        public <T extends org.bukkit.Keyed> T get(
                io.papermc.paper.registry.RegistryKey<T> registryKey,
                org.bukkit.NamespacedKey key) {
            if (registryKey == io.papermc.paper.registry.RegistryKey.ATTRIBUTE) {
                return (T) Attributes.byKey(key);
            }
            if (registryKey == io.papermc.paper.registry.RegistryKey.ENCHANTMENT) {
                return (T) Registries.enchantment(key);
            }
            return null;
        }

        public int getProtocolVersion() { return PROTOCOL; }
        static final int DATA_VERSION = 5023;
        public int getDataVersion() { return DATA_VERSION; }

        /** {@code UnsafeValues#getMainLevelName}: имя основного мира. */
        public String getMainLevelName() { return "world"; }

        /** `Material.asBlockType()` для legacy-констант: без этого `isBlock()` падал. */
        public org.bukkit.Material fromLegacy(org.bukkit.Material type) {
            if (type == null || !type.isLegacy()) {
                return type;
            }
            return LegacyMaterials.modernize(type, type.getId() < 256);
        }

        public org.bukkit.Material fromLegacy(org.bukkit.material.MaterialData data) {
            return fromLegacy(data, false);
        }

        public org.bukkit.Material fromLegacy(org.bukkit.material.MaterialData data,
                boolean legacyName) {
            if (data == null || data.getItemType() == null) {
                return null;
            }
            org.bukkit.Material type = data.getItemType();
            if (!type.isLegacy()) {
                return type;
            }
            if (legacyName) {
                return type;
            }
            return LegacyMaterials.modernize(data, type.getId() < 256);
        }

        public org.bukkit.Material toLegacy(org.bukkit.Material type) {
            return type;
        }

        public org.bukkit.Material getMaterial(String name, int data) {
            return org.bukkit.Material.matchMaterial(name);
        }

        /**
         * Paper API: {@code ItemStack(Material, int)} создаёт craftDelegate
         * через {@code ItemType.createItemStack -> Unsafe.createItemStack}.
         * Без этого у обычного {@code new ItemStack(...)} craftDelegate
         * остаётся null, и {@code ItemStack.getData(...)} падает (nightcore,
         * ExcellentCrates).
         */
        public org.bukkit.inventory.ItemStack createItemStack(
                org.bukkit.inventory.ItemType type, int amount) {
            org.bukkit.Material material = type == null ? null : type.asMaterial();
            return new StackImpl(material == null ? org.bukkit.Material.AIR : material,
                    Math.max(amount, 1));
        }

        public org.bukkit.inventory.ItemStack createItemStack(
                org.bukkit.inventory.ItemType type) {
            return createItemStack(type, 1);
        }
    }

    /** Загружает jar из каталога и включает плагины. Возвращает число включённых. */
    int loadAndEnable(File dir) {
        pluginsDir = dir;
        LegacyEnums.install();
        if (Bukkit.getServer() == null) {
            Bukkit.setServer(api);
        }
        try {
            net.minecraft.core.registries.BuiltInRegistries.populateVanillaItems();
        } catch (Throwable t) {
            logger.log(Level.WARNING, "populateVanillaItems failed: " + t);
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
                candidates.add(new PluginOrder.Candidate(jar, PluginLoader.describe(jar),
                        PluginLoader.paperDeps(jar)));
            } catch (Throwable t) {
                logger.log(Level.SEVERE, "Could not load " + jar.getName() + ": " + t.getMessage());
            }
        }
        // Как Paper: зависимости — раньше зависящих, `loadbefore` — наоборот.
        List<PluginOrder.Candidate> order = PluginOrder.sort(candidates, plugins.plugins.keySet(), logger);
        // Как на Paper: реестры ванили подняты до первого класса плагина.
        VanillaBootstrap.await();
        List<JavaPlugin> loaded = new ArrayList<>();
        // Two-phase loading, as Paper/CraftBukkit: first create and register
        // every plugin class loader in the shared group, then resolve
        // dependencies and create main classes in dependency order. Without
        // the first phase a plugin cannot see classes of a plugin that comes
        // later in the order (CustomDiscs implements a PacketEvents
        // interface, PacketEvents is loaded after it alphabetically).
        java.util.Set<String> loadedNames = new java.util.HashSet<>(plugins.plugins.keySet());
        List<PluginLoader> loaders = new ArrayList<>(order.size());
        int skipped = 0;
        List<PluginDescriptionFile> descriptions = new ArrayList<>(order.size());
        List<File> loaderJars = new ArrayList<>(order.size());
        for (int i = 0; i < order.size(); i++) {
            File jar = order.get(i).jar();
            PluginDescriptionFile desc = order.get(i).description();
            String label = jar.getName();
            logger.info("plugins: [" + (i + 1) + "/" + order.size() + "] " + label);
            try {
                if (plugins.plugins.containsKey(desc.getName())) {
                    logger.warning("Duplicate plugin " + desc.getName() + " in " + jar.getName());
                    continue;
                }
                PluginLoader loader = new PluginLoader(this, jar, desc,
                        via.parent(getClass().getClassLoader()), classGroup);
                classGroup.add(loader);
                loaders.add(loader);
                descriptions.add(desc);
                loaderJars.add(jar);
                int before = loader.declaredLibraryCount();
                if (before > 0) {
                    logger.info("plugins: " + desc.getName() + " declares " + before
                            + " library(ies), resolving...");
                }
                loader.addDeclaredLibraries(logger);
            } catch (Throwable t) {
                if (!Internals.report(logger, jar.getName(), "loading", t)) {
                    logger.log(Level.SEVERE, "Could not load " + jar.getName(), t);
                    logCauseChain(jar.getName(), t);
                } else {
                    skipped++;
                }
            }
        }
        for (int k = 0; k < loaders.size(); k++) {
            PluginLoader loader = loaders.get(k);
            PluginDescriptionFile desc = descriptions.get(k);
            File jar = loaderJars.get(k);
            try {
                long one = System.nanoTime();
                List<String> missing = PluginOrder.missing(desc,
                        PluginLoader.paperDeps(jar), loadedNames);
                if (!missing.isEmpty()) {
                    logger.severe("Could not load '" + jar.getName()
                            + "': dependencies failed to load: " + missing);
                    classGroup.remove(loader);
                    continue;
                }
                // `paper-plugin.yml`: `loader:` runs before the main class,
                // `bootstrapper:` prepares state for onLoad.
                loader.runPaperLoader(logger);
                // Paper-семантика: во время bootstrap `Bukkit.getServer()`
                // ещё null (сервер создаётся позже). Плагины вроде CommandAPI
                // именно так различают bootstrap- и onLoad-регистрацию.
                java.lang.reflect.Field bukkitServer =
                        org.bukkit.Bukkit.class.getDeclaredField("server");
                bukkitServer.setAccessible(true);
                Object savedServer = bukkitServer.get(null);
                try {
                    bukkitServer.set(null, null);
                    loader.runBootstrapper();
                } finally {
                    bukkitServer.set(null, savedServer);
                }
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
                    logCauseChain(jar.getName(), t);
                } else {
                    skipped++;
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
            // ProtocolLib: привязать CHAT_COMMAND к нашему NMS-классу до onEnable.
            ProtocolBridge.prepare(plugin);
            // WorldEdit без NMS-адаптера: подписаться на ConfigurationLoadEvent,
            // чтобы поставить API-адаптер до первого PlatformReadyEvent.
            WorldEditSupport.prepare(plugin);
            plugins.enablePlugin(plugin);
            // Запасной путь, если слушатель не сработал (другая версия WorldEdit).
            WorldEditSupport.install(plugin);
            if (plugin.isEnabled()) {
                enabled++;
            }
            // `onEnable` бывает долгим (BlueMap грузит ресурсы): время видно,
            // и по нему понятно, кого ждать при следующем запуске.
            logger.info("plugins: " + plugin.getName()
                    + (plugin.isEnabled() ? " enabled" : " failed") + " in " + ms(one) + " ms");
        }
        // `CraftServer.enablePlugins(POSTWORLD)`: `/help` — запасной, после плагинов.
        // Paper: LifecycleEvents.COMMANDS выполняются после onEnable всех
        // плагинов (Incendo Cloud ожидает именно этот порядок).
        flushLifecycleCommands();
        commands.setFallbackCommands();
        help.rebuild(commands.getCommands());
        publishCommands();
        logger.info("plugins: " + enabled + " of " + loaded.size() + " enabled"
                + (skipped > 0 ? ", " + skipped + " skipped (missing server internals)" : "")
                + " in " + ms(started) + " ms");
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
        registerVanillaCommandNames();
    }

    /**
     * Paper exposes the whole vanilla command tree through
     * {@code Server.getCommandMap()}. Chunkwake executes the tree in Rust, so
     * register API-visible stubs for names that are not already provided by a
     * plugin. Dispatch remains handled by the Rust command layer.
     */
    private void registerVanillaCommandNames() {
        try (var in = ChunkwakeServer.class.getResourceAsStream("/vanilla-command-names.txt")) {
            if (in == null) {
                return;
            }
            for (String line : new String(in.readAllBytes(), java.nio.charset.StandardCharsets.US_ASCII)
                    .split("\\R")) {
                String name = line.trim();
                if (name.isEmpty()
                        || commands.getCommand(name) != null
                        || commands.getCommand("minecraft:" + name) != null) {
                    continue;
                }
                commands.register("minecraft", vanillaCommand(name));
            }
        } catch (java.io.IOException ignored) {
            Bridge.log(2, "vanilla command names unavailable");
        }
        // Paper/Bukkit aliases that are part of CraftCommandMap but not of the
        // Mojang command report.
        java.util.Map<String, org.bukkit.command.Command> known = commands.getKnownCommands();
        for (String name : java.util.List.of(
                "minecraft:version", "paper", "paper:mspt", "reload", "restart",
                "spark", "spigot", "timings", "tps")) {
            known.putIfAbsent(name, vanillaCommand(name));
        }
    }

    private static org.bukkit.command.Command vanillaCommand(String name) {
        return new org.bukkit.command.Command(name) {
            @Override
            public boolean execute(org.bukkit.command.CommandSender sender, String label,
                    String[] args) {
                return false;
            }

            @Override
            public java.util.List<String> tabComplete(org.bukkit.command.CommandSender sender,
                    String alias, String[] args) {
                return java.util.List.of();
            }
        };
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

    /** Игроки между {@code PlayerLoginEvent} и входом: тот же объект уходит в {@code join}. */
    final Map<UUID, PlayerImpl> logins = new java.util.concurrent.ConcurrentHashMap<>();

    /** Имена входов: нужны `getOfflinePlayer`/`OfflinePlayer.getName`. */
    private final Map<UUID, String> knownNames = new java.util.concurrent.ConcurrentHashMap<>();
    private final Map<UUID, Long> seen = new java.util.concurrent.ConcurrentHashMap<>();

    void join(UUID id, String name) {
        PlayerImpl pending = logins.remove(id);
        PlayerImpl p = pending != null ? pending : new PlayerImpl(this, id, name);
        p.online = true;
        knownNames.put(id, name);
        seen.put(id, System.currentTimeMillis());
        online.put(id, p);
        Component msg = Component.translatable("multiplayer.player.joined", NamedTextColor.YELLOW,
                Component.text(name));
        PlayerJoinEvent e = new PlayerJoinEvent(p.api, msg);
        plugins.callEvent(e);
        if (p.api instanceof org.bukkit.craftbukkit.entity.CraftPlayer craft) {
            craft.getHandle().connection.connection.restoreChannel();
        }
        if (e.joinMessage() != null) {
            announce(e.joinMessage());
        }
        // `CraftPlayer.sendSupportedChannels`: клиент узнаёт входящие каналы плагинов.
        p.sendSupportedChannels(messenger.getIncomingChannels());
    }

    void quit(UUID id) {
        logins.remove(id);
        PlayerImpl p = online.get(id);
        if (p == null) {
            return;
        }
        Component msg = Component.translatable("multiplayer.player.left", NamedTextColor.YELLOW,
                Component.text(p.name));
        // После `PlayerKickEvent` — его сообщение о выходе и причина KICKED, как у Paper.
        if (p.kicked) {
            msg = p.kickLeaveMessage;
        }
        PlayerQuitEvent e = new PlayerQuitEvent(p.api, msg,
                p.kicked ? PlayerQuitEvent.QuitReason.KICKED : PlayerQuitEvent.QuitReason.DISCONNECTED);
        plugins.callEvent(e);
        online.remove(id);
        knownNames.put(id, p.name);
        seen.put(id, System.currentTimeMillis());
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
        if (kind == Bridge.EVENT_CHUNK_LOAD) {
            // `<dimension> <x> <z> <isNew>`; событие вызовется на тике.
            String[] f = arg.split(" ");
            WorldImpl target = world(f.length > 0 ? f[0] : null);
            if (target == null) {
                target = overworld;
            }
            boolean isNew = f.length > 3 && !"0".equals(f[3]);
            ChunkEvents.queue(target, Integer.parseInt(f[1]), Integer.parseInt(f[2]), isNew);
            return "";
        }
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
        if (kind == Bridge.EVENT_MOB_DEATH || kind == Bridge.EVENT_CREATURE_SPAWN || kind == Bridge.EVENT_BREED
                || kind == Bridge.EVENT_SLIME_SPLIT) {
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
        if (kind == Bridge.EVENT_PRE_LOGIN) {
            return LoginEvents.preLogin(this, id, arg);
        }
        if (kind == Bridge.EVENT_LOGIN) {
            return LoginEvents.login(this, id, arg);
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
        // PacketType.Play.Client.CHAT_COMMAND: Rust уже принял пакет и позвал
        // Bukkit — отдаём его слушателям ProtocolLib (пока без Netty-канала).
        ProtocolBridge.command(sender, line);
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
    public int getCurrentTick() {
        try {
            return Integer.parseInt(Bridge.world("tick").trim());
        } catch (RuntimeException error) {
            return 0;
        }
    }
    public double getAverageTickTime() { return tickNumber("MSPT:", 0.0); }

    /** `Server.getTickTimes()`: наносекунды последних тиков (tps-hud). */
    public long[] getTickTimes() {
        double mspt = tickNumber("MSPT:", 0.0);
        long nanos = Math.max(1L, (long) (mspt * 1_000_000.0));
        return new long[] {nanos, nanos, nanos, nanos, nanos};
    }
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
    public org.bukkit.Server.Spigot spigot() { return spigot; }
    public Logger getLogger() { return logger; }
    public PluginManager getPluginManager() { return simple; }
    public org.bukkit.inventory.ItemFactory getItemFactory() { return ItemFactoryImpl.API; }
    public BukkitScheduler getScheduler() { return scheduler.api; }
    public CommandMap getCommandMap() { return commands; }
    public ConsoleCommandSender getConsoleSender() { return console; }
    public boolean isPrimaryThread() { return Thread.currentThread() == mainThread; }
    public io.papermc.paper.threadedregions.scheduler.GlobalRegionScheduler getGlobalRegionScheduler() { return folia.global; }
    public io.papermc.paper.threadedregions.scheduler.RegionScheduler getRegionScheduler() { return folia.region; }
    public io.papermc.paper.threadedregions.scheduler.AsyncScheduler getAsyncScheduler() { return folia.async; }
    // Регион один: им владеет главный поток плагинов, как тиковый поток Paper.
    public boolean isGlobalTickThread() { return isPrimaryThread(); }
    public boolean isOwnedByCurrentRegion(World w, io.papermc.paper.math.Position p) {
        return isPrimaryThread() || folia.ownsRegion(w, p.blockX(), p.blockZ());
    }
    public boolean isOwnedByCurrentRegion(World w, io.papermc.paper.math.Position p, int r) {
        return isPrimaryThread() || ownsBlockArea(w, p.blockX() - r, p.blockZ() - r, p.blockX() + r, p.blockZ() + r);
    }
    public boolean isOwnedByCurrentRegion(org.bukkit.Location l) {
        return l.getWorld() != null && (isPrimaryThread() || folia.ownsRegion(l.getWorld(), l.getBlockX(), l.getBlockZ()));
    }
    public boolean isOwnedByCurrentRegion(org.bukkit.Location l, int r) {
        return l.getWorld() != null
                && (isPrimaryThread() || ownsBlockArea(l.getWorld(), l.getBlockX() - r, l.getBlockZ() - r,
                        l.getBlockX() + r, l.getBlockZ() + r));
    }
    public boolean isOwnedByCurrentRegion(World w, int cx, int cz) {
        return isPrimaryThread() || folia.ownsRegionChunk(w, cx, cz);
    }
    public boolean isOwnedByCurrentRegion(World w, int cx, int cz, int r) {
        return isPrimaryThread() || (folia.ownsRegionChunk(w, cx, cz)
                && folia.ownsRegionChunk(w, cx + r, cz + r));
    }
    public boolean isOwnedByCurrentRegion(World w, int x0, int z0, int x1, int z1) {
        return isPrimaryThread() || ownsBlockArea(w, Math.min(x0, x1), Math.min(z0, z1),
                Math.max(x0, x1), Math.max(z0, z1));
    }
    public boolean isOwnedByCurrentRegion(org.bukkit.entity.Entity e) {
        org.bukkit.Location at = e.getLocation();
        return at.getWorld() != null && (isPrimaryThread() || folia.ownsRegion(at.getWorld(), at.getBlockX(), at.getBlockZ()));
    }

    private boolean ownsBlockArea(World w, int x0, int z0, int x1, int z1) {
        return folia.ownsRegionChunk(w, x0 >> 4, z0 >> 4)
                && folia.ownsRegionChunk(w, x1 >> 4, z1 >> 4);
    }
    public int getMaxPlayers() { return intProperty("max-players", 20); }
    public boolean getOnlineMode() { return property("online-mode", "true").equals("true"); }
    public int getPort() { return intProperty("server-port", 25565); }
    public String getIp() { return property("server-ip", ""); }
    public String getMotd() { return property("motd", "A Minecraft Server"); }
    private final io.papermc.paper.configuration.ServerConfiguration serverConfig =
            io.papermc.paper.configuration.ServerConfiguration.EMPTY;

    /** Paper API: {@code Bukkit.getServerConfig()}. */
    public io.papermc.paper.configuration.ServerConfiguration getServerConfig() {
        return this.serverConfig;
    }

    /** Paper {@code CraftServer.syncCommands()}: у нас команды ведёт Rust. */
    public void syncCommands() {
    }

    /** Paper API: {@code Server.createBossBar(...)}. */
    public org.bukkit.boss.BossBar createBossBar(String title,
            org.bukkit.boss.BarColor color, org.bukkit.boss.BarStyle style,
            org.bukkit.boss.BarFlag... flags) {
        return new BossBarImpl(title, color, style, flags);
    }

    /** Paper API: keyed-вариант {@code createBossBar}. */
    public org.bukkit.boss.KeyedBossBar createBossBar(org.bukkit.NamespacedKey key,
            String title, org.bukkit.boss.BarColor color, org.bukkit.boss.BarStyle style,
            org.bukkit.boss.BarFlag... flags) {
        return new KeyedBossBarImpl(key, title, color, style, flags);
    }

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
    public List<World> getWorlds() {
        return worlds().values().stream().map(w -> w.api).toList();
    }

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

    /** `Server.getPluginsFolder()` (faststats в InvSee++, метрики плагинов). */
    public File getPluginsFolder() { return pluginsDir; }

    public World getWorld(String name) {
        for (WorldImpl w : worlds().values()) {
            if (w.getName().equals(name)) {
                return w.api;
            }
        }
        return null;
    }

    public World getWorld(UUID uid) {
        for (WorldImpl w : worlds().values()) {
            if (w.getUID().equals(uid)) {
                return w.api;
            }
        }
        return null;
    }

    public World getWorld(org.bukkit.NamespacedKey key) {
        WorldImpl w = key == null ? null : worlds().get(key.toString());
        return w == null ? null : w.api;
    }

    public World getWorld(net.kyori.adventure.key.Key key) {
        WorldImpl w = key == null ? null : worlds().get(key.asString());
        return w == null ? null : w.api;
    }

    public Collection<? extends Player> getOnlinePlayers() {
        return online.values().stream().map(p -> p.api).toList();
    }

    /** Как {@code CraftServer.createProfile}: профиль онлайн-игрока берёт его свойства. */
    public com.destroystokyo.paper.profile.PlayerProfile createProfile(UUID id) {
        return createProfile(id, null);
    }

    public com.destroystokyo.paper.profile.PlayerProfile createProfile(String name) {
        return createProfile(null, name);
    }

    public com.destroystokyo.paper.profile.PlayerProfile createProfile(UUID id, String name) {
        PlayerImpl p = id != null ? online.get(id) : null;
        if (p != null && (name == null || name.equals(p.name))) {
            return p.getPlayerProfile();
        }
        return createProfileExact(id, name);
    }

    public com.destroystokyo.paper.profile.PlayerProfile createProfileExact(UUID id, String name) {
        if (id == null && name == null) {
            throw new IllegalArgumentException("Name and ID cannot both be null");
        }
        return new ProfileImpl(id, name);
    }

    public Player getPlayer(UUID id) {
        PlayerImpl p = online.get(id);
        return p == null ? null : p.api;
    }

    /** `Server.getOfflinePlayer(UUID)`: онлайн-игрок или офлайн-обёртка. */
    public org.bukkit.OfflinePlayer getOfflinePlayer(UUID id) {
        Player online = getPlayer(id);
        if (online != null) {
            return online;
        }
        return new OfflinePlayerImpl(this, id, knownNames.get(id)).api;
    }

    /** `Server.getOfflinePlayer(String)`: как в offline-режиме (`OfflinePlayer:<имя>`). */
    public org.bukkit.OfflinePlayer getOfflinePlayer(String name) {
        Player online = getPlayerExact(name);
        if (online != null) {
            return online;
        }
        for (Map.Entry<UUID, String> e : knownNames.entrySet()) {
            if (e.getValue().equalsIgnoreCase(name)) {
                return new OfflinePlayerImpl(this, e.getKey(), e.getValue()).api;
            }
        }
        UUID id = UUID.nameUUIDFromBytes(
                ("OfflinePlayer:" + name).getBytes(java.nio.charset.StandardCharsets.UTF_8));
        knownNames.put(id, name);
        return new OfflinePlayerImpl(this, id, name).api;
    }

    /** `Server.getOfflinePlayerIfCached(String)`: только известные имена. */
    public org.bukkit.OfflinePlayer getOfflinePlayerIfCached(String name) {
        Player online = getPlayerExact(name);
        if (online != null) {
            return online;
        }
        for (Map.Entry<UUID, String> e : knownNames.entrySet()) {
            if (e.getValue().equalsIgnoreCase(name)) {
                return new OfflinePlayerImpl(this, e.getKey(), e.getValue()).api;
            }
        }
        return null;
    }

    public org.bukkit.OfflinePlayer[] getOfflinePlayers() {
        java.util.List<org.bukkit.OfflinePlayer> out = new java.util.ArrayList<>();
        for (Map.Entry<UUID, String> e : knownNames.entrySet()) {
            if (getPlayer(e.getKey()) == null) {
                out.add(new OfflinePlayerImpl(this, e.getKey(), e.getValue()).api);
            }
        }
        return out.toArray(new org.bukkit.OfflinePlayer[0]);
    }

    /** Имя по UUID из кэша входов (для `OfflinePlayer.getName`). */
    String nameOf(UUID id) {
        PlayerImpl p = online.get(id);
        return p != null ? p.name : knownNames.get(id);
    }

    boolean hasPlayed(UUID id) {
        return knownNames.containsKey(id);
    }

    long lastSeen(UUID id) {
        return seen.getOrDefault(id, 0L);
    }

    com.destroystokyo.paper.profile.PlayerProfile profile(UUID id, String name) {
        return createProfile(id, name);
    }

    org.bukkit.profile.PlayerProfile bukkitProfile(UUID id, String name) {
        return createProfile(id, name);
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
    /** Отложенные обработчики {@code LifecycleEvents.COMMANDS}. */
    private final java.util.List<Runnable> pendingCommands = new java.util.ArrayList<>();

    void runLifecycleCommands(String owner, Object configuration, LifecycleEventHandler<?> handler) {
        // Paper запускает COMMANDS после onEnable всех плагинов; синхронный
        // запуск ломает state-machine Incendo Cloud (GrimAC): обработчик
        // вызывался прямо из registerEventHandler, лок регистрации
        // срабатывал раньше, чем Cloud выставит commandRegistrationHandler.
        pendingCommands.add(() -> dispatchCommands(owner, configuration, handler));
    }

    /** Выполняет отложенные COMMANDS-обработчики (конец включения плагинов). */
    void flushLifecycleCommands() {
        java.util.List<Runnable> queued = new java.util.ArrayList<>(this.pendingCommands);
        this.pendingCommands.clear();
        for (Runnable runnable : queued) {
            try {
                runnable.run();
            } catch (Throwable broken) {
                this.logger.log(Level.WARNING, "lifecycle: deferred command registration failed",
                        broken);
            }
        }
    }

    private void dispatchCommands(String owner, Object configuration,
            LifecycleEventHandler<?> handler) {
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
    /** Явно печатает цепочку причин (log4j теряет причину ExceptionInInitializerError). */
    private void logCauseChain(String what, Throwable error) {
        int depth = 0;
        for (Throwable cause = error.getCause(); cause != null && depth++ < 12; cause = cause.getCause()) {
            logger.log(Level.SEVERE, what + ": caused by "
                    + cause.getClass().getName() + ": " + cause.getMessage(), cause);
        }
    }

    void runLifecycleDatapacks(String owner, LifecycleEventHandler<?> handler) {
        try {
            callHandler(handler, new Tags.DiscoveryEvent(new Tags.Registrar(owner)));
        } catch (Throwable t) {
            logger.log(Level.SEVERE, "lifecycle[" + owner + "]: datapack discovery failed", t);
            // ExceptionInInitializerError иногда теряет исходную причину в логгере;
            // печатаем цепочку явно, иначе нельзя понять, какой член NMS не найден.
            int depth = 0;
            for (Throwable cause = t.getCause(); cause != null && depth++ < 10; cause = cause.getCause()) {
                logger.log(Level.SEVERE, "lifecycle[" + owner + "]: caused by "
                        + cause.getClass().getName() + ": " + cause.getMessage(), cause);
            }
        }
    }

    /** Теги блоков и предметов: ванильные из Rust плюс теги датапаков плагинов. */
    public <T extends org.bukkit.Keyed> org.bukkit.Tag<T> getTag(String registry, org.bukkit.NamespacedKey tag,
            Class<T> clazz) {
        return Tags.get(registry, tag, clazz);
    }

    /** {@code Server.getRegistry(Class)} (Skript/Paper API). */
    public <T extends org.bukkit.Keyed> org.bukkit.Registry<T> getRegistry(Class<T> type) {
        return io.papermc.paper.registry.RegistryAccess.registryAccess().getRegistry(type);
    }

    /** {@code Server.getRegistry(RegistryKey)} (Paper API). */
    public <T extends org.bukkit.Keyed> org.bukkit.Registry<T> getRegistry(
            io.papermc.paper.registry.RegistryKey<T> key) {
        return io.papermc.paper.registry.RegistryAccess.registryAccess().getRegistry(key);
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

}
