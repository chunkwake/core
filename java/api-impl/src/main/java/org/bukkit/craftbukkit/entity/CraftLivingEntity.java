package org.bukkit.craftbukkit.entity;

import java.util.LinkedHashSet;
import java.util.Set;
import org.bukkit.Bukkit;
import org.bukkit.entity.Player;

/**
 * CraftBukkit-представление живой сущности.
 *
 * <p>Emotecraft берёт {@code Avatar.getBukkitEntity()} и спрашивает
 * {@code getTrackedBy()}, чтобы разослать эмоцию наблюдателям. Точное
 * отслеживание у нас не моделируется, поэтому возвращаем всех онлайн-игроков:
 * это ванильное поведение «видят все, кто в мире».
 */
public class CraftLivingEntity extends CraftHumanEntity {
    public CraftLivingEntity(Object handle) {
        super(handle);
    }

    public CraftLivingEntity(org.bukkit.craftbukkit.CraftServer server,
            net.minecraft.server.level.ServerPlayer entity) {
        super(server, entity);
    }

    /** CraftBukkit: у CraftLivingEntity getHandle() объявлен здесь. */
    @Override
    public net.minecraft.world.entity.LivingEntity getHandle() {
        net.minecraft.world.entity.Entity handle = super.getHandle();
        return handle instanceof net.minecraft.world.entity.LivingEntity living ? living : null;
    }

    @Override
    public Set<Player> getTrackedBy() {
        return new LinkedHashSet<>(Bukkit.getOnlinePlayers());
    }
}
