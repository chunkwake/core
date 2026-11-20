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

    public CraftEntity(Object handle) {
        this.handle = handle;
    }

    public net.minecraft.world.entity.Entity getHandle() {
        return this.handle instanceof net.minecraft.world.entity.Entity entity ? entity : null;
    }
}
