package dev.chunkwake.testplugin;

import org.bukkit.Location;
import org.bukkit.Sound;
import org.bukkit.SoundCategory;
import org.bukkit.World;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.plugin.java.JavaPlugin;

/**
 * Живая проба {@code World.playSound}: игроку на входе отправляются
 * позиционный, строковый, сущностный и Adventure-звуки. Дальний звук
 * (100 блоков) не должен дойти. Включается {@code CHUNKWAKE_WORLD_SOUND_PROBE}.
 */
final class WorldSoundProbe implements Listener {
    private final JavaPlugin plugin;

    WorldSoundProbe(JavaPlugin plugin) {
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
            Location at = player.getLocation();
            world.playSound(at, Sound.ENTITY_PLAYER_LEVELUP, SoundCategory.PLAYERS, 1.0f, 1.0f);
            world.playSound(at.clone().add(100, 0, 0), Sound.BLOCK_NOTE_BLOCK_PLING,
                    SoundCategory.PLAYERS, 1.0f, 1.0f);
            world.playSound(player, Sound.ENTITY_EXPERIENCE_ORB_PICKUP,
                    SoundCategory.PLAYERS, 1.0f, 1.0f);
            world.playSound(at, "minecraft:block.note_block.bass", SoundCategory.PLAYERS,
                    1.0f, 1.0f);
            net.kyori.adventure.sound.Sound adventure = net.kyori.adventure.sound.Sound.sound(
                    net.kyori.adventure.key.Key.key("minecraft:block.note_block.bell"),
                    net.kyori.adventure.sound.Sound.Source.MASTER, 1.0f, 1.0f);
            world.playSound(adventure, at);
            world.playSound(adventure);
            plugin.getLogger().info("WORLDSOUND sent expected=5 near=" + at
                    + " far=" + at.clone().add(100, 0, 0));
        } catch (Throwable t) {
            plugin.getLogger().log(java.util.logging.Level.WARNING,
                    "WORLDSOUND fail", t);
        }
    }
}
