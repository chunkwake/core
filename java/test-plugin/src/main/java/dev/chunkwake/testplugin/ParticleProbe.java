package dev.chunkwake.testplugin;

import java.util.List;
import org.bukkit.Color;
import org.bukkit.Location;
import org.bukkit.Particle;
import org.bukkit.World;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.plugin.java.JavaPlugin;

/**
 * Живая проба {@code World.spawnParticle}/{@code Player.spawnParticle}:
 * простая FLAME, DUST с {@link Particle.DustOptions}, вариант с явным
 * списком получателей и частица только для игрока.
 * Включается {@code CHUNKWAKE_PARTICLE_PROBE}.
 */
final class ParticleProbe implements Listener {
    private final JavaPlugin plugin;

    ParticleProbe(JavaPlugin plugin) {
        this.plugin = plugin;
    }

    @EventHandler
    public void onJoin(PlayerJoinEvent event) {
        Player player = event.getPlayer();
        plugin.getServer().getScheduler().runTaskLater(plugin, () -> run(player), 40L);
    }

    private void run(Player player) {
        try {
            World world = player.getWorld();
            Location at = player.getLocation().add(0, 1, 0);

            world.spawnParticle(Particle.FLAME, at, 12, 0.25, 0.5, 0.25, 0.01);
            world.spawnParticle(Particle.DUST, at.clone().add(1, 0, 0), 8,
                    0.2, 0.2, 0.2, 0.0, new Particle.DustOptions(Color.RED, 1.5F));
            world.spawnParticle(Particle.FLAME, List.of(player), player,
                    at.getX(), at.getY(), at.getZ(), 6,
                    0.1, 0.1, 0.1, 0.0, null, false);
            player.spawnParticle(Particle.FLAME, at, 4, 0.1, 0.1, 0.1, 0.0);

            plugin.getLogger().info("PARTICLE expected level_particles=4"
                    + " (world-flame=1, world-dust=1, receivers=1, player=1)");
        } catch (Throwable t) {
            plugin.getLogger().log(java.util.logging.Level.WARNING, "PARTICLE error", t);
        }
    }
}
