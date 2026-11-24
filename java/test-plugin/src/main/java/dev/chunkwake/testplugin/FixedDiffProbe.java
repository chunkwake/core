package dev.chunkwake.testplugin;

import java.util.concurrent.atomic.AtomicInteger;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.BlockFace;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.block.BlockPistonExtendEvent;
import org.bukkit.event.entity.EntityDeathEvent;
import org.bukkit.event.inventory.InventoryMoveItemEvent;
import org.bukkit.event.inventory.InventoryPickupItemEvent;
import org.bukkit.inventory.ItemStack;
import org.bukkit.plugin.java.JavaPlugin;

/** Fixed-tick Redstone/hopper/fluids/fly/mob scenario used by the Paper diff. */
final class FixedDiffProbe implements Listener {
    private final JavaPlugin plugin;
    private final AtomicInteger tick = new AtomicInteger();
    private final AtomicInteger moves = new AtomicInteger();
    private final AtomicInteger pickups = new AtomicInteger();
    private final AtomicInteger pistonExtends = new AtomicInteger();
    private final java.util.Set<String> death = java.util.concurrent.ConcurrentHashMap.newKeySet();

    private FixedDiffProbe(JavaPlugin plugin) {
        this.plugin = plugin;
    }

    static void run(JavaPlugin plugin) {
        String midSave = System.getenv("CHUNKWAKE_PISTON_MIDSAVE");
        if (midSave != null) {
            runMidSave(plugin, Integer.parseInt(midSave));
            return;
        }
        if (System.getenv("CHUNKWAKE_PISTON_CHECK") != null) {
            runCheck(plugin);
            return;
        }
        FixedDiffProbe probe = new FixedDiffProbe(plugin);
        plugin.getServer().getPluginManager().registerEvents(probe, plugin);
        plugin.getServer().getScheduler().runTaskTimer(plugin, probe::step, 1, 1);
    }

    /** Saves the world in the middle of a piston move, then hard-kills the JVM. */
    private static void runMidSave(JavaPlugin plugin, int at) {
        AtomicInteger now = new AtomicInteger();
        World w = plugin.getServer().getWorlds().get(0);
        plugin.getServer().getScheduler().runTaskTimer(plugin, () -> {
            int t = now.incrementAndGet();
            if (t == 20) {
                for (int cx = 5; cx <= 9; cx++) {
                    for (int cz = -1; cz <= 1; cz++) {
                        w.getChunkAt(cx, cz).load(true);
                        w.getChunkAt(cx, cz).addPluginChunkTicket(plugin);
                    }
                }
                for (int x = 95; x <= 140; x++) {
                    for (int y = 209; y <= 211; y++) {
                        for (int z = -1; z <= 2; z++) {
                            w.getBlockAt(x, y, z).setType(Material.AIR, false);
                        }
                    }
                }
                flySetup(w);
            }
            if (t == at) {
                var moves = 0;
                for (int x = 95; x <= 140; x++) {
                    for (int z = 0; z <= 1; z++) {
                        if (w.getBlockAt(x, 210, z).getType() == Material.MOVING_PISTON) {
                            moves++;
                        }
                    }
                }
                plugin.getLogger().info("VANILLADIFF midsave t=" + t + " moving=" + moves);
                w.save();
                plugin.getLogger().info("VANILLADIFF midsave saved");
                Runtime.getRuntime().halt(0);
            }
        }, 1, 1);
    }

    /** After restart: lists any moving pistons that were not restored. */
    private static void runCheck(JavaPlugin plugin) {
        AtomicInteger now = new AtomicInteger();
        World w = plugin.getServer().getWorlds().get(0);
        boolean before = w.isChunkLoaded(6, 0);
        boolean loaded = w.getChunkAt(6, 0).load(true);
        plugin.getLogger().info("VANILLADIFF chunk-api before=" + before
                + " load=" + loaded + " after=" + w.isChunkLoaded(6, 0)
                + " block=" + w.getBlockAt(105, 210, 1).getType());
        for (int cx = 5; cx <= 9; cx++) {
            for (int cz = -1; cz <= 1; cz++) {
                w.getChunkAt(cx, cz).load(true);
                w.getChunkAt(cx, cz).addPluginChunkTicket(plugin);
            }
        }
        plugin.getServer().getScheduler().runTaskTimer(plugin, () -> {
            int t = now.incrementAndGet();
            if (t < 20 || t % 20 != 0) {
                return;
            }
            int moves = 0;
            int rear = -1;
            for (int x = 95; x <= 155; x++) {
                for (int z = 0; z <= 1; z++) {
                    Material m = w.getBlockAt(x, 210, z).getType();
                    if (m == Material.MOVING_PISTON) {
                        moves++;
                    }
                    if (m == Material.OBSERVER && z == 0) {
                        rear = x;
                    }
                }
            }
            StringBuilder z0 = new StringBuilder();
            StringBuilder z1 = new StringBuilder();
            for (int x = 90; x <= 150; x++) {
                z0.append(glyph(w.getBlockAt(x, 210, 0)));
                z1.append(glyph(w.getBlockAt(x, 210, 1)));
            }
            plugin.getLogger().info("VANILLADIFF check t=" + t + " moving=" + moves
                    + " rear0=" + rear + " z0=" + z0 + " z1=" + z1);
        }, 1, 1);
    }

    private static void flySetup(World w) {
        fly(w, Material.OBSERVER, 100, 0, BlockFace.WEST);
        fly(w, Material.PISTON, 101, 0, BlockFace.EAST);
        w.getBlockAt(100, 210, 1).setType(Material.SLIME_BLOCK);
        w.getBlockAt(101, 210, 1).setType(Material.SLIME_BLOCK);
        w.getBlockAt(103, 210, 0).setType(Material.SLIME_BLOCK);
        w.getBlockAt(104, 210, 0).setType(Material.SLIME_BLOCK);
        fly(w, Material.STICKY_PISTON, 103, 1, BlockFace.WEST);
        fly(w, Material.OBSERVER, 104, 1, BlockFace.EAST);
        w.getBlockAt(105, 210, 1).setType(Material.STONE);
    }

    private void step() {
        int now = this.tick.incrementAndGet();
        if (now == 20) {
            setup();
        } else if (now == 60) {
            mob();
        } else if (now == 150) {
            report();
        }
        if (now >= 20 && now <= 80 && now % 4 == 0) {
            trace(now);
        }
    }

    private void load(World w, int x, int z) {
        for (int dx = -1; dx <= 1; dx++) {
            for (int dz = -1; dz <= 1; dz++) {
                var chunk = w.getChunkAt((x >> 4) + dx, (z >> 4) + dz);
                chunk.load(true);
                chunk.addPluginChunkTicket(this.plugin);
            }
        }
    }

    private void setup() {
        World w = this.plugin.getServer().getWorlds().get(0);
        for (int x : new int[] {0, 10, 20, 33, 40, 50, 100, 101, 103, 104, 105}) {
            load(w, x, 0);
        }
        // Wipe the fly and wire areas so a previous run cannot leave debris.
        for (int x = 95; x <= 140; x++) {
            for (int y = 209; y <= 211; y++) {
                for (int z = -1; z <= 2; z++) {
                    w.getBlockAt(x, y, z).setType(Material.AIR, false);
                }
            }
        }
        for (int x = 6; x <= 14; x++) {
            for (int y = 198; y <= 202; y++) {
                for (int z = -3; z <= 2; z++) {
                    w.getBlockAt(x, y, z).setType(Material.AIR, false);
                }
            }
        }
        // Redstone
        w.getBlockAt(0, 200, -1).setType(Material.STONE);
        w.getBlockAt(0, 200, 0).setType(Material.PISTON);
        w.getBlockAt(0, 200, 1).setType(Material.REDSTONE_BLOCK);
        for (int x = 10; x <= 13; x++) {
            w.getBlockAt(x, 199, 0).setType(Material.STONE);
        }
        w.getBlockAt(10, 200, -1).setType(Material.STONE);
        w.getBlockAt(10, 200, 0).setType(Material.PISTON);
        // Place the power source before the dust so both servers process the
        // same neighbour-update order.
        w.getBlockAt(13, 200, 0).setType(Material.REDSTONE_BLOCK);
        w.getBlockAt(12, 200, 0).setType(Material.REDSTONE_WIRE);
        w.getBlockAt(11, 200, 0).setType(Material.REDSTONE_WIRE);

        // Hopper
        w.getBlockAt(20, 199, 0).setType(Material.CHEST);
        w.getBlockAt(20, 200, 0).setType(Material.HOPPER);
        w.dropItem(new org.bukkit.Location(w, 20.5, 201.2, 0.5), new ItemStack(Material.DIAMOND, 3));

        // Fluids
        for (int x = 28; x <= 38; x++) {
            for (int z = -4; z <= 4; z++) {
                w.getBlockAt(x, 199, z).setType(Material.STONE);
            }
        }
        w.getBlockAt(33, 200, 0).setType(Material.WATER);
        w.getBlockAt(40, 200, 0).setType(Material.LAVA);
        w.getBlockAt(41, 200, 0).setType(Material.WATER);
        for (int x = 50; x <= 54; x++) {
            for (int z = -1; z <= 1; z++) {
                w.getBlockAt(x, 199, z).setType(Material.STONE);
            }
        }
        w.getBlockAt(50, 200, 0).setType(Material.OAK_STAIRS);
        w.getBlockAt(50, 200, 1).setType(Material.WATER);
        w.getBlockAt(52, 200, 0).setType(Material.WATER);

        // Fly
        flySetup(w);
        this.plugin.getLogger().info("VANILLADIFF setup minY=" + w.getMinHeight()
                + " maxY=" + w.getMaxHeight()
                + " redstone=" + w.getBlockAt(0, 200, 1).getType()
                + " water=" + w.getBlockAt(33, 200, 0).getType()
                + " fly=" + w.getBlockAt(100, 210, 0).getType());
    }

    private void mob() {
        World w = this.plugin.getServer().getWorlds().get(0);
        load(w, 0, 30);
        var sheep = w.spawn(new org.bukkit.Location(w, 3.5, 200, 30.5),
                org.bukkit.entity.Sheep.class, s -> s.setColor(org.bukkit.DyeColor.RED));
        for (int i = 0; i < 12; i++) {
            var cow = (org.bukkit.entity.Cow) w.spawnEntity(
                    new org.bukkit.Location(w, 0.5 + i * 1.2, 200, 30.5 + i * 0.4),
                    org.bukkit.entity.EntityType.COW);
            if (i == 0) {
                this.plugin.getLogger().info("VANILLADIFF mob-state type=" + cow.getType()
                        + " hp=" + cow.getHealth() + "/" + cow.getMaxHealth()
                        + " sheep=" + sheep.getColor());
            }
            cow.damage(20);
        }
    }

    private void report() {
        World w = this.plugin.getServer().getWorlds().get(0);
        this.plugin.getLogger().info("VANILLADIFF redstone direct="
                + w.getBlockAt(0, 200, -2).getType() + "/"
                + w.getBlockAt(0, 200, -1).getType() + " wire="
                + w.getBlockAt(10, 200, -2).getType() + "/"
                + w.getBlockAt(10, 200, -1).getType());
        this.plugin.getLogger().info("VANILLADIFF wire-data w11="
                + w.getBlockAt(11, 200, 0).getBlockData().getAsString()
                + " w12=" + w.getBlockAt(12, 200, 0).getBlockData().getAsString()
                + " piston=" + w.getBlockAt(10, 200, 0).getBlockData().getAsString());
        this.plugin.getLogger().info("VANILLADIFF hopper moves=" + this.moves.get()
                + " pickups=" + this.pickups.get());
        this.plugin.getLogger().info("VANILLADIFF fluids flow="
                + w.getBlockAt(33, 200, 2).getType() + " fall="
                + w.getBlockAt(33, 199, 5).getType() + " edge="
                + w.getBlockAt(33, 200, 4).getType() + " meet="
                + w.getBlockAt(40, 200, 0).getType() + " logged="
                + w.getBlockAt(50, 200, 1).getType() + "/"
                + w.getBlockAt(52, 200, 0).getType());
        int rear = -1, blocks = 0;
        for (int x = 98; x <= 140; x++) {
            for (int z = 0; z <= 1; z++) {
                Material m = w.getBlockAt(x, 210, z).getType();
                if (m == Material.OBSERVER && z == 0) {
                    rear = x;
                }
                if (m == Material.OBSERVER || m == Material.SLIME_BLOCK
                        || m == Material.PISTON || m == Material.STICKY_PISTON
                        || m == Material.MOVING_PISTON) {
                    blocks++;
                }
            }
        }
        this.plugin.getLogger().info("VANILLADIFF fly rear=" + rear + " blocks=" + blocks
                + " extends=" + this.pistonExtends.get());
        this.plugin.getLogger().info("VANILLADIFF mob-death=" + this.death.stream()
                .sorted().toList());
    }

    private void trace(int now) {
        World w = this.plugin.getServer().getWorlds().get(0);
        StringBuilder a = new StringBuilder();
        StringBuilder b = new StringBuilder();
        for (int x = 98; x <= 118; x++) {
            a.append(glyph(w.getBlockAt(x, 210, 0)));
            b.append(glyph(w.getBlockAt(x, 210, 1)));
        }
        String obs = "";
        for (int x : new int[] {100, 104, 123}) {
            Material m = w.getBlockAt(x, 210, 0).getType();
            if (m == Material.OBSERVER) {
                obs += x + "=" + w.getBlockAt(x, 210, 0).getBlockData().getAsString() + " ";
            }
        }
        this.plugin.getLogger().info("VANILLADIFF fly-trace t=" + now + " z0=" + a
                + " z1=" + b + " obs=" + obs.trim());
    }

    private static char glyph(org.bukkit.block.Block b) {
        return switch (b.getType()) {
            case OBSERVER -> b.getBlockData().getAsString().contains("powered=true") ? 'o' : 'O';
            case PISTON -> 'P';
            case STICKY_PISTON -> 'T';
            case PISTON_HEAD -> 'H';
            case MOVING_PISTON -> 'M';
            case SLIME_BLOCK -> 'S';
            case STONE -> '#';
            case AIR, CAVE_AIR -> '.';
            default -> '?';
        };
    }

    private static void fly(World w, Material material, int x, int z, BlockFace face) {
        var data = (org.bukkit.block.data.Directional) material.createBlockData();
        data.setFacing(face);
        w.getBlockAt(x, 210, z).setBlockData(data, false);
    }

    @EventHandler
    public void onMove(InventoryMoveItemEvent event) {
        this.moves.incrementAndGet();
    }

    @EventHandler
    public void onPickup(InventoryPickupItemEvent event) {
        this.pickups.incrementAndGet();
    }

    @EventHandler
    public void onPiston(BlockPistonExtendEvent event) {
        this.pistonExtends.incrementAndGet();
    }

    @EventHandler
    public void onDeath(EntityDeathEvent event) {
        if (event.getEntity() instanceof org.bukkit.entity.Cow) {
            event.getDrops().stream().map(i -> i.getType().name()).forEach(this.death::add);
        }
    }
}
