package dev.chunkwake.bridge;

import java.util.UUID;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;
import org.bukkit.entity.LightningStrike;
import org.bukkit.entity.Player;

/**
 * {@link LightningStrike} из {@code World.strikeLightning*}.
 * Java-обёртка живёт столько же, сколько вызов; Rust-сущность исчезает сама
 * через два тика, поэтому {@link #remove()} только помечает объект мёртвым.
 */
final class LightningStrikeImpl {
    final WorldImpl world;
    final UUID uuid = UUID.randomUUID();
    final Entity api;
    final boolean effect;
    private final Location location;
    volatile int id = -1;
    private volatile boolean removed;
    private int flashes;
    private int lifeTicks;
    private Player causingPlayer;

    LightningStrikeImpl(WorldImpl world, Location location, boolean effect) {
        this.world = world;
        this.location = location.clone();
        this.effect = effect;
        this.api = (LightningStrike) Stub.ofAll(
                new Class<?>[] {LightningStrike.class, Entity.class}, this);
    }

    public UUID getUniqueId() { return uuid; }

    public Location getLocation() { return location.clone(); }

    public Location getLocation(Location into) {
        if (into != null) {
            into.setWorld(location.getWorld());
            into.setX(location.getX());
            into.setY(location.getY());
            into.setZ(location.getZ());
            into.setYaw(location.getYaw());
            into.setPitch(location.getPitch());
        }
        return into;
    }

    public World getWorld() { return world.api; }

    public EntityType getType() { return EntityType.LIGHTNING_BOLT; }

    public int getEntityId() { return id; }

    public boolean isValid() { return !removed; }

    public boolean isDead() { return removed; }

    public void remove() { removed = true; }

    public boolean isEffect() { return effect; }

    public int getFlashes() { return flashes; }

    public void setFlashes(int flashes) { this.flashes = flashes; }

    public int getFlashCount() { return flashes; }

    public void setFlashCount(int flashes) { this.flashes = flashes; }

    public int getLifeTicks() { return lifeTicks; }

    public void setLifeTicks(int lifeTicks) { this.lifeTicks = lifeTicks; }

    public Player getCausingPlayer() { return causingPlayer; }

    public void setCausingPlayer(Player causingPlayer) { this.causingPlayer = causingPlayer; }

    public Entity getCausingEntity() { return causingPlayer; }

    public String getName() { return "LIGHTNING_BOLT"; }

    @Override
    public String toString() { return "CraftLightningStrike{id=" + id + "}"; }
}
