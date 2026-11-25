package dev.chunkwake.testplugin;

import org.bukkit.Bukkit;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.entity.EntityType;
import org.bukkit.plugin.java.JavaPlugin;

/** Plugin-driven A/B load: same 200/1000 cows + redstone windows без opcmd_probe. */
final class AbLoadProbe {
    private final JavaPlugin plugin;
    private final World world;
    private final int round;
    private int tick;
    private int spawned;
    private int target;
    private boolean redstone;
    private int toggles;

    private AbLoadProbe(JavaPlugin plugin, int round) {
        this.plugin = plugin;
        this.world = plugin.getServer().getWorlds().get(0);
        this.round = round;
    }

    static void run(JavaPlugin plugin, int round) {
        AbLoadProbe probe = new AbLoadProbe(plugin, round);
        plugin.getServer().getScheduler().runTaskTimer(plugin, probe::step, 1, 1);
    }

    private void step() {
        int now = ++this.tick;
        if (now == 20) {
            startCows(200);
        } else if (now == 340) {
            command("perfprobe reset");
        } else if (now == 640) {
            command("perfprobe sample r" + this.round + "-cows200");
        } else if (now == 660) {
            startCows(800);
        } else if (now == 1080) {
            command("perfprobe reset");
        } else if (now == 1380) {
            command("perfprobe sample r" + this.round + "-cows1000");
        } else if (now == 1400) {
            buildRedstone();
        } else if (now >= 1400 && now <= 1480 && (now - 1400) % 5 == 0) {
            toggleRedstone();
        } else if (now == 1520) {
            command("perfprobe reset");
        } else if (now == 1820) {
            command("perfprobe sample r" + this.round + "-redstone");
        } else if (now == 1830) {
            this.plugin.getLogger().info("ABLOAD done round " + this.round
                    + " entities=" + this.world.getEntities().size());
        }
        if (this.spawned < this.target && now % 1 == 0) {
            int batch = Math.min(20, this.target - this.spawned);
            for (int i = 0; i < batch; i++) {
                int n = this.spawned++;
                double angle = 2 * Math.PI * n / Math.max(this.target, 1);
                double radius = 6 + (n % 12);
                Location spawn = this.world.getSpawnLocation();
                double x = spawn.getX() + Math.cos(angle) * radius;
                double z = spawn.getZ() + Math.sin(angle) * radius;
                this.world.spawnEntity(new Location(this.world, x, spawn.getY(), z),
                        EntityType.COW);
            }
        }
    }

    private void startCows(int count) {
        this.spawned = 0;
        this.target = count;
        this.plugin.getLogger().info("ABLOAD start cows=" + count + " round=" + this.round);
    }

    private void command(String line) {
        this.plugin.getServer().dispatchCommand(this.plugin.getServer().getConsoleSender(), line);
    }

    private void buildRedstone() {
        Location spawn = this.world.getSpawnLocation();
        int x = spawn.getBlockX() + 4;
        int y = spawn.getBlockY();
        int z = spawn.getBlockZ() + 7;
        for (int i = 0; i <= 72; i++) {
            this.world.getBlockAt(x, y - 1, z + i).setType(Material.STONE, false);
            this.world.getBlockAt(x, y, z + i).setType(Material.AIR, false);
        }
        for (int i = 1; i <= 72; i++) {
            this.world.getBlockAt(x, y, z + i).setType(Material.REDSTONE_WIRE, false);
        }
        this.world.getBlockAt(x, y, z).setBlockData(Bukkit.createBlockData(
                "minecraft:lever[face=floor,facing=south,powered=false]"), false);
        this.redstone = true;
        this.plugin.getLogger().info("ABLOAD redstone built at " + x + "," + y + "," + z);
    }

    private void toggleRedstone() {
        if (!this.redstone) {
            return;
        }
        this.toggles++;
        Location spawn = this.world.getSpawnLocation();
        int x = spawn.getBlockX() + 4;
        int y = spawn.getBlockY();
        int z = spawn.getBlockZ() + 7;
        boolean on = this.toggles % 2 == 0;
        this.world.getBlockAt(x, y, z).setBlockData(Bukkit.createBlockData(
                "minecraft:lever[face=floor,facing=south,powered=" + on + "]"), true);
    }
}
