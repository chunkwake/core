package dev.chunkwake.bridge;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.UUID;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.ComponentLike;
import net.kyori.adventure.text.serializer.gson.GsonComponentSerializer;
import org.bukkit.GameMode;
import org.bukkit.Material;
import org.bukkit.Particle;
import org.bukkit.Server;
import org.bukkit.World;
import org.bukkit.entity.Player;
import org.bukkit.event.player.PlayerGameModeChangeEvent;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.PlayerInventory;
import java.util.Set;
import org.bukkit.permissions.PermissibleBase;
import org.bukkit.permissions.Permission;
import org.bukkit.permissions.PermissionAttachment;
import org.bukkit.permissions.PermissionAttachmentInfo;
import org.bukkit.plugin.Plugin;

/** {@link Player}: онлайн-игрок Rust-сервера (MVP — всё, что нужно для чата и событий). */
public final class PlayerImpl {
    final Player api;
    private final ChunkwakeServer server;
    final UUID id;
    final String name;
    volatile boolean online = true;
    /** Права как у CraftBukkit: {@code PermissibleBase} над {@link #isOp()}. */
    /** `Player.setDisplayName`/`setPlayerListName` (не в Rust: только витрина). */
    private String display;
    /** `Player.getRespawnLocation`: кровать/якорь, если плагин её задал. */
    private org.bukkit.Location respawn;
    /** `Player.setFallDistance`: витрина (Rust считает свою физику). */
    private float fallDistance;
    /** Последняя заданная скорость: клиент авторитетен, getVelocity возвращает её. */
    private volatile org.bukkit.util.Vector velocity = new org.bukkit.util.Vector();
    /** Скрытые сущности: UUID -> набор плагинов (`*` — hidePlayer без плагина). */
    private final java.util.Map<UUID, Set<String>> hidden =
            new java.util.concurrent.ConcurrentHashMap<>();
    private volatile boolean allowFlight;
    private volatile boolean flying;
    /** `Player.setSleepingIgnored`: не мешать спать (Essentials). */
    private boolean sleepingIgnored;
    /** `Player.getLocale`: клиентский язык (по умолчанию `en_us`). */
    private String locale = "en_us";
    private Component listHeader = Component.empty();
    private Component listFooter = Component.empty();
    private String listHeaderLegacy = "";
    private String listFooterLegacy = "";

    PlayerImpl(ChunkwakeServer server, UUID id, String name) {
        this.server = server;
        this.id = id;
        this.name = name;
        this.api = new org.bukkit.craftbukkit.entity.CraftPlayer(this, id, name);
        // NMS-мост: плагины (DecentHolograms/TAB) шлют пакеты через
        // `player.getHandle().connection.send(...)`.
        ((org.bukkit.craftbukkit.entity.CraftPlayer) this.api).bindConnection(id);
    }

    public String getName() { return name; }
    public UUID getUniqueId() { return id; }
    /** Атрибут игрока: база и модификаторы те же, что у `/attribute`. */
    public org.bukkit.attribute.AttributeInstance getAttribute(org.bukkit.attribute.Attribute a) {
        return AttributeInstanceImpl.of(() -> id, a);
    }
    public void registerAttribute(org.bukkit.attribute.Attribute a) {}
    /** Версия протокола клиента: через ViaVersion — его собственная, иначе наша. */
    public int getProtocolVersion() {
        int via = Bridge.via != null ? Bridge.via.protocol(id) : -1;
        return via >= 0 ? via : ChunkwakeServer.Unsafe.PROTOCOL;
    }
    public Server getServer() { return server.api; }
    /** Мир игрока — измерение из снимка Rust. */
    public World getWorld() { return server.world(state().dim()).api; }
    public boolean isOnline() { return online; }
    public boolean isConnected() { return online; }

    /** Активный счётчик: по умолчанию главный (состояние Rust). */
    private org.bukkit.scoreboard.Scoreboard scoreboard;
    private boolean warnedScoreboard;

    /**
     * {@code Player.getScoreboard()}: главный счётчик, если плагин не подменил.
     *
     * <p>Счётчик из {@code getNewScoreboard()} хранится только в Java и
     * клиенту пока не отправляется; подмена не мешает главному состоянию.
     */
    public org.bukkit.scoreboard.Scoreboard getScoreboard() {
        return this.scoreboard != null ? this.scoreboard : server.mainScoreboard();
    }

    public void setScoreboard(org.bukkit.scoreboard.Scoreboard board) {
        if (board == null) {
            throw new IllegalArgumentException("Scoreboard cannot be null");
        }
        this.scoreboard = board;
        if (!this.warnedScoreboard && board != server.mainScoreboard()) {
            this.warnedScoreboard = true;
            server.logger.warning("Player " + name + " uses a per-player scoreboard; "
                    + "Chunkwake currently sends only the main scoreboard to clients");
        }
    }

    /** {@code ScoreHolder.getScoreboardName()} для Bukkit scoreboard-записей. */
    public String getScoreboardEntryName() { return name; }

    /** {@code OfflinePlayer#getPlayer}: для онлайн-игрока — он сам. */
    public Player getPlayer() { return this.api; }
    public boolean isOp() { return Bridge.opLevel(id) > 0; }
    public void setOp(boolean op) {
        Bridge.setOp(id, op);
        recalculatePermissions();
    }

    /** Базовые права; при смене op (в т.ч. {@code /op} из Rust) пересчитываются. */
    private PermissibleBase perms() {
        // Единый Permissible из CraftHumanEntity.perm: его подменяет LuckPerms.
        return ((org.bukkit.craftbukkit.entity.CraftHumanEntity) api).perm;
    }

    public boolean hasPermission(String p) { return perms().hasPermission(p); }
    public boolean hasPermission(Permission p) { return perms().hasPermission(p); }
    public boolean isPermissionSet(String p) { return perms().isPermissionSet(p); }
    public boolean isPermissionSet(Permission p) { return perms().isPermissionSet(p); }

    /**
     * {@code Permissible#permissionValue}: Paper API объявляет его default-методом,
     * но наш прокси {@code Stub} не выполняет default-тела, поэтому AxiomPaper
     * получал {@code null} и падал на {@code TriState.ordinal()}.
     */
    public net.kyori.adventure.util.TriState permissionValue(String permission) {
        if (permission == null || !this.isPermissionSet(permission)) {
            return net.kyori.adventure.util.TriState.NOT_SET;
        }
        return this.hasPermission(permission)
                ? net.kyori.adventure.util.TriState.TRUE
                : net.kyori.adventure.util.TriState.FALSE;
    }

    public net.kyori.adventure.util.TriState permissionValue(Permission permission) {
        return this.permissionValue(permission == null ? null : permission.getName());
    }
    public PermissionAttachment addAttachment(Plugin plugin) { return perms().addAttachment(plugin); }
    public PermissionAttachment addAttachment(Plugin plugin, String name, boolean value) {
        return perms().addAttachment(plugin, name, value);
    }
    public PermissionAttachment addAttachment(Plugin plugin, int ticks) { return perms().addAttachment(plugin, ticks); }
    public PermissionAttachment addAttachment(Plugin plugin, String name, boolean value, int ticks) {
        return perms().addAttachment(plugin, name, value, ticks);
    }
    public void removeAttachment(PermissionAttachment attachment) { perms().removeAttachment(attachment); }
    public void recalculatePermissions() {
        perms().recalculatePermissions();
    }
    public Set<PermissionAttachmentInfo> getEffectivePermissions() { return perms().getEffectivePermissions(); }
    public Component name() { return Component.text(name); }
    public Component displayName() { return Component.text(getDisplayName()); }
    public String getDisplayName() { return display != null ? display : name; }
    public void setDisplayName(String display) {
        this.display = display;
    }
    public String getPlayerListName() { return getDisplayName(); }
    public void setPlayerListName(String name) {
        this.display = name;
    }

    /** `Player.getRespawnLocation(boolean)`: точка возрождения (кровать/якорь). */
    public org.bukkit.Location getRespawnLocation(boolean loadLocationAndValidate) {
        return respawn;
    }

    public void setRespawnLocation(org.bukkit.Location location, boolean force) {
        this.respawn = location;
    }

    public void setBedSpawnLocation(org.bukkit.Location location, boolean force) {
        this.respawn = location;
    }

    public float getFallDistance() { return this.fallDistance; }

    public void setFallDistance(float distance) { this.fallDistance = distance; }

    public boolean isSleepingIgnored() { return this.sleepingIgnored; }

    public void setSleepingIgnored(boolean ignored) { this.sleepingIgnored = ignored; }

    public String getLocale() { return this.locale; }

    public void setLocale(String locale) {
        this.locale = locale == null ? "en_us" : locale;
    }

    /** `Player.isInWater`: вода или waterlogged-блок в ногах/голове. */
    public boolean isInWater() {
        return inFluid("minecraft:water");
    }

    public boolean isInLava() {
        return inFluid("minecraft:lava");
    }

    public boolean isUnderWater() {
        if (online) {
            // Верх головы чуть ниже глаз: если и там вода — игрок под водой.
            return inFluidAt(0.0, 0.1, "minecraft:water");
        }
        return false;
    }

    private boolean inFluid(String fluid) {
        return inFluidAt(0.0, 0.0, fluid) || inFluidAt(0.0, 0.9, fluid);
    }

    private boolean inFluidAt(double dx, double dy, String fluid) {
        WorldImpl world = WorldImpl.of(getWorld());
        if (world == null || !online) {
            return false;
        }
        var st = state();
        int x = (int) Math.floor(st.x() + dx);
        int y = (int) Math.floor(st.y() + dy);
        int z = (int) Math.floor(st.z());
        String data = WorldImpl.request(world.api, "data " + x + " " + y + " " + z);
        return data.startsWith(fluid + "[") || data.equals(fluid)
                || (fluid.equals("minecraft:water") && data.contains("waterlogged=true"));
    }

    public void sendMessage(String message) {
        sendMessage(ChunkwakeServer.legacy(message));
    }

    public void sendMessage(String... messages) {
        for (String m : messages) sendMessage(m);
    }

    public void sendMessage(Component message) {
        if (online) {
            Bridge.sendMessage(id, GsonComponentSerializer.gson().serialize(message));
        }
    }

    public void sendMessage(ComponentLike message) {
        sendMessage(message.asComponent());
    }

    // --- заголовки, action bar, звуки и клиентские блоки ---

    private void playerEffect(String line) {
        if (online) {
            Bridge.world("player " + id + " " + line);
        }
    }

    /** Отправка уже собранной команды частицы (её же использует World-список). */
    void sendParticleCommand(String line) {
        playerEffect(line);
    }

    private void spawnParticle0(Particle particle, double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ, double speed,
            Object data, boolean force) {
        String line = ParticleWire.command(particle, x, y, z, count,
                offsetX, offsetY, offsetZ, speed, data, force);
        if (line != null) {
            sendParticleCommand(line);
        }
    }

    public void spawnParticle(Particle particle, org.bukkit.Location location, int count,
            Object data) {
        if (location == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        spawnParticle0(particle, location.getX(), location.getY(), location.getZ(),
                count, 0.0, 0.0, 0.0, 0.0, data, false);
    }

    public void spawnParticle(Particle particle, org.bukkit.Location location, int count,
            double offsetX, double offsetY, double offsetZ, double speed) {
        if (location == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        spawnParticle0(particle, location.getX(), location.getY(), location.getZ(),
                count, offsetX, offsetY, offsetZ, speed, null, false);
    }

    public void spawnParticle(Particle particle, org.bukkit.Location location, int count,
            double offsetX, double offsetY, double offsetZ) {
        if (location == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        spawnParticle0(particle, location.getX(), location.getY(), location.getZ(),
                count, offsetX, offsetY, offsetZ, 0.0, null, false);
    }

    public void spawnParticle(Particle particle, double x, double y, double z, int count) {
        spawnParticle0(particle, x, y, z, count, 0.0, 0.0, 0.0, 0.0, null, false);
    }

    public void spawnParticle(Particle particle, double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ) {
        spawnParticle0(particle, x, y, z, count,
                offsetX, offsetY, offsetZ, 0.0, null, false);
    }

    public void spawnParticle(Particle particle, org.bukkit.Location location, int count,
            double offsetX, double offsetY, double offsetZ, Object data) {
        if (location == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        spawnParticle0(particle, location.getX(), location.getY(), location.getZ(),
                count, offsetX, offsetY, offsetZ, 0.0, data, false);
    }

    public void spawnParticle(Particle particle, double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ, Object data) {
        spawnParticle0(particle, x, y, z, count,
                offsetX, offsetY, offsetZ, 0.0, data, false);
    }

    public void spawnParticle(Particle particle, double x, double y, double z, int count,
            Object data) {
        spawnParticle0(particle, x, y, z, count, 0.0, 0.0, 0.0, 0.0, data, false);
    }

    public void spawnParticle(Particle particle, org.bukkit.Location location, int count,
            double offsetX, double offsetY, double offsetZ, double speed,
            Object data, boolean force) {
        if (location == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        spawnParticle0(particle, location.getX(), location.getY(), location.getZ(),
                count, offsetX, offsetY, offsetZ, speed, data, force);
    }

    public void spawnParticle(Particle particle, double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ, double speed,
            Object data, boolean force) {
        spawnParticle0(particle, x, y, z, count,
                offsetX, offsetY, offsetZ, speed, data, force);
    }

    /**
     * Paper-ядро `Player.spawnParticle`: скорость по осям в пакете моста
     * представлена одним значением ({@code speedX}), randomization — DEFAULT.
     */
    public void spawnParticle(Particle particle, double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ,
            double speedX, double speedY, double speedZ, Object data, boolean force,
            Particle.RandomizationType randomizationType) {
        spawnParticle0(particle, x, y, z, count,
                offsetX, offsetY, offsetZ, speedX, data, force);
    }

    public void spawnParticle(Particle particle, org.bukkit.Location location, int count) {
        if (location == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        spawnParticle0(particle, location.getX(), location.getY(), location.getZ(),
                count, 0.0, 0.0, 0.0, 0.0, null, false);
    }

    public void spawnParticle(Particle particle, double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ, double speed, Object data) {
        spawnParticle0(particle, x, y, z, count,
                offsetX, offsetY, offsetZ, speed, data, false);
    }

    public void spawnParticle(Particle particle, org.bukkit.Location location, int count,
            double offsetX, double offsetY, double offsetZ, double speed, Object data) {
        if (location == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        spawnParticle0(particle, location.getX(), location.getY(), location.getZ(),
                count, offsetX, offsetY, offsetZ, speed, data, false);
    }

    public void spawnParticle(Particle particle, double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ, double speed) {
        spawnParticle0(particle, x, y, z, count,
                offsetX, offsetY, offsetZ, speed, null, false);
    }

    private void sendTabList() {
        String header = GsonComponentSerializer.gson().serialize(listHeader);
        String footer = GsonComponentSerializer.gson().serialize(listFooter);
        playerEffect("tablist "
                + java.util.HexFormat.of().formatHex(header.getBytes(java.nio.charset.StandardCharsets.UTF_8))
                + " "
                + java.util.HexFormat.of().formatHex(footer.getBytes(java.nio.charset.StandardCharsets.UTF_8)));
    }

    private static String legacyText(Component component) {
        return net.kyori.adventure.text.serializer.legacy.LegacyComponentSerializer
                .legacySection().serialize(component);
    }

    public void setPlayerListHeader(String header) {
        listHeaderLegacy = header == null ? "" : header;
        listHeader = header == null ? Component.empty() : ChunkwakeServer.legacy(header);
        sendTabList();
    }

    public void setPlayerListFooter(String footer) {
        listFooterLegacy = footer == null ? "" : footer;
        listFooter = footer == null ? Component.empty() : ChunkwakeServer.legacy(footer);
        sendTabList();
    }

    public void setPlayerListHeaderFooter(String header, String footer) {
        listHeaderLegacy = header == null ? "" : header;
        listFooterLegacy = footer == null ? "" : footer;
        listHeader = header == null ? Component.empty() : ChunkwakeServer.legacy(header);
        listFooter = footer == null ? Component.empty() : ChunkwakeServer.legacy(footer);
        sendTabList();
    }

    public void setPlayerListHeaderFooter(net.md_5.bungee.api.chat.BaseComponent header,
            net.md_5.bungee.api.chat.BaseComponent footer) {
        setPlayerListHeaderFooter(
                header == null ? null : net.md_5.bungee.api.chat.TextComponent.toLegacyText(header),
                footer == null ? null : net.md_5.bungee.api.chat.TextComponent.toLegacyText(footer));
    }

    public void setPlayerListHeaderFooter(net.md_5.bungee.api.chat.BaseComponent[] header,
            net.md_5.bungee.api.chat.BaseComponent[] footer) {
        setPlayerListHeaderFooter(
                header == null ? null : net.md_5.bungee.api.chat.TextComponent.toLegacyText(header),
                footer == null ? null : net.md_5.bungee.api.chat.TextComponent.toLegacyText(footer));
    }

    public String getPlayerListHeader() { return listHeaderLegacy; }
    public String getPlayerListFooter() { return listFooterLegacy; }

    public void sendPlayerListHeader(Component header) {
        listHeader = header == null ? Component.empty() : header;
        listHeaderLegacy = legacyText(listHeader);
        sendTabList();
    }

    public void sendPlayerListHeader(ComponentLike header) {
        sendPlayerListHeader(header == null ? null : header.asComponent());
    }

    public void sendPlayerListFooter(Component footer) {
        listFooter = footer == null ? Component.empty() : footer;
        listFooterLegacy = legacyText(listFooter);
        sendTabList();
    }

    public void sendPlayerListFooter(ComponentLike footer) {
        sendPlayerListFooter(footer == null ? null : footer.asComponent());
    }

    public void sendPlayerListHeaderAndFooter(Component header, Component footer) {
        listHeader = header == null ? Component.empty() : header;
        listFooter = footer == null ? Component.empty() : footer;
        listHeaderLegacy = legacyText(listHeader);
        listFooterLegacy = legacyText(listFooter);
        sendTabList();
    }

    public void sendPlayerListHeaderAndFooter(ComponentLike header, ComponentLike footer) {
        sendPlayerListHeaderAndFooter(header == null ? null : header.asComponent(),
                footer == null ? null : footer.asComponent());
    }

    private void titlePart(String part, Component component) {
        if (component != null) {
            playerEffect("title " + part + " "
                    + GsonComponentSerializer.gson().serialize(component));
        }
    }

    private void titleTimes(int fadeIn, int stay, int fadeOut) {
        playerEffect("times " + fadeIn + " " + stay + " " + fadeOut);
    }

    public void sendActionBar(String message) {
        sendActionBar(ChunkwakeServer.legacy(message));
    }

    public void sendActionBar(char alternateChar, String message) {
        sendActionBar(message);
    }

    public void sendActionBar(Component message) {
        titlePart("actionbar", message);
    }

    public void sendActionBar(ComponentLike message) {
        sendActionBar(message.asComponent());
    }

    public void sendActionBar(net.md_5.bungee.api.chat.BaseComponent[] message) {
        sendActionBar(ChunkwakeServer.legacy(
                net.md_5.bungee.api.chat.TextComponent.toLegacyText(message)));
    }

    public void sendTitle(String title, String subtitle) {
        sendTitle(title, subtitle, 10, 70, 20);
    }

    public void sendTitle(String title, String subtitle, int fadeIn, int stay, int fadeOut) {
        if (title != null) {
            titlePart("title", ChunkwakeServer.legacy(title));
        }
        if (subtitle != null) {
            titlePart("subtitle", ChunkwakeServer.legacy(subtitle));
        }
        titleTimes(fadeIn, stay, fadeOut);
    }

    public void showTitle(com.destroystokyo.paper.Title title) {
        if (title != null) {
            sendTitle(net.md_5.bungee.api.chat.TextComponent.toLegacyText(title.getTitle()),
                    net.md_5.bungee.api.chat.TextComponent.toLegacyText(title.getSubtitle()),
                    title.getFadeIn(), title.getStay(), title.getFadeOut());
        }
    }

    public void showTitle(net.md_5.bungee.api.chat.BaseComponent title) {
        sendTitle(net.md_5.bungee.api.chat.TextComponent.toLegacyText(title), null);
    }

    public void showTitle(net.md_5.bungee.api.chat.BaseComponent[] title) {
        sendTitle(net.md_5.bungee.api.chat.TextComponent.toLegacyText(title), null);
    }

    public void showTitle(net.md_5.bungee.api.chat.BaseComponent title,
            net.md_5.bungee.api.chat.BaseComponent subtitle, int fadeIn, int stay, int fadeOut) {
        sendTitle(net.md_5.bungee.api.chat.TextComponent.toLegacyText(title),
                net.md_5.bungee.api.chat.TextComponent.toLegacyText(subtitle),
                fadeIn, stay, fadeOut);
    }

    public void showTitle(net.md_5.bungee.api.chat.BaseComponent[] title,
            net.md_5.bungee.api.chat.BaseComponent[] subtitle, int fadeIn, int stay, int fadeOut) {
        sendTitle(net.md_5.bungee.api.chat.TextComponent.toLegacyText(title),
                net.md_5.bungee.api.chat.TextComponent.toLegacyText(subtitle),
                fadeIn, stay, fadeOut);
    }

    public void showTitle(net.kyori.adventure.title.Title title) {
        if (title == null) {
            return;
        }
        titlePart("title", title.title());
        titlePart("subtitle", title.subtitle());
        net.kyori.adventure.title.Title.Times times = title.times();
        if (times != null) {
            titleTimes((int) (times.fadeIn().toMillis() / 50),
                    (int) (times.stay().toMillis() / 50),
                    (int) (times.fadeOut().toMillis() / 50));
        }
    }

    public void clearTitle() {
        playerEffect("cleartitle 0");
    }

    public void resetTitle() {
        playerEffect("cleartitle 1");
    }

    public void sendTitlePart(net.kyori.adventure.title.TitlePart<?> part, Object value) {
        if (part == net.kyori.adventure.title.TitlePart.TITLE) {
            titlePart("title", asComponent(value));
        } else if (part == net.kyori.adventure.title.TitlePart.SUBTITLE) {
            titlePart("subtitle", asComponent(value));
        } else if (part == net.kyori.adventure.title.TitlePart.TIMES
                && value instanceof net.kyori.adventure.title.Title.Times times) {
            titleTimes((int) (times.fadeIn().toMillis() / 50),
                    (int) (times.stay().toMillis() / 50),
                    (int) (times.fadeOut().toMillis() / 50));
        }
    }

    private static Component asComponent(Object value) {
        if (value instanceof Component component) {
            return component;
        }
        if (value instanceof ComponentLike like) {
            return like.asComponent();
        }
        return value == null ? null : Component.text(value.toString());
    }

    private static String soundName(Object sound) {
        if (sound instanceof org.bukkit.Sound bukkit) {
            return bukkit.getKey().toString();
        }
        if (sound instanceof net.kyori.adventure.sound.Sound adventure) {
            return adventure.name().asString();
        }
        String name = String.valueOf(sound);
        return name.contains(":") ? name : "minecraft:" + name;
    }

    private static String soundSource(Object source) {
        if (source == null) {
            return "MASTER";
        }
        if (source instanceof org.bukkit.SoundCategory category) {
            return category.name();
        }
        return source.toString().toUpperCase(java.util.Locale.ROOT);
    }

    private void playSoundAt(Object sound, Object source, double x, double y, double z,
            float volume, float pitch) {
        playerEffect("sound " + soundName(sound) + " " + soundSource(source)
                + " " + x + " " + y + " " + z + " " + volume + " " + pitch);
    }

    /** Позиционный звук от имени мира: мир сам решает, каким игрокам его отправить. */
    void worldSound(Object sound, Object source, double x, double y, double z,
            float volume, float pitch) {
        playSoundAt(sound, source, x, y, z, volume, pitch);
    }

    public void playSound(org.bukkit.Location location, org.bukkit.Sound sound,
            org.bukkit.SoundCategory source, float volume, float pitch) {
        if (location != null && sound != null) {
            playSoundAt(sound, source, location.getX(), location.getY(), location.getZ(), volume, pitch);
        }
    }

    public void playSound(org.bukkit.Location location, String sound, float volume, float pitch) {
        if (location != null && sound != null) {
            playSoundAt(sound, null, location.getX(), location.getY(), location.getZ(), volume, pitch);
        }
    }

    public void playSound(org.bukkit.Location location, org.bukkit.Sound sound,
            float volume, float pitch) {
        playSound(location, sound, org.bukkit.SoundCategory.MASTER, volume, pitch);
    }

    public void playSound(org.bukkit.Location location, String sound,
            org.bukkit.SoundCategory source, float volume, float pitch) {
        if (location != null && sound != null) {
            playSoundAt(sound, source, location.getX(), location.getY(), location.getZ(), volume, pitch);
        }
    }

    public void playSound(org.bukkit.Location location, org.bukkit.Sound sound,
            org.bukkit.SoundCategory source, float volume, float pitch, long seed) {
        playSound(location, sound, source, volume, pitch);
    }

    public void playSound(org.bukkit.Location location, String sound,
            org.bukkit.SoundCategory source, float volume, float pitch, long seed) {
        playSound(location, sound, source, volume, pitch);
    }

    public void playSound(org.bukkit.entity.Entity entity, org.bukkit.Sound sound,
            float volume, float pitch) {
        playSound(entity == null ? null : entity.getLocation(), sound, volume, pitch);
    }

    public void playSound(org.bukkit.entity.Entity entity, String sound,
            float volume, float pitch) {
        playSound(entity == null ? null : entity.getLocation(), sound, volume, pitch);
    }

    public void playSound(org.bukkit.entity.Entity entity, org.bukkit.Sound sound,
            org.bukkit.SoundCategory source, float volume, float pitch) {
        playSound(entity == null ? null : entity.getLocation(), sound, source, volume, pitch);
    }

    public void playSound(org.bukkit.entity.Entity entity, String sound,
            org.bukkit.SoundCategory source, float volume, float pitch) {
        playSound(entity == null ? null : entity.getLocation(), sound, source, volume, pitch);
    }

    public void playSound(org.bukkit.entity.Entity entity, org.bukkit.Sound sound,
            org.bukkit.SoundCategory source, float volume, float pitch, long seed) {
        playSound(entity, sound, source, volume, pitch);
    }

    public void playSound(org.bukkit.entity.Entity entity, String sound,
            org.bukkit.SoundCategory source, float volume, float pitch, long seed) {
        playSound(entity, sound, source, volume, pitch);
    }

    public void playSound(net.kyori.adventure.sound.Sound sound, double x, double y, double z) {
        if (sound != null) {
            playSoundAt(sound, sound.source(), x, y, z, sound.volume(), sound.pitch());
        }
    }

    public void playSound(net.kyori.adventure.sound.Sound sound) {
        if (sound != null) {
            org.bukkit.Location at = getLocation();
            playSound(sound, at.getX(), at.getY(), at.getZ());
        }
    }

    public void playSound(net.kyori.adventure.sound.Sound sound,
            io.papermc.paper.math.Position position) {
        if (sound != null && position != null) {
            playSound(sound, position.x(), position.y(), position.z());
        }
    }

    public void playSound(net.kyori.adventure.sound.Sound sound,
            net.kyori.adventure.sound.Sound.Emitter emitter) {
        if (sound == null) {
            return;
        }
        if (emitter instanceof org.bukkit.entity.Entity entity) {
            org.bukkit.Location at = entity.getLocation();
            playSound(sound, at.getX(), at.getY(), at.getZ());
        } else {
            playSound(sound);
        }
    }

    public void stopSound(String sound) {
        if (sound != null) {
            playerEffect("stopsound " + soundName(sound) + " *");
        }
    }

    public void stopSound(org.bukkit.Sound sound) {
        if (sound != null) {
            stopSound(sound.getKey().toString());
        }
    }

    public void stopSound(org.bukkit.SoundCategory source) {
        playerEffect("stopsound * " + soundSource(source));
    }

    public void stopSound(String sound, org.bukkit.SoundCategory source) {
        if (sound != null) {
            playerEffect("stopsound " + soundName(sound) + " " + soundSource(source));
        }
    }

    public void stopSound(org.bukkit.Sound sound, org.bukkit.SoundCategory source) {
        if (sound != null) {
            playerEffect("stopsound " + sound.getKey() + " " + soundSource(source));
        }
    }

    public void stopSound(net.kyori.adventure.sound.Sound sound) {
        if (sound != null) {
            stopSound(sound.name().asString(), org.bukkit.SoundCategory.valueOf(
                    sound.source().name()));
        }
    }

    public void stopSound(net.kyori.adventure.sound.SoundStop stop) {
        if (stop == null) {
            return;
        }
        net.kyori.adventure.key.Key key = stop.sound();
        net.kyori.adventure.sound.Sound.Source source = stop.source();
        playerEffect("stopsound " + (key == null ? "*" : key.asString())
                + " " + (source == null ? "*" : source.name()));
    }

    public void sendBlockChange(org.bukkit.Location location,
            org.bukkit.block.data.BlockData data) {
        if (location != null && data != null) {
            playerEffect("block " + location.getBlockX() + " " + location.getBlockY()
                    + " " + location.getBlockZ() + " " + data.getAsString());
        }
    }

    public void sendBlockChange(org.bukkit.Location location,
            org.bukkit.Material material, byte data) {
        if (location != null && material != null && material.isBlock()) {
            sendBlockChange(location, material.createBlockData());
        }
    }

    public void sendBlockChanges(Collection<org.bukkit.block.BlockState> blocks) {
        sendBlockChanges(blocks, true);
    }

    public void sendBlockChanges(Collection<org.bukkit.block.BlockState> blocks,
            boolean suppressLightUpdates) {
        if (blocks == null) {
            return;
        }
        for (org.bukkit.block.BlockState block : blocks) {
            if (block != null) {
                sendBlockChange(block.getLocation(), block.getBlockData());
            }
        }
    }

    // --- состояние из Rust-сессии ---

    /** Снимок игрока: здоровье, голод, режим, инвентарь в индексах Bukkit. */
    record State(double health, int food, float saturation, float exhaustion, int gameMode,
            int selected, double x, double y, double z, float yaw, float pitch, ItemStack[] items,
            List<org.bukkit.potion.PotionEffect> effects, int level, float exp, int totalExp, boolean sneaking,
            String dim) {
        static State parse(String text) {
            String[] lines = text.split("\n", -1);
            List<org.bukkit.potion.PotionEffect> effects = new ArrayList<>();
            for (int i = 1 + InventoryImpl.SIZE; i < lines.length; i++) {
                String[] e = lines[i].split(" ");
                if (e.length >= 5 && e[0].equals("e")) {
                    var type = org.bukkit.Registry.EFFECT.get(org.bukkit.NamespacedKey.fromString(e[1]));
                    if (type != null) {
                        boolean ambient = e.length > 5 && e[5].equals("1");
                        boolean icon = e.length <= 6 || e[6].equals("1");
                        effects.add(new org.bukkit.potion.PotionEffect(type, Integer.parseInt(e[3]),
                                Integer.parseInt(e[2]), ambient, e[4].equals("1"), icon));
                    }
                }
            }
            String[] h = lines[0].split(" ");
            ItemStack[] items = new ItemStack[InventoryImpl.SIZE];
            for (int i = 0; i < items.length; i++) {
                String l = i + 1 < lines.length ? lines[i + 1] : "-";
                items[i] = StackImpl.parse(l);
            }
            return new State(Double.parseDouble(h[0]), Integer.parseInt(h[1]), Float.parseFloat(h[2]),
                    Float.parseFloat(h[3]), Integer.parseInt(h[4]), Integer.parseInt(h[5]),
                    Double.parseDouble(h[6]), Double.parseDouble(h[7]), Double.parseDouble(h[8]),
                    Float.parseFloat(h[9]), Float.parseFloat(h[10]), items, effects,
                    h.length > 13 ? Integer.parseInt(h[11]) : 0, h.length > 13 ? Float.parseFloat(h[12]) : 0,
                    h.length > 13 ? Integer.parseInt(h[13]) : 0,
                    h.length > 14 && h[14].equals("1"),
                    h.length > 15 ? h[15] : WorldImpl.OVERWORLD);
        }
    }

    private State last;
    /** Отпечаток состояния, для которого собран {@link #last}. */
    private long lastVersion = Long.MIN_VALUE;
    private final InventoryImpl inventory = new InventoryImpl(this);

    /** Текущий снимок; после выхода — последний известный. */
    State state() {
        if (online) {
            // Сначала дешёвый отпечаток: пока состояние не менялось, разбор
            // снимается с прошлого результата. Значения те же, что отдавал
            // пересбор снимка на каждый геттер.
            long version = Bridge.playerVersion(id);
            if (version >= 0 && version == lastVersion && last != null) {
                return last;
            }
            String text = Bridge.playerState(id);
            if (text != null) {
                last = State.parse(text);
                lastVersion = version;
            }
        }
        if (last == null) {
            throw new IllegalStateException("chunkwake: no state for " + name);
        }
        return last;
    }

    /** Каналы, которые клиент объявил через `minecraft:register`. */
    private final Set<String> channels = java.util.concurrent.ConcurrentHashMap.newKeySet();

    public void sendPluginMessage(Plugin source, String channel, byte[] message) {
        // Как CraftPlayer: клиент сам объявляет каналы через `minecraft:register`.
        if (channels.contains(channel)) {
            sendPayload(channel, message);
        }
    }

    public void addChannel(String channel) {
        if (channel != null) {
            this.channels.add(channel);
        }
    }

    public void removeChannel(String channel) {
        if (channel != null) {
            this.channels.remove(channel);
        }
    }

    public Set<String> getListeningPluginChannels() {
        return Set.copyOf(channels);
    }

    private void sendPayload(String channel, byte[] message) {
        control("payload " + channel + " " + java.util.HexFormat.of().formatHex(message));
    }

    void sendSupportedChannels(Set<String> incoming) {
        if (!incoming.isEmpty()) {
            sendPayload("minecraft:register",
                    String.join(" ", incoming).getBytes(java.nio.charset.StandardCharsets.UTF_8));
        }
    }

    /** Сообщение канала от клиента: регистрация каналов или данные плагину. */
    void pluginMessage(String channel, byte[] data) {
        boolean register = channel.equals("minecraft:register");
        if (register || channel.equals("minecraft:unregister")) {
            for (String name : new String(data, java.nio.charset.StandardCharsets.UTF_8).split(" ")) {
                if (name.isEmpty()) {
                    continue;
                }
                if (register ? channels.add(name) : channels.remove(name)) {
                    server.api.getPluginManager().callEvent(register
                            ? new org.bukkit.event.player.PlayerRegisterChannelEvent(api, name)
                            : new org.bukkit.event.player.PlayerUnregisterChannelEvent(api, name));
                }
            }
            return;
        }
        server.getMessenger().dispatchIncomingMessage(api, channel, data);
    }

    void control(String line) {
        if (online) {
            Bridge.playerControl(id, line);
        }
    }

    // --- таблички ---

    /** {@code Player.openSign}: события с причиной {@code PLUGIN}, затем редактор у клиента. */
    public void openSign(org.bukkit.block.Sign sign, org.bukkit.block.sign.Side side) {
        if (sign == null || !sign.isPlaced() || sign.getWorld() != getWorld()) {
            throw new IllegalArgumentException("Sign must be placed in the player's world");
        }
        org.bukkit.block.Block b = sign.getBlock();
        if (Events.openSignEvents(api, b, side, io.papermc.paper.event.player.PlayerOpenSignEvent.Cause.PLUGIN)) {
            control("opensign " + b.getX() + " " + b.getY() + " " + b.getZ() + " " + side.name());
        }
    }

    public void openSign(org.bukkit.block.Sign sign) { openSign(sign, org.bukkit.block.sign.Side.FRONT); }

    // --- метаданные и PDC ---

    public void setMetadata(String key, org.bukkit.metadata.MetadataValue value) {
        server.metadata().players.setMetadata(api, key, value);
    }
    public List<org.bukkit.metadata.MetadataValue> getMetadata(String key) {
        return server.metadata().players.getMetadata(api, key);
    }
    public boolean hasMetadata(String key) { return server.metadata().players.hasMetadata(api, key); }
    public void removeMetadata(String key, org.bukkit.plugin.Plugin owner) {
        server.metadata().players.removeMetadata(api, key, owner);
    }

    private volatile PdcImpl pdc;

    /**
     * PDC игрока — {@code BukkitValues} его {@code playerdata}: держит Rust,
     * правка сразу уходит туда и сохраняется с игроком.
     */
    public org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        PdcImpl p = pdc;
        if (p == null) {
            synchronized (this) {
                if (pdc == null) {
                    java.util.Map<String, Object> tags = new java.util.LinkedHashMap<>();
                    if (Bridge.table != null) {
                        String json = Bridge.world("pdc " + id);
                        if (!json.isEmpty()) {
                            tags.putAll(PdcImpl.Nbt.decode(
                                    com.google.gson.JsonParser.parseString(json).getAsJsonObject()));
                        }
                    }
                    pdc = new PdcImpl(tags, t -> {
                        if (Bridge.table != null) {
                            Bridge.world("pdc " + id + " " + PdcImpl.Nbt.encode(t));
                        }
                    });
                }
                p = pdc;
            }
        }
        return p.api;
    }

    // --- профиль, адрес и кик ---

    /** Профиль с входа (свойства session server); до входа — только имя и UUID. */
    volatile ProfileImpl profile;
    volatile java.net.InetSocketAddress address;
    /** Сообщение о выходе из {@code PlayerKickEvent}: {@code quit} шлёт его с {@code KICKED}. */
    volatile Component kickLeaveMessage;
    volatile boolean kicked;

    /** Как {@code CraftPlayer.getPlayerProfile}: копия, правки не меняют игрока. */
    public com.destroystokyo.paper.profile.PlayerProfile getPlayerProfile() {
        ProfileImpl p = profile;
        return p != null ? p.clone() : new ProfileImpl(id, name);
    }

    public void setPlayerProfile(com.destroystokyo.paper.profile.PlayerProfile p) {
        ProfileImpl copy = new ProfileImpl(id, name);
        copy.setProperties(p.getProperties());
        profile = copy;
    }

    /** Как {@code CraftPlayer.getAddress}: без соединения — loopback:0. */
    public java.net.InetSocketAddress getAddress() {
        java.net.InetSocketAddress a = address;
        return a != null ? a : new java.net.InetSocketAddress(java.net.InetAddress.getLoopbackAddress(), 0);
    }

    public void kickPlayer(String message) {
        kick(message == null ? Component.empty() : ChunkwakeServer.legacy(message),
                org.bukkit.event.player.PlayerKickEvent.Cause.PLUGIN);
    }

    /**
     * {@code ServerGamePacketListenerImpl.disconnect} Paper: {@code PlayerKickEvent}
     * с сообщением о выходе {@code multiplayer.player.left}, отмена — игрок остаётся.
     */
    public void kick(Component reason, org.bukkit.event.player.PlayerKickEvent.Cause cause) {
        if (!online) {
            return;
        }
        var e = fireKick(reason == null ? Component.empty() : reason, cause);
        if (e != null) {
            control("kick " + GsonComponentSerializer.gson().serialize(e.reason()));
        }
    }

    /** Событие кика; {@code null} — отменено. */
    private org.bukkit.event.player.PlayerKickEvent fireKick(Component reason,
            org.bukkit.event.player.PlayerKickEvent.Cause cause) {
        Component leave = Component.translatable("multiplayer.player.left",
                net.kyori.adventure.text.format.NamedTextColor.YELLOW, Component.text(name));
        var e = new org.bukkit.event.player.PlayerKickEvent(api, reason, leave, cause);
        server.api.getPluginManager().callEvent(e);
        if (e.isCancelled()) {
            return null;
        }
        kickLeaveMessage = e.leaveMessage();
        kicked = true;
        return e;
    }

    /** Кик сервером (команды, бан): {@code ПРИЧИНА\n<json>} → {@code c} или JSON причины. */
    String kickEvent(String arg) {
        int nl = arg.indexOf('\n');
        var cause = org.bukkit.event.player.PlayerKickEvent.Cause.valueOf(arg.substring(0, nl));
        Component reason = GsonComponentSerializer.gson().deserialize(arg.substring(nl + 1));
        var e = fireKick(reason, cause);
        return e == null ? "c" : GsonComponentSerializer.gson().serialize(e.reason());
    }

    public PlayerInventory getInventory() { return inventory.api; }

    /** Переслать клиенту актуальный инвентарь (Bukkit {@code Player.updateInventory}). */
    public void updateInventory() { playerEffect("inventory"); }

    /**
     * Устаревший {@code Player.getItemInHand()}: CraftHumanEntity
     * делегирует его в handle, поэтому метод должен быть здесь.
     * GriefPrevention вызывает {@code getItemInHand().getType()} и падал
     * на null.
     */
    public org.bukkit.inventory.ItemStack getItemInHand() {
        return inventory.api.getItemInMainHand();
    }

    public void setItemInHand(org.bukkit.inventory.ItemStack item) {
        inventory.api.setItemInMainHand(item);
    }

    /** `CommandSender.spigot()`: SkinsRestorer и другие шлют BaseComponent. */
    public org.bukkit.entity.Player.Spigot spigot() {
        return new SpigotImpl(this);
    }

    static final class SpigotImpl extends org.bukkit.entity.Player.Spigot {
        private final PlayerImpl player;

        SpigotImpl(PlayerImpl player) {
            this.player = player;
        }

        @Override
        public void sendMessage(net.md_5.bungee.api.chat.BaseComponent component) {
            this.player.sendMessage(net.md_5.bungee.api.chat.TextComponent.toLegacyText(component));
        }

        @Override
        public void sendMessage(net.md_5.bungee.api.chat.BaseComponent... components) {
            this.player.sendMessage(net.md_5.bungee.api.chat.TextComponent.toLegacyText(components));
        }

        @Override
        public void sendMessage(java.util.UUID sender,
                net.md_5.bungee.api.chat.BaseComponent component) {
            this.sendMessage(component);
        }

        @Override
        public void sendMessage(java.util.UUID sender,
                net.md_5.bungee.api.chat.BaseComponent... components) {
            this.sendMessage(components);
        }

        @Override
        public void sendMessage(net.md_5.bungee.api.ChatMessageType type,
                net.md_5.bungee.api.chat.BaseComponent component) {
            this.sendMessage(component);
        }

        @Override
        public void sendMessage(net.md_5.bungee.api.ChatMessageType type,
                net.md_5.bungee.api.chat.BaseComponent... components) {
            this.sendMessage(components);
        }

        @Override
        public void sendMessage(net.md_5.bungee.api.ChatMessageType type, java.util.UUID sender,
                net.md_5.bungee.api.chat.BaseComponent component) {
            this.sendMessage(component);
        }

        @Override
        public void sendMessage(net.md_5.bungee.api.ChatMessageType type, java.util.UUID sender,
                net.md_5.bungee.api.chat.BaseComponent... components) {
            this.sendMessage(components);
        }

        @Override
        public int getPing() {
            return 0;
        }

        @Override
        public java.net.InetSocketAddress getRawAddress() {
            return null;
        }

        @Override
        public void respawn() {
        }

        @Override
        public java.util.Set<org.bukkit.entity.Player> getHiddenPlayers() {
            return java.util.Set.of();
        }
    }

    /** `Player.getEquipment()`: ImageFrame и другие плагины. */
    public org.bukkit.inventory.EntityEquipment getEquipment() {
        return Stub.of(org.bukkit.inventory.EntityEquipment.class, new EquipmentImpl(this));
    }

    static final class EquipmentImpl {
        private final PlayerImpl player;

        EquipmentImpl(PlayerImpl player) {
            this.player = player;
        }

        public org.bukkit.inventory.ItemStack getItemInMainHand() {
            return this.player.getInventory().getItemInMainHand();
        }

        public void setItemInMainHand(org.bukkit.inventory.ItemStack item) {
            this.player.getInventory().setItemInMainHand(item);
        }

        public org.bukkit.inventory.ItemStack getItemInOffHand() {
            return this.player.getInventory().getItemInOffHand();
        }

        public void setItemInOffHand(org.bukkit.inventory.ItemStack item) {
            this.player.getInventory().setItemInOffHand(item);
        }

        public org.bukkit.inventory.ItemStack getItem(org.bukkit.inventory.EquipmentSlot slot) {
            return switch (slot) {
                case HAND -> this.getItemInMainHand();
                case OFF_HAND -> this.getItemInOffHand();
                case HEAD -> this.player.getInventory().getHelmet();
                case CHEST -> this.player.getInventory().getChestplate();
                case LEGS -> this.player.getInventory().getLeggings();
                case FEET -> this.player.getInventory().getBoots();
                default -> null;
            };
        }

        public void setItem(org.bukkit.inventory.EquipmentSlot slot,
                org.bukkit.inventory.ItemStack item) {
            switch (slot) {
                case HAND -> this.setItemInMainHand(item);
                case OFF_HAND -> this.setItemInOffHand(item);
                case HEAD -> this.player.getInventory().setHelmet(item);
                case CHEST -> this.player.getInventory().setChestplate(item);
                case LEGS -> this.player.getInventory().setLeggings(item);
                case FEET -> this.player.getInventory().setBoots(item);
                default -> {
                }
            }
        }

        public org.bukkit.inventory.ItemStack getHelmet() {
            return this.player.getInventory().getHelmet();
        }

        public void setHelmet(org.bukkit.inventory.ItemStack item) {
            this.player.getInventory().setHelmet(item);
        }

        public org.bukkit.inventory.ItemStack getChestplate() {
            return this.player.getInventory().getChestplate();
        }

        public void setChestplate(org.bukkit.inventory.ItemStack item) {
            this.player.getInventory().setChestplate(item);
        }

        public org.bukkit.inventory.ItemStack getLeggings() {
            return this.player.getInventory().getLeggings();
        }

        public void setLeggings(org.bukkit.inventory.ItemStack item) {
            this.player.getInventory().setLeggings(item);
        }

        public org.bukkit.inventory.ItemStack getBoots() {
            return this.player.getInventory().getBoots();
        }

        public void setBoots(org.bukkit.inventory.ItemStack item) {
            this.player.getInventory().setBoots(item);
        }
    }

    /** Открытое окно плагина (null — своё окно игрока или окно блока). */
    private volatile ViewImpl openView;
    private volatile PluginInventory openInv;
    /** id окна Rust для ванильного контейнера; -1 — окно плагина/закрыто. */
    private volatile int openWindowId = -1;

    public org.bukkit.inventory.InventoryView openInventory(org.bukkit.inventory.Inventory inv) {
        if (!(Stub.impl(inv) instanceof PluginInventory pi)) {
            if (inv == inventory.api) {
                return getOpenInventory();
            }
            throw new UnsupportedOperationException("chunkwake: openInventory " + inv.getType());
        }
        ViewImpl view = new ViewImpl(this, inv, pi.type, pi.title);
        var e = new org.bukkit.event.inventory.InventoryOpenEvent(view.api);
        server.api.getPluginManager().callEvent(e);
        if (e.isCancelled()) {
            return null;
        }
        Component title = e.titleOverride() != null ? e.titleOverride() : pi.title;
        openView = view;
        openInv = pi;
        openWindowId = -1;
        pi.viewers.add(api);
        control("open " + pi.id + " " + GsonComponentSerializer.gson().serialize(title));
        return view.api;
    }

    public org.bukkit.inventory.InventoryView getOpenInventory() {
        ViewImpl v = openView;
        return v != null ? v.api : craftingView().api;
    }

    ViewImpl craftingView() {
        return new ViewImpl(this, ViewImpl.blockTop(this, org.bukkit.event.inventory.InventoryType.CRAFTING, 5),
                org.bukkit.event.inventory.InventoryType.CRAFTING, Component.translatable("container.crafting"));
    }

    /** Вид для события клика: открытое окно плагина, если клик в нём. */
    ViewImpl viewFor(int windowId) {
        ViewImpl v = openView;
        if (v == null) {
            return null;
        }
        PluginInventory pi = openInv;
        if (pi != null) {
            return pi.id == windowId ? v : null;
        }
        return openWindowId == windowId ? v : null;
    }

    /** Ванильный контейнер открыт: {@code InventoryOpenEvent} до open_screen. */
    String inventoryOpened(String arg) {
        String[] lines = arg.split("\n", -1);
        String[] f = lines[0].split(" ");
        if (f.length < 4) {
            return "c";
        }
        org.bukkit.event.inventory.InventoryType type;
        int id, size, x = 0, y = 0, z = 0;
        String holder = "";
        try {
            type = org.bukkit.event.inventory.InventoryType.valueOf(f[0]);
            id = Integer.parseInt(f[1]);
            size = Math.max(0, Integer.parseInt(f[2]));
            if (f.length >= 7) {
                x = Integer.parseInt(f[3]);
                y = Integer.parseInt(f[4]);
                z = Integer.parseInt(f[5]);
                holder = f[6];
            }
        } catch (RuntimeException e) {
            return "c";
        }
        String[] before = new String[size];
        ItemStack[] items = new ItemStack[size];
        for (int i = 0; i < size; i++) {
            String line = lines.length > 1 + i ? lines[1 + i] : "-";
            before[i] = line;
            items[i] = "-".equals(line) ? null : StackImpl.parse(line);
        }
        int titleIndex = 1 + size;
        String titleText = titleIndex < lines.length
                ? String.join("\n", java.util.Arrays.copyOfRange(lines, titleIndex, lines.length))
                : "{}";
        Component title;
        try {
            title = GsonComponentSerializer.gson().deserialize(titleText);
        } catch (RuntimeException e) {
            title = Component.translatable("container." + type.name().toLowerCase(java.util.Locale.ROOT));
        }
        org.bukkit.Location location = null;
        if (type != org.bukkit.event.inventory.InventoryType.CRAFTING) {
            try {
                location = new org.bukkit.Location(api.getWorld(), x, y, z);
            } catch (RuntimeException ignored) {
                // Мир мог быть выгружен: позиция необязательна.
            }
        }
        ViewImpl.BlockTop top = new ViewImpl.BlockTop(this, type, items, location, holder);
        ViewImpl view = new ViewImpl(this, ViewImpl.blockTop(top), type, title, top);
        var event = new org.bukkit.event.inventory.InventoryOpenEvent(view.api);
        server.api.getPluginManager().callEvent(event);
        if (event.isCancelled()) {
            return "c";
        }
        openView = view;
        openInv = null;
        openWindowId = id;
        // Плагин (ChestSort) мог переставить предметы: отдаём Rust только изменения.
        StringBuilder out = new StringBuilder("k");
        for (int i = 0; i < size; i++) {
            String after = StackImpl.line(items[i]);
            if (after == null) {
                after = "-";
            }
            if (!after.equals(before[i])) {
                out.append('\n').append(i).append(' ').append(after);
            }
        }
        return out.toString();
    }

    public void closeInventory() { control("closewin"); }
    public void closeInventory(org.bukkit.event.inventory.InventoryCloseEvent.Reason reason) { closeInventory(); }

    /** Rust закрыл окно плагина {@code id}: событие и сброс вида. */
    String inventoryClosed(String arg) {
        String[] lines = arg.split("\n", -1);
        int id;
        try {
            id = Integer.parseInt(lines[0].trim());
        } catch (RuntimeException e) {
            return "k";
        }
        ViewImpl v = openView;
        if (v == null) {
            return "k";
        }
        PluginInventory pi = openInv;
        int current = pi != null ? pi.id : openWindowId;
        if (current != id) {
            return "k";
        }
        StringBuilder out = new StringBuilder("k");
        if (pi == null && v.blockTop != null) {
            ViewImpl.BlockTop top = v.blockTop;
            String[] before = new String[top.size()];
            for (int i = 0; i < before.length; i++) {
                before[i] = i + 1 < lines.length ? lines[i + 1] : "-";
                top.setLine(i, before[i]);
            }
            server.api.getPluginManager()
                    .callEvent(new org.bukkit.event.inventory.InventoryCloseEvent(v.api));
            // ChestSort по умолчанию сортирует на close: возвращаем правки в Rust.
            for (int i = 0; i < before.length; i++) {
                String after = top.line(i);
                if (!after.equals(before[i])) {
                    out.append('\n').append(i).append(' ').append(after);
                }
            }
        } else {
            server.api.getPluginManager()
                    .callEvent(new org.bukkit.event.inventory.InventoryCloseEvent(v.api));
        }
        openView = null;
        openInv = null;
        openWindowId = -1;
        if (pi != null) {
            pi.viewers.remove(api);
        }
        return out.toString();
    }
    public ItemStack getItemOnCursor() { return new StackImpl(org.bukkit.Material.AIR, 0); }
    public double getHealth() { return state().health(); }
    public double getMaxHealth() { return 20.0; }

    /** Как {@code CraftLivingEntity.setHealth}: 0..max, 0 — смерть. */
    public void setHealth(double health) {
        if (health < 0 || health > getMaxHealth()) {
            throw new IllegalArgumentException("Health value (" + health + ") must be between 0 and " + getMaxHealth());
        }
        control("health " + (float) health);
    }

    // --- эффекты (как CraftLivingEntity: событие с причиной PLUGIN) ---

    public Collection<org.bukkit.potion.PotionEffect> getActivePotionEffects() { return state().effects(); }

    public org.bukkit.potion.PotionEffect getPotionEffect(org.bukkit.potion.PotionEffectType type) {
        for (var e : state().effects()) {
            if (e.getType().equals(type)) {
                return e;
            }
        }
        return null;
    }

    public boolean hasPotionEffect(org.bukkit.potion.PotionEffectType type) { return getPotionEffect(type) != null; }

    public boolean addPotionEffect(org.bukkit.potion.PotionEffect effect) {
        return addPotionEffect(effect, false);
    }

    public boolean addPotionEffect(org.bukkit.potion.PotionEffect effect, boolean force) {
        if (!online || isDead() || effect == null) {
            return false;
        }
        var old = getPotionEffect(effect.getType());
        // `MobEffectInstance.update`: слабее или короче — не применяется, если не force.
        boolean longer = old != null && !old.isInfinite() && (effect.isInfinite() || effect.getDuration() > old.getDuration());
        if (!force && old != null && !(effect.getAmplifier() > old.getAmplifier()
                || (effect.getAmplifier() == old.getAmplifier() && longer))) {
            return false;
        }
        var action = old == null ? org.bukkit.event.entity.EntityPotionEffectEvent.Action.ADDED
                : org.bukkit.event.entity.EntityPotionEffectEvent.Action.CHANGED;
        var e = new org.bukkit.event.entity.EntityPotionEffectEvent(api, old, effect, null,
                org.bukkit.event.entity.EntityPotionEffectEvent.Cause.PLUGIN, action, true);
        server.api.getPluginManager().callEvent(e);
        if (e.isCancelled()) {
            return false;
        }
        var t = effect.getType().getKey();
        return Bridge.playerControl(id, "effect " + t + " " + Math.max(0, Math.min(255, effect.getAmplifier())) + " "
                + (effect.isInfinite() ? -1 : Math.max(1, effect.getDuration())) + " "
                + (effect.hasParticles() ? 1 : 0) + " " + (effect.isAmbient() ? 1 : 0) + " "
                + (effect.hasIcon() ? 1 : 0) + " " + (force ? 1 : 0));
    }

    public boolean addPotionEffects(Collection<org.bukkit.potion.PotionEffect> effects) {
        boolean all = true;
        for (var e : effects) {
            all &= addPotionEffect(e);
        }
        return all;
    }

    public void removePotionEffect(org.bukkit.potion.PotionEffectType type) {
        var old = getPotionEffect(type);
        if (old == null || !online) {
            return;
        }
        var e = new org.bukkit.event.entity.EntityPotionEffectEvent(api, old, null, null,
                org.bukkit.event.entity.EntityPotionEffectEvent.Cause.PLUGIN,
                org.bukkit.event.entity.EntityPotionEffectEvent.Action.REMOVED, false);
        server.api.getPluginManager().callEvent(e);
        if (!e.isCancelled()) {
            control("uneffect " + type.getKey());
        }
    }

    public boolean clearActivePotionEffects() {
        var all = new ArrayList<>(state().effects());
        if (all.isEmpty() || !online) {
            return false;
        }
        var e = new org.bukkit.event.entity.EntityPotionEffectEvent(api, null, null, null,
                org.bukkit.event.entity.EntityPotionEffectEvent.Cause.PLUGIN,
                org.bukkit.event.entity.EntityPotionEffectEvent.Action.CLEARED, false);
        server.api.getPluginManager().callEvent(e);
        if (e.isCancelled()) {
            return false;
        }
        control("uneffect *");
        return true;
    }

    public int getFoodLevel() { return state().food(); }
    public void setFoodLevel(int food) { control("food " + food); }

    // --- опыт (`Player.giveExperiencePoints` и соседи — в Rust) ---
    public int getLevel() { return state().level(); }
    public void setLevel(int level) {
        if (level < 0) throw new IllegalArgumentException("Level cannot be negative");
        control("xp level " + level);
    }
    public float getExp() { return state().exp(); }
    public void setExp(float exp) {
        if (exp < 0 || exp > 1) throw new IllegalArgumentException("Experience progress must be between 0.0 and 1.0");
        control("xp progress " + exp);
    }
    public int getTotalExperience() { return state().totalExp(); }
    public void setTotalExperience(int exp) {
        if (exp < 0) throw new IllegalArgumentException("Total experience points must not be negative");
        control("xp total " + exp);
    }
    public void giveExp(int amount) { control("xp points " + amount); }
    public void giveExp(int amount, boolean applyMending) { giveExp(amount); }
    public int applyMending(int amount) { return amount; }
    public void giveExpLevels(int levels) { control("xp levels " + levels); }
    /** {@code getXpNeededForNextLevel}. */
    public int getExpToLevel() {
        int level = getLevel();
        return level >= 30 ? 112 + (level - 30) * 9 : level >= 15 ? 37 + (level - 15) * 5 : 7 + level * 2;
    }
    public int getExperiencePointsNeededForNextLevel() { return getExpToLevel(); }
    /** Как {@code CraftPlayer.calculateTotalExperiencePoints}: очки уровней плюс полоска. */
    public int calculateTotalExperiencePoints() {
        int level = getLevel();
        int points = level <= 16 ? level * level + 6 * level
                : level <= 31 ? (int) (2.5 * level * level - 40.5 * level + 360)
                : (int) (4.5 * level * level - 162.5 * level + 2220);
        return points + Math.round(getExp() * getExpToLevel());
    }
    public float getSaturation() { return state().saturation(); }
    public void setSaturation(float s) { control("saturation " + s); }
    public float getExhaustion() { return state().exhaustion(); }
    public void setExhaustion(float e) { control("exhaustion " + e); }
    public boolean isDead() { return getHealth() <= 0; }

    /** Присед из `player_input`: меняется сразу, без ожидания тика. */
    public boolean isSneaking() { return state().sneaking(); }

    /** Планировщик задач этого игрока: на Folia — поток региона, у нас общий. */
    private io.papermc.paper.threadedregions.scheduler.EntityScheduler scheduler;

    public io.papermc.paper.threadedregions.scheduler.EntityScheduler getScheduler() {
        if (scheduler == null) {
            scheduler = server.folia().entity(() -> !online);
        }
        return scheduler;
    }

    public org.bukkit.Location getLocation() {
        State s = state();
        return new org.bukkit.Location(server.world(s.dim()).api, s.x(), s.y(), s.z(), s.yaw(), s.pitch());
    }

    public org.bukkit.Location getEyeLocation() { return getLocation().add(0, 1.62, 0); }

    public org.bukkit.util.Vector getVelocity() { return velocity.clone(); }

    public void setVelocity(org.bukkit.util.Vector v) {
        if (v == null) {
            throw new IllegalArgumentException("Velocity cannot be null");
        }
        velocity = v.clone();
        control("velocity " + v.getX() + " " + v.getY() + " " + v.getZ());
    }

    private static String hideKey(Plugin plugin) {
        return plugin == null ? "*" : plugin.getName();
    }

    public void hideEntity(Plugin plugin, org.bukkit.entity.Entity entity) {
        if (entity == null) {
            return;
        }
        hidden.computeIfAbsent(entity.getUniqueId(),
                ignored -> java.util.concurrent.ConcurrentHashMap.newKeySet()).add(hideKey(plugin));
        if (entity instanceof Player) {
            playerEffect("hideplayer " + entity.getUniqueId());
        } else {
            playerEffect("hideentity " + entity.getEntityId());
        }
    }

    public void showEntity(Plugin plugin, org.bukkit.entity.Entity entity) {
        if (entity == null) {
            return;
        }
        Set<String> keys = hidden.get(entity.getUniqueId());
        if (keys != null) {
            keys.remove(hideKey(plugin));
            if (keys.isEmpty()) {
                hidden.remove(entity.getUniqueId(), keys);
            }
        }
        if (keys != null && !keys.isEmpty()) {
            return;
        }
        if (entity instanceof Player) {
            playerEffect("showplayer " + entity.getUniqueId());
        } else {
            playerEffect("showentity " + entity.getEntityId());
        }
    }

    public void hidePlayer(Player player) { hideEntity(null, player); }
    public void hidePlayer(Plugin plugin, Player player) { hideEntity(plugin, player); }
    public void showPlayer(Plugin plugin, Player player) { showEntity(plugin, player); }
    public void showPlayer(Player player) { showEntity(null, player); }

    public boolean canSee(org.bukkit.entity.Entity entity) {
        if (entity == null || (entity instanceof Player p && !p.isOnline())) {
            return false;
        }
        Set<String> keys = hidden.get(entity.getUniqueId());
        return keys == null || keys.isEmpty();
    }

    public boolean canSee(Player player) { return canSee((org.bukkit.entity.Entity) player); }

    public boolean getAllowFlight() { return allowFlight; }

    public void setAllowFlight(boolean value) {
        allowFlight = value;
        if (!value) {
            flying = false;
        }
        control("allowflight " + (value ? 1 : 0));
    }

    public boolean isFlying() { return flying; }

    public void setFlying(boolean value) {
        flying = value;
        control("flying " + (value ? 1 : 0));
    }

    public java.util.List<org.bukkit.entity.Entity> getNearbyEntities(double x, double y, double z) {
        java.util.List<org.bukkit.entity.Entity> out = new java.util.ArrayList<>();
        for (org.bukkit.entity.Entity e : getWorld().getNearbyEntities(getLocation(), x, y, z)) {
            if (e != api && !e.equals(api)) {
                out.add(e);
            }
        }
        return out;
    }

    /** Упрощённый лучевой поиск блока: шаг 0.2 блока, без тонких форм. */
    public org.bukkit.block.Block getTargetBlock(java.util.Set<Material> transparent, int maxDistance) {
        org.bukkit.Location at = getEyeLocation();
        org.bukkit.util.Vector step = at.getDirection().normalize().multiply(0.2);
        int steps = Math.max(1, maxDistance * 5);
        for (int i = 0; i < steps; i++) {
            at.add(step);
            org.bukkit.block.Block block = at.getBlock();
            Material type = block.getType();
            if (type.isAir() || (transparent != null && transparent.contains(type))) {
                continue;
            }
            return block;
        }
        return null;
    }

    public java.util.concurrent.CompletableFuture<Boolean> teleportAsync(
            org.bukkit.Location to, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause cause) {
        return java.util.concurrent.CompletableFuture.completedFuture(teleport(to, cause));
    }

    public boolean performCommand(String command) {
        return command != null && server.dispatchCommand(api, command);
    }

    public boolean teleport(org.bukkit.Location to) {
        return teleport(to, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause.PLUGIN);
    }

    public boolean teleport(org.bukkit.entity.Entity target) { return teleport(target.getLocation()); }

    /** Как {@code CraftPlayer.teleport}: событие, отмена — остаёмся на месте. */
    public boolean teleport(org.bukkit.Location to, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause cause) {
        if (!online || isDead()) {
            return false;
        }
        var e = new org.bukkit.event.player.PlayerTeleportEvent(api, getLocation(), to.clone(), cause);
        server.api.getPluginManager().callEvent(e);
        if (e.isCancelled() || e.getTo() == null) {
            return false;
        }
        org.bukkit.Location t = e.getTo();
        // Мир цели последним словом: другой мир — переход измерения в Rust.
        WorldImpl target = t.getWorld() != null ? WorldImpl.of(t.getWorld()) : null;
        String dim = target != null ? target.dim : state().dim();
        control("tp " + t.getX() + " " + t.getY() + " " + t.getZ() + " " + t.getYaw() + " " + t.getPitch() + " " + dim);
        return true;
    }

    @SuppressWarnings("deprecation")
    public GameMode getGameMode() { return GameMode.getByValue(state().gameMode()); }

    /** Как {@code CraftPlayer.setGameMode}: событие с причиной PLUGIN, отмена — без смены. */
    @SuppressWarnings("deprecation")
    public void setGameMode(GameMode mode) {
        if (mode == null) {
            throw new IllegalArgumentException("GameMode cannot be null");
        }
        if (mode == getGameMode()) {
            return;
        }
        PlayerGameModeChangeEvent e = new PlayerGameModeChangeEvent(api, mode,
                PlayerGameModeChangeEvent.Cause.PLUGIN, null);
        server.api.getPluginManager().callEvent(e);
        if (!e.isCancelled()) {
            control("mode " + mode.getValue());
        }
    }

    @Override
    public String toString() {
        return "ChunkwakePlayer{name=" + name + "}";
    }
}
