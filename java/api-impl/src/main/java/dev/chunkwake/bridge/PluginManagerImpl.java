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
            server.logger.log(Level.SEVERE, "Error occurred while enabling " + p.getName(), t);
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
        HandlerList.unregisterAll(p);
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
                server.logger.log(Level.SEVERE, "Could not pass event " + event.getEventName()
                        + " to " + rl.getPlugin().getName(), t);
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
