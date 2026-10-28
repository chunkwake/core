package dev.chunkwake.bridge;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.UUID;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.ComponentLike;
import net.kyori.adventure.text.serializer.gson.GsonComponentSerializer;
import org.bukkit.GameMode;
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
final class PlayerImpl {
    final Player api = Stub.of(Player.class, this);
    private final ChunkwakeServer server;
    final UUID id;
    final String name;
    volatile boolean online = true;
    /** Права как у CraftBukkit: {@code PermissibleBase} над {@link #isOp()}. */
    private PermissibleBase perms;
    private boolean permsOp;

    PlayerImpl(ChunkwakeServer server, UUID id, String name) {
        this.server = server;
        this.id = id;
        this.name = name;
    }

    public String getName() { return name; }
    public UUID getUniqueId() { return id; }
    public Server getServer() { return server.api; }
    public World getWorld() { return server.world; }
    public boolean isOnline() { return online; }
    public boolean isConnected() { return online; }
    public boolean isOp() { return Bridge.opLevel(id) > 0; }
    public void setOp(boolean op) {
        Bridge.setOp(id, op);
        recalculatePermissions();
    }

    /** Базовые права; при смене op (в т.ч. {@code /op} из Rust) пересчитываются. */
    private PermissibleBase perms() {
        boolean op = isOp();
        if (perms == null) {
            permsOp = op;
            perms = new PermissibleBase(api);
        } else if (op != permsOp) {
            permsOp = op;
            perms.recalculatePermissions();
        }
        return perms;
    }

    public boolean hasPermission(String p) { return perms().hasPermission(p); }
    public boolean hasPermission(Permission p) { return perms().hasPermission(p); }
    public boolean isPermissionSet(String p) { return perms().isPermissionSet(p); }
    public boolean isPermissionSet(Permission p) { return perms().isPermissionSet(p); }
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
        if (perms != null) {
            permsOp = isOp();
            perms.recalculatePermissions();
        }
    }
    public Set<PermissionAttachmentInfo> getEffectivePermissions() { return perms().getEffectivePermissions(); }
    public Component name() { return Component.text(name); }
    public Component displayName() { return Component.text(name); }
    public String getDisplayName() { return name; }

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

    // --- состояние из Rust-сессии ---

    /** Снимок игрока: здоровье, голод, режим, инвентарь в индексах Bukkit. */
    record State(double health, int food, float saturation, float exhaustion, int gameMode,
            int selected, double x, double y, double z, float yaw, float pitch, ItemStack[] items,
            List<org.bukkit.potion.PotionEffect> effects) {
        static State parse(String text) {
            String[] lines = text.split("\n", -1);
            List<org.bukkit.potion.PotionEffect> effects = new ArrayList<>();
            for (int i = 1 + InventoryImpl.SIZE; i < lines.length; i++) {
                String[] e = lines[i].split(" ");
                if (e.length == 5 && e[0].equals("e")) {
                    var type = org.bukkit.Registry.EFFECT.get(org.bukkit.NamespacedKey.fromString(e[1]));
                    if (type != null) {
                        effects.add(new org.bukkit.potion.PotionEffect(type, Integer.parseInt(e[3]),
                                Integer.parseInt(e[2]), false, e[4].equals("1"), true));
                    }
                }
            }
            String[] h = lines[0].split(" ");
            ItemStack[] items = new ItemStack[InventoryImpl.SIZE];
            for (int i = 0; i < items.length; i++) {
                String l = i + 1 < lines.length ? lines[i + 1] : "-";
                int sp = l.indexOf(' ');
                items[i] = sp < 0 ? new StackImpl(org.bukkit.Material.AIR, 0)
                        : StackImpl.of(l.substring(0, sp), Integer.parseInt(l.substring(sp + 1)));
            }
            return new State(Double.parseDouble(h[0]), Integer.parseInt(h[1]), Float.parseFloat(h[2]),
                    Float.parseFloat(h[3]), Integer.parseInt(h[4]), Integer.parseInt(h[5]),
                    Double.parseDouble(h[6]), Double.parseDouble(h[7]), Double.parseDouble(h[8]),
                    Float.parseFloat(h[9]), Float.parseFloat(h[10]), items, effects);
        }
    }

    private State last;
    private final InventoryImpl inventory = new InventoryImpl(this);

    /** Текущий снимок; после выхода — последний известный. */
    State state() {
        String text = online ? Bridge.playerState(id) : null;
        if (text != null) {
            last = State.parse(text);
        }
        if (last == null) {
            throw new IllegalStateException("chunkwake: no state for " + name);
        }
        return last;
    }

    void control(String line) {
        if (online) {
            Bridge.playerControl(id, line);
        }
    }

    public PlayerInventory getInventory() { return inventory.api; }
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
        if (!online || isDead()) {
            return false;
        }
        var old = getPotionEffect(effect.getType());
        // `MobEffectInstance.update`: слабее или короче — не применяется.
        boolean longer = old != null && !old.isInfinite() && (effect.isInfinite() || effect.getDuration() > old.getDuration());
        if (old != null && !(effect.getAmplifier() > old.getAmplifier() || (effect.getAmplifier() == old.getAmplifier() && longer))) {
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
                + (effect.isInfinite() ? -1 : Math.max(1, effect.getDuration())) + " " + (effect.hasParticles() ? 1 : 0));
    }

    public boolean addPotionEffect(org.bukkit.potion.PotionEffect effect, boolean force) { return addPotionEffect(effect); }

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
        for (var e : all) {
            removePotionEffect(e.getType());
        }
        return !all.isEmpty();
    }

    public int getFoodLevel() { return state().food(); }
    public void setFoodLevel(int food) { control("food " + food); }
    public float getSaturation() { return state().saturation(); }
    public void setSaturation(float s) { control("saturation " + s); }
    public float getExhaustion() { return state().exhaustion(); }
    public void setExhaustion(float e) { control("exhaustion " + e); }
    public boolean isDead() { return getHealth() <= 0; }

    public org.bukkit.Location getLocation() {
        State s = state();
        return new org.bukkit.Location(server.world, s.x(), s.y(), s.z(), s.yaw(), s.pitch());
    }

    public org.bukkit.Location getEyeLocation() { return getLocation().add(0, 1.62, 0); }

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
        control("tp " + t.getX() + " " + t.getY() + " " + t.getZ() + " " + t.getYaw() + " " + t.getPitch());
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
