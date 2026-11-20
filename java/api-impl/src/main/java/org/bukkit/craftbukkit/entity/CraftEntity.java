package org.bukkit.craftbukkit.entity;

/**
 * CraftBukkit-база сущности.
 *
 * <p>Важно для бинарной совместимости: плагины (Emotecraft) вызывают
 * {@code CraftEntity.getHandle()} с дескриптором
 * {@code ()Lnet/minecraft/world/entity/Entity;}. Ковариантные переопределения
 * в наследниках получают сгенерированный компилятором bridge-метод с этим
 * дескриптором, поэтому базовый класс обязан объявлять именно его.
 */
public class CraftEntity {
    protected final Object handle;
    /**
     * Алиас, который CraftEngine (CraftEntityProxy) находит по имени
     * {@code entity}; тип обязан быть NMS Entity.
     */
    protected final net.minecraft.world.entity.Entity entity;

    public CraftEntity(Object handle) {
        this.handle = handle;
        this.entity = handle instanceof net.minecraft.world.entity.Entity e ? e : null;
    }

    /**
     * Метод, который CraftEngine (CraftEntityProxy) ищет именно у
     * CraftEntity; подклассы переопределяют его настоящей реализацией.
     */
    public org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        return null;
    }

    public net.minecraft.world.entity.Entity getHandle() {
        return this.handle instanceof net.minecraft.world.entity.Entity entity ? entity : null;
    }
}
