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

    @Override
    public Set<Player> getTrackedBy() {
        return new LinkedHashSet<>(Bukkit.getOnlinePlayers());
    }
}
