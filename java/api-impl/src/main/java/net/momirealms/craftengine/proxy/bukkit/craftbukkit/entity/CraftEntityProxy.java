package net.momirealms.craftengine.proxy.bukkit.craftbukkit.entity;

/**
 * Compatibility shim for CraftEngine's reflection proxy. CraftEngine normally
 * generates this interface implementation with ASM and expects a concrete
 * {@code org.bukkit.craftbukkit.entity.CraftEntity} class with an {@code entity}
 * field. Chunkwake exposes Bukkit entities through an interface instead, so the
 * generated field getter cannot bind. This implementation keeps the same public
 * contract and dispatches through the Bukkit/NMS bridge.
 */
public interface CraftEntityProxy {
    CraftEntityProxy INSTANCE = new CraftEntityProxy() {
        @Override
        public Object getEntity(org.bukkit.entity.Entity entity) {
            if (entity instanceof org.bukkit.craftbukkit.entity.CraftEntity craft) {
                Object handle = craft.getHandle();
                if (handle != null) {
                    return handle;
                }
            }
            try {
                java.lang.reflect.Field field = entity.getClass().getDeclaredField("handle");
                field.setAccessible(true);
                return field.get(entity);
            } catch (ReflectiveOperationException ignored) {
                return null;
            }
        }

        @Override
        public void setEntity(org.bukkit.entity.Entity entity, Object value) {
            if (entity instanceof org.bukkit.craftbukkit.entity.CraftEntity craft) {
                craft.setHandle(value);
                return;
            }
            try {
                java.lang.reflect.Field field = entity.getClass().getDeclaredField("handle");
                field.setAccessible(true);
                field.set(entity, value);
            } catch (ReflectiveOperationException ignored) {
                // Bukkit entity implementation without a mutable handle.
            }
        }

        @Override
        public Object getPersistentDataContainer(org.bukkit.entity.Entity entity) {
            return entity.getPersistentDataContainer();
        }
    };

    Object getEntity(org.bukkit.entity.Entity entity);

    void setEntity(org.bukkit.entity.Entity entity, Object value);

    Object getPersistentDataContainer(org.bukkit.entity.Entity entity);
}
