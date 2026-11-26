package dev.chunkwake.bridge;

import java.util.UUID;
import java.util.concurrent.ThreadLocalRandom;
import org.bukkit.Location;
import org.bukkit.entity.AbstractArrow;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;
import org.bukkit.entity.SpectralArrow;
import org.bukkit.entity.TippedArrow;
import org.bukkit.entity.Trident;
import org.bukkit.util.Vector;

/**
 * {@code World.spawnArrow}: стрела/трезубец с начальной скоростью
 * {@code direction * speed} и ванильным разбросом
 * {@code 0.007499999832361937 * spread} на компонент.
 */
final class ArrowSpawn {
    private ArrowSpawn() {
    }

    @SuppressWarnings("unchecked")
    static <T extends AbstractArrow> T spawn(WorldImpl world, Location location, Vector direction,
            float speed, float spread, Class<T> clazz) {
        if (location == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        if (direction == null) {
            throw new IllegalArgumentException("Vector cannot be null");
        }
        if (clazz == null) {
            throw new IllegalArgumentException("clazz Entity for the arrow cannot be null");
        }
        EntityType type;
        if (Trident.class.isAssignableFrom(clazz)) {
            type = EntityType.TRIDENT;
        } else if (SpectralArrow.class.isAssignableFrom(clazz)
                || TippedArrow.class.isAssignableFrom(clazz)) {
            throw new UnsupportedOperationException(
                    "chunkwake: spawnArrow " + clazz.getSimpleName());
        } else {
            type = EntityType.ARROW;
        }
        Vector velocity = direction.clone();
        if (velocity.lengthSquared() != 0.0) {
            velocity.normalize().multiply(speed);
            double factor = 0.007499999832361937D * spread;
            ThreadLocalRandom random = ThreadLocalRandom.current();
            velocity.add(new Vector(
                    random.nextGaussian() * factor,
                    random.nextGaussian() * factor,
                    random.nextGaussian() * factor));
        }
        String kind = type == EntityType.TRIDENT ? "minecraft:trident" : "minecraft:arrow";
        String reply = world.request("entity spawnarrow " + kind + " "
                + location.getX() + " " + location.getY() + " " + location.getZ() + " "
                + velocity.getX() + " " + velocity.getY() + " " + velocity.getZ()).trim();
        if (reply.isEmpty()) {
            throw new IllegalStateException("chunkwake: spawnArrow " + kind + " failed");
        }
        String[] f = reply.split(" ");
        if (f.length < 2) {
            throw new IllegalStateException("chunkwake: spawnArrow " + kind + " reply: " + reply);
        }
        UUID uuid;
        try {
            uuid = UUID.fromString(f[1]);
        } catch (IllegalArgumentException e) {
            throw new IllegalStateException("chunkwake: spawnArrow " + kind + " uuid: " + f[1], e);
        }
        Entity entity = NonMobImpl.wrap(world, Integer.parseInt(f[0]), kind, uuid, type);
        return (T) entity;
    }
}
