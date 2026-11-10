package dev.chunkwake.bridge;

import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.logging.Level;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.HandlerList;
import org.bukkit.event.Listener;
import org.bukkit.permissions.Permissible;
import org.bukkit.permissions.Permission;
import org.bukkit.plugin.EventExecutor;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.PluginManager;
import org.bukkit.plugin.RegisteredListener;
import org.bukkit.plugin.java.JavaPlugin;

/** {@link PluginManager}: плагины, события, права (минимум). */
final class PluginManagerImpl {
    final PluginManager api = Stub.of(PluginManager.class, this);
    private final ChunkwakeServer server;
    final Map<String, Plugin> plugins = new LinkedHashMap<>();
    private final Map<String, Permission> permissions = new HashMap<>();

    PluginManagerImpl(ChunkwakeServer server) {
        this.server = server;
    }

    public Plugin getPlugin(String name) {
        return plugins.get(name);
    }

    public Plugin[] getPlugins() {
        return plugins.values().toArray(new Plugin[0]);
    }

    public boolean isPluginEnabled(String name) {
        Plugin p = plugins.get(name);
        return p != null && p.isEnabled();
    }

    public boolean isPluginEnabled(Plugin p) {
        return p != null && p.isEnabled();
    }

    public void enablePlugin(Plugin p) {
        if (p.isEnabled()) {
            return;
        }
        p.getLogger().info("Enabling " + p.getPluginMeta().getDisplayName());
        try {
            setEnabled(p, true);
        } catch (Throwable t) {
            if (!Internals.report(server.logger, p.getName(), "enabling", t)) {
                server.logger.log(Level.SEVERE, "Error occurred while enabling " + p.getName(), t);
            }
            setEnabledQuietly(p, false);
        }
        updateListenerMask();
    }

    public void disablePlugin(Plugin p) {
        if (!p.isEnabled()) {
            return;
        }
        p.getLogger().info("Disabling " + p.getPluginMeta().getDisplayName());
        setEnabledQuietly(p, false);
        server.scheduler().cancelTasks(p);
        server.folia().cancelTasks(p);
        server.metadata().removeAll(p);
        HandlerList.unregisterAll(p);
        server.pluginDisabled(p);
        updateListenerMask();
    }

    public void disablePlugins() {
        List<Plugin> list = new ArrayList<>(plugins.values());
        java.util.Collections.reverse(list);
        list.forEach(this::disablePlugin);
    }

    private static void setEnabled(Plugin p, boolean on) throws ReflectiveOperationException {
        Method m = JavaPlugin.class.getDeclaredMethod("setEnabled", boolean.class);
        m.setAccessible(true);
        try {
            m.invoke(p, on);
        } catch (java.lang.reflect.InvocationTargetException e) {
            throw new IllegalStateException(e.getCause());
        }
    }

    private void setEnabledQuietly(Plugin p, boolean on) {
        try {
            setEnabled(p, on);
        } catch (Throwable t) {
            server.logger.log(Level.SEVERE, "Error occurred while disabling " + p.getName(), t);
        }
    }

    // --- события ---

    public void callEvent(Event event) {
        for (RegisteredListener rl : event.getHandlers().getRegisteredListeners()) {
            if (!rl.getPlugin().isEnabled()) {
                continue;
            }
            try {
                rl.callEvent(event);
            } catch (Throwable t) {
                if (!Internals.report(server.logger, rl.getPlugin().getName(), event.getEventName(), t)) {
                    server.logger.log(Level.SEVERE, "Could not pass event " + event.getEventName()
                            + " to " + rl.getPlugin().getName(), t);
                }
            }
        }
    }

    public void registerEvents(Listener listener, Plugin plugin) {
        Set<Method> methods = new java.util.LinkedHashSet<>(List.of(listener.getClass().getMethods()));
        methods.addAll(List.of(listener.getClass().getDeclaredMethods()));
        for (Method m : methods) {
            EventHandler eh = m.getAnnotation(EventHandler.class);
            if (eh == null || m.isBridge() || m.isSynthetic() || m.getParameterCount() != 1
                    || !Event.class.isAssignableFrom(m.getParameterTypes()[0])) {
                continue;
            }
            Class<? extends Event> type = m.getParameterTypes()[0].asSubclass(Event.class);
            m.setAccessible(true);
            registerEvent(type, listener, eh.priority(), EventExecutor.create(m, type), plugin, eh.ignoreCancelled());
        }
        updateListenerMask();
    }

    public void registerEvent(Class<? extends Event> type, Listener l, EventPriority pr, EventExecutor ex, Plugin p) {
        registerEvent(type, l, pr, ex, p, false);
    }

    public void registerEvent(Class<? extends Event> type, Listener l, EventPriority pr, EventExecutor ex,
            Plugin p, boolean ignoreCancelled) {
        handlers(type).register(new RegisteredListener(l, ex, pr, p, ignoreCancelled));
        updateListenerMask();
    }

    /** `getHandlerList()` события или ближайшего предка. */
    static HandlerList handlers(Class<?> type) {
        for (Class<?> c = type; c != null && Event.class.isAssignableFrom(c); c = c.getSuperclass()) {
            try {
                Method m = c.getDeclaredMethod("getHandlerList");
                m.setAccessible(true);
                return (HandlerList) m.invoke(null);
            } catch (NoSuchMethodException e) {
                // выше по иерархии
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }
        throw new IllegalArgumentException("Unable to find handler list for event " + type.getName());
    }

    /** Какие события Rust вообще стоит присылать. */
    void updateListenerMask() {
        long mask = 0;
        if (has(org.bukkit.event.player.PlayerJoinEvent.getHandlerList())) mask |= Bridge.LISTEN_JOIN;
        if (has(org.bukkit.event.player.PlayerQuitEvent.getHandlerList())) mask |= Bridge.LISTEN_QUIT;
        if (has(org.bukkit.event.block.BlockBreakEvent.getHandlerList())) mask |= Bridge.LISTEN_BLOCK_BREAK;
        if (has(org.bukkit.event.entity.EntityDamageEvent.getHandlerList())) mask |= Bridge.listenBit(Bridge.EVENT_DAMAGE);
        if (has(org.bukkit.event.entity.EntityDeathEvent.getHandlerList())) mask |= Bridge.listenBit(Bridge.EVENT_DEATH);
        if (has(org.bukkit.event.player.PlayerDropItemEvent.getHandlerList())) mask |= Bridge.listenBit(Bridge.EVENT_DROP);
        if (has(org.bukkit.event.entity.EntityPickupItemEvent.getHandlerList())) mask |= Bridge.listenBit(Bridge.EVENT_PICKUP);
        if (has(org.bukkit.event.player.PlayerGameModeChangeEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_GAME_MODE);
        }
        if (has(io.papermc.paper.event.player.AsyncChatEvent.getHandlerList())
                || has(org.bukkit.event.player.AsyncPlayerChatEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_CHAT);
        }
        if (has(org.bukkit.event.player.PlayerCommandPreprocessEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_COMMAND_PRE);
        }
        // Таб-комплит команд плагинов нужен всегда (TabCompleter, а не слушатель).
        mask |= Bridge.listenBit(Bridge.EVENT_TAB_COMPLETE);
        // Каналы плагинов: `minecraft:register` нужен всегда, иначе игрок
        // не «слушает» ни один канал и `sendPluginMessage` молчит.
        mask |= Bridge.listenBit(Bridge.EVENT_PLUGIN_MESSAGE);
        if (has(org.bukkit.event.block.BlockPlaceEvent.getHandlerList())) mask |= Bridge.listenBit(Bridge.EVENT_PLACE);
        if (has(org.bukkit.event.player.PlayerMoveEvent.getHandlerList())) mask |= Bridge.listenBit(Bridge.EVENT_MOVE);
        if (has(org.bukkit.event.player.PlayerInteractEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_INTERACT);
        }
        if (has(org.bukkit.event.entity.EntityPotionEffectEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_POTION);
        }
        if (has(org.bukkit.event.enchantment.PrepareItemEnchantEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_PREPARE_ENCHANT);
        }
        if (has(org.bukkit.event.enchantment.EnchantItemEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_ENCHANT);
        }
        if (has(com.destroystokyo.paper.event.player.PlayerPickupExperienceEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_PICKUP_XP);
        }
        if (has(org.bukkit.event.player.PlayerItemMendEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_MEND);
        }
        if (has(org.bukkit.event.player.PlayerExpChangeEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_EXP_CHANGE);
        }
        if (has(org.bukkit.event.block.BlockRedstoneEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_REDSTONE);
        }
        if (has(org.bukkit.event.block.BlockPistonExtendEvent.getHandlerList())
                || has(org.bukkit.event.block.BlockPistonRetractEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_PISTON);
        }
        if (has(org.bukkit.event.inventory.InventoryMoveItemEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_MOVE_ITEM);
        }
        if (has(org.bukkit.event.inventory.InventoryPickupItemEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_HOPPER_PICKUP);
        }
        if (has(org.bukkit.event.block.BlockDispenseEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_DISPENSE);
        }
        if (has(org.bukkit.event.player.PlayerBucketEmptyEvent.getHandlerList())
                || has(org.bukkit.event.player.PlayerBucketFillEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_BUCKET);
        }
        if (has(org.bukkit.event.player.PlayerPickupArrowEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_PICKUP_ARROW);
        }
        if (has(org.bukkit.event.inventory.InventoryClickEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_INVENTORY_CLICK);
        }
        // Закрытие окна плагина нужно и мосту: по нему сбрасывается открытый вид.
        mask |= Bridge.listenBit(Bridge.EVENT_INVENTORY_CLOSE);
        if (has(org.bukkit.event.entity.EntityShootBowEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_SHOOT_BOW);
        }
        if (has(org.bukkit.event.entity.EntityDamageByEntityEvent.getHandlerList())
                || has(org.bukkit.event.entity.EntityDamageEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_MOB_DAMAGE);
        }
        if (has(org.bukkit.event.entity.EntityDeathEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_MOB_DEATH);
        }
        if (has(org.bukkit.event.player.PlayerInteractEntityEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_INTERACT_ENTITY);
        }
        if (has(org.bukkit.event.entity.CreatureSpawnEvent.getHandlerList())
                || has(org.bukkit.event.entity.EntitySpawnEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_CREATURE_SPAWN);
        }
        if (has(org.bukkit.event.entity.EntityBreedEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_BREED);
        }
        if (has(org.bukkit.event.player.PlayerShearEntityEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_SHEAR);
        }
        if (has(org.bukkit.event.block.CampfireStartEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_CAMPFIRE_START);
        }
        if (has(org.bukkit.event.block.BlockCookEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_COOK);
        }
        if (has(org.bukkit.event.inventory.PrepareGrindstoneEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_PREPARE_GRINDSTONE);
        }
        if (has(org.bukkit.event.player.AsyncPlayerPreLoginEvent.getHandlerList())
                || has(org.bukkit.event.player.PlayerPreLoginEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_PRE_LOGIN);
        }
        // Вход ждём всегда: игрок для `join` создаётся здесь, с профилем и адресом.
        mask |= Bridge.listenBit(Bridge.EVENT_LOGIN);
        if (has(org.bukkit.event.player.PlayerKickEvent.getHandlerList())
                || has(org.bukkit.event.player.PlayerQuitEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_KICK);
        }
        if (has(org.bukkit.event.player.PlayerRespawnEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_RESPAWN);
        }
        if (has(org.bukkit.event.player.PlayerTeleportEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_TELEPORT);
        }
        if (has(org.bukkit.event.player.PlayerItemHeldEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_ITEM_HELD);
        }
        if (has(org.bukkit.event.player.PlayerToggleSneakEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_TOGGLE_SNEAK);
        }
        if (has(org.bukkit.event.player.PlayerToggleSprintEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_TOGGLE_SPRINT);
        }
        if (has(org.bukkit.event.player.PlayerChangedWorldEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_CHANGED_WORLD);
        }
        if (has(io.papermc.paper.event.packet.UncheckedSignChangeEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_SIGN_UNCHECKED);
        }
        if (has(org.bukkit.event.block.SignChangeEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_SIGN_CHANGE);
        }
        if (has(io.papermc.paper.event.player.PlayerOpenSignEvent.getHandlerList())
                || has(org.bukkit.event.player.PlayerSignOpenEvent.getHandlerList())) {
            mask |= Bridge.listenBit(Bridge.EVENT_OPEN_SIGN);
        }
        Bridge.setFlag(Bridge.FLAG_LISTENERS, mask);
    }

    private static boolean has(HandlerList h) {
        return h.getRegisteredListeners().length > 0;
    }

    static boolean cancelled(Event e) {
        return e instanceof Cancellable c && c.isCancelled();
    }

    // --- права: реестр как у SimplePluginManager, умолчания считаются по запросу ---

    public Permission getPermission(String name) {
        return permissions.get(name.toLowerCase(java.util.Locale.ROOT));
    }

    public void addPermission(Permission perm) {
        if (getPermission(perm.getName()) != null) {
            throw new IllegalArgumentException("The permission " + perm.getName() + " is already defined!");
        }
        permissions.put(perm.getName().toLowerCase(java.util.Locale.ROOT), perm);
    }

    public void removePermission(Permission perm) {
        removePermission(perm.getName());
    }

    public void removePermission(String name) {
        permissions.remove(name.toLowerCase(java.util.Locale.ROOT));
    }

    /** `/reload`: права плагинов регистрируются заново. */
    void clearPermissions() {
        permissions.clear();
    }

    public Set<Permission> getPermissions() {
        return Set.copyOf(permissions.values());
    }

    public Set<Permission> getDefaultPermissions(boolean op) {
        Set<Permission> out = new java.util.HashSet<>();
        for (Permission p : permissions.values()) {
            if (p.getDefault().getValue(op)) {
                out.add(p);
            }
        }
        return out;
    }

    public void recalculatePermissionDefaults(Permission perm) {}

    public void subscribeToPermission(String perm, Permissible p) {}

    public void unsubscribeFromPermission(String perm, Permissible p) {}

    public Set<Permissible> getPermissionSubscriptions(String perm) {
        return Set.of();
    }

    public void subscribeToDefaultPerms(boolean op, Permissible p) {}

    public void unsubscribeFromDefaultPerms(boolean op, Permissible p) {}

    public Set<Permissible> getDefaultPermSubscriptions(boolean op) {
        return Set.of();
    }

    public boolean useTimings() {
        return false;
    }
}
