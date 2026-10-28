package dev.chunkwake.testplugin;

import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;
import org.bukkit.Material;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.entity.Player;
import org.bukkit.event.block.BlockBreakEvent;
import org.bukkit.event.entity.EntityDamageEvent;
import org.bukkit.event.entity.PlayerDeathEvent;
import org.bukkit.event.player.PlayerDropItemEvent;
import org.bukkit.event.player.PlayerGameModeChangeEvent;
import org.bukkit.inventory.ItemStack;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.plugin.java.JavaPlugin;

/** Обычный Bukkit-плагин для проверки моста: команды, события, планировщик. */
public final class ChunkwakeTestPlugin extends JavaPlugin implements Listener {
    private long ticks;

    @Override
    public void onEnable() {
        getLogger().info("onEnable on " + getServer().getName() + " " + getServer().getVersion());
        String bench = System.getenv("CHUNKWAKE_BENCH_MOBS");
        if (bench != null) {
            benchMobs(Integer.parseInt(bench));
            return;
        }
        getServer().getPluginManager().registerEvents(this, this);
        ItemStack bread = new ItemStack(Material.BREAD, 3);
        getLogger().info("item api: " + bread.getType() + " x" + bread.getAmount() + " max " + bread.getMaxStackSize());
        getServer().getScheduler().runTaskTimer(this, () -> {
            ticks += 100;
            getLogger().info("scheduler: " + ticks + " ticks");
        }, 100, 100);
        // Редстоун: поршень от редстоун-блока и поршень через провод.
        getServer().getScheduler().runTaskLater(this, () -> {
            org.bukkit.World w = getServer().getWorlds().get(0);
            w.getBlockAt(0, 200, -1).setType(Material.STONE);
            w.getBlockAt(0, 200, 0).setType(Material.PISTON);
            w.getBlockAt(0, 200, 1).setType(Material.REDSTONE_BLOCK);
            for (int x = 10; x <= 13; x++) {
                w.getBlockAt(x, 199, 0).setType(Material.STONE);
            }
            w.getBlockAt(10, 200, -1).setType(Material.STONE);
            w.getBlockAt(10, 200, 0).setType(Material.PISTON);
            w.getBlockAt(11, 200, 0).setType(Material.REDSTONE_WIRE);
            w.getBlockAt(12, 200, 0).setType(Material.REDSTONE_WIRE);
            w.getBlockAt(13, 200, 0).setType(Material.REDSTONE_BLOCK);
            // Воронка над сундуком: предмет падает, воронка втягивает и отдаёт вниз.
            w.getBlockAt(20, 199, 0).setType(Material.CHEST);
            w.getBlockAt(20, 200, 0).setType(Material.HOPPER);
            w.dropItem(new org.bukkit.Location(w, 20.5, 201.2, 0.5), new ItemStack(Material.DIAMOND, 3));
            // Жидкости: вода растекается по полу, лава у воды — обсидиан.
            for (int x = 28; x <= 38; x++) {
                for (int z = -4; z <= 4; z++) {
                    w.getBlockAt(x, 199, z).setType(Material.STONE);
                }
            }
            w.getBlockAt(33, 200, 0).setType(Material.WATER);
            w.getBlockAt(40, 200, 0).setType(Material.LAVA);
            w.getBlockAt(41, 200, 0).setType(Material.WATER);
            // Заводнённые ступени — источник воды; сундук рядом вода не смывает.
            for (int x = 50; x <= 54; x++) {
                w.getBlockAt(x, 199, 0).setType(Material.STONE);
            }
            w.getBlockAt(50, 200, 0).setBlockData(getServer().createBlockData("minecraft:oak_stairs[waterlogged=true]"));
            w.getBlockAt(52, 200, 0).setType(Material.CHEST);
            // Раздатчик со стрелами: воронка сверху подаёт, редстоун-блок запускает.
            w.getBlockAt(60, 200, 0).setBlockData(getServer().createBlockData("minecraft:dispenser[facing=east]"), false);
            w.getBlockAt(60, 201, 0).setType(Material.HOPPER);
            w.dropItem(new org.bukkit.Location(w, 60.5, 202.2, 0.5), new ItemStack(Material.ARROW, 2));
            w.dropItem(new org.bukkit.Location(w, 60.5, 202.2, 0.5), new ItemStack(Material.SNOWBALL, 1));
            w.getBlockAt(66, 200, 0).setType(Material.STONE);
            // Флай-машина (двусторонний двигатель): липкие поршни друг к другу через щель.
            fly(w, Material.OBSERVER, 100, 0, org.bukkit.block.BlockFace.WEST);
            fly(w, Material.PISTON, 101, 0, org.bukkit.block.BlockFace.EAST);
            w.getBlockAt(100, 210, 1).setType(Material.SLIME_BLOCK);
            w.getBlockAt(101, 210, 1).setType(Material.SLIME_BLOCK);
            w.getBlockAt(103, 210, 0).setType(Material.SLIME_BLOCK);
            w.getBlockAt(104, 210, 0).setType(Material.SLIME_BLOCK);
            fly(w, Material.STICKY_PISTON, 103, 1, org.bukkit.block.BlockFace.WEST);
            fly(w, Material.OBSERVER, 104, 1, org.bukkit.block.BlockFace.EAST);
        }, 40);
        // Запуск: блок перед правым наблюдателем.
        getServer().getScheduler().runTaskLater(this,
                () -> getServer().getWorlds().get(0).getBlockAt(105, 210, 1).setType(Material.STONE), 50);
        getServer().getScheduler().runTaskLater(this,
                () -> getServer().getWorlds().get(0).getBlockAt(59, 200, 0).setType(Material.REDSTONE_BLOCK), 110);
        getServer().getScheduler().runTaskLater(this,
                () -> getServer().getWorlds().get(0).getBlockAt(59, 200, 0).setType(Material.AIR), 125);
        getServer().getScheduler().runTaskLater(this,
                () -> getServer().getWorlds().get(0).getBlockAt(59, 200, 0).setType(Material.REDSTONE_BLOCK), 135);
        // Через 6 с: где задний наблюдатель и цела ли машина (8 блоков).
        getServer().getScheduler().runTaskLater(this, () -> {
            org.bukkit.World w = getServer().getWorlds().get(0);
            int rear = -1, blocks = 0;
            for (int x = 98; x <= 140; x++) {
                for (int z = 0; z <= 1; z++) {
                    Material m = w.getBlockAt(x, 210, z).getType();
                    if (m == Material.OBSERVER && z == 0) rear = x;
                    if (m == Material.OBSERVER || m == Material.SLIME_BLOCK || m == Material.PISTON || m == Material.STICKY_PISTON
                            || m == Material.MOVING_PISTON) blocks++;
                }
            }
            getLogger().info("fly: rear observer x=" + rear + " blocks=" + blocks);
        }, 170);
        getServer().getScheduler().runTaskLater(this, () -> {
            org.bukkit.World w = getServer().getWorlds().get(0);
            getLogger().info("redstone: direct=" + w.getBlockAt(0, 200, -2).getType() + "/" + w.getBlockAt(0, 200, -1).getType()
                    + " wire=" + w.getBlockAt(10, 200, -2).getType() + "/" + w.getBlockAt(10, 200, -1).getType());
        }, 60);
        getServer().getScheduler().runTaskLater(this, () -> {
            org.bukkit.World w = getServer().getWorlds().get(0);
            getLogger().info("fluids: flow=" + w.getBlockAt(33, 200, 2).getType() + " fall=" + w.getBlockAt(33, 199, 5).getType() + " edge=" + w.getBlockAt(33, 200, 4).getType()
                    + " meet=" + w.getBlockAt(40, 200, 0).getType()
                    + " logged=" + w.getBlockAt(50, 200, 1).getType() + "/" + w.getBlockAt(52, 200, 0).getType()
                    + " " + ((org.bukkit.block.data.Waterlogged) w.getBlockAt(50, 200, 0).getBlockData()).isWaterlogged());
        }, 140);
        getServer().getScheduler().runTaskLater(this, () -> getLogger().info("hopper: moves=" + hopperMoves + " pickups=" + hopperPickups), 120);
        getServer().getScheduler().runTaskLater(this, () -> {
            org.bukkit.World w = getServer().getWorlds().get(0);
            var cow = (org.bukkit.entity.Cow) w.spawnEntity(new org.bukkit.Location(w, 0.5, 200, 30.5), org.bukkit.entity.EntityType.COW);
            var sheep = w.spawn(new org.bukkit.Location(w, 3.5, 200, 30.5), org.bukkit.entity.Sheep.class, s -> s.setColor(org.bukkit.DyeColor.RED));
            getLogger().info("mob: " + cow.getType() + " health=" + cow.getHealth() + "/" + cow.getMaxHealth() + " adult=" + cow.isAdult()
                    + " sheep=" + sheep.getColor() + " living=" + w.getLivingEntities().size() + " spawned=" + creatureSpawns);
            cow.damage(20);
            getServer().getScheduler().runTaskLater(this, () -> getLogger().info("mob dead: " + cow.isDead() + " valid=" + cow.isValid()), 30);
        }, 150);
    }

    private int hopperMoves, hopperPickups, creatureSpawns;

    /** Замер: {@code CHUNKWAKE_BENCH_MOBS=N} — N коров у спавна, TPS/MSPT до и после. */
    private void benchMobs(int n) {
        org.bukkit.World w = getServer().getWorlds().get(0);
        getServer().getScheduler().runTaskLater(this, () -> {
            getLogger().info("bench before: " + tps());
            for (int i = 0; i < n; i++) {
                w.spawnEntity(new org.bukkit.Location(w, (i % 40) - 20 + 0.5, -60, (i / 40) - 12 + 0.5), org.bukkit.entity.EntityType.COW);
            }
            getLogger().info("bench spawned " + n + " living=" + w.getLivingEntities().size());
        }, 200);
        for (int t = 1; t <= 9; t++) {
            int at = t;
            getServer().getScheduler().runTaskLater(this, () -> getLogger().info("bench after " + at * 10 + "s: " + tps()
                    + " living=" + w.getLivingEntities().size()), 200 + t * 200L);
        }
    }

    private static String tps() {
        try {
            var m = Class.forName("dev.chunkwake.bridge.Bridge").getDeclaredMethod("world", String.class);
            m.setAccessible(true);
            return (String) m.invoke(null, "tps");
        } catch (ReflectiveOperationException e) {
            return "?";
        }
    }

    @EventHandler
    public void onCreatureSpawn(org.bukkit.event.entity.CreatureSpawnEvent e) {
        creatureSpawns++;
    }

    @EventHandler
    public void onMobDeath(org.bukkit.event.entity.EntityDeathEvent e) {
        if (e.getEntity() instanceof org.bukkit.entity.Player) {
            return;
        }
        getLogger().info("mob death: " + e.getEntityType() + " drops=" + e.getDrops().stream().map(i -> i.getType().name()).sorted().toList()
                + " xp=" + e.getDroppedExp());
    }

    @EventHandler
    public void onMenuClose(org.bukkit.event.inventory.InventoryCloseEvent e) {
        getLogger().info("menu closed: " + e.getPlayer().getName() + " size=" + e.getInventory().getSize());
    }

    @EventHandler
    public void onMenuClick(org.bukkit.event.inventory.InventoryClickEvent e) {
        getLogger().info("menu click: " + e.getRawSlot() + " " + e.getClick() + " " + e.getAction() + " " + e.getCurrentItem());
        if (e.getView().getTopInventory().getSize() == 18 && e.getRawSlot() < 18) {
            e.setCancelled(true);
        }
    }

    @EventHandler
    public void onShootBow(org.bukkit.event.entity.EntityShootBowEvent e) {
        getLogger().info("shoot: " + e.getBow().getType() + " " + e.getConsumable().getType() + " force=" + e.getForce() + " " + e.getHand());
    }

    @EventHandler
    public void onDispense(org.bukkit.event.block.BlockDispenseEvent e) {
        getLogger().info("dispense: " + e.getItem().getType() + " at " + e.getBlock().getX());
    }

    private static void fly(org.bukkit.World w, Material m, int x, int z, org.bukkit.block.BlockFace face) {
        org.bukkit.block.Block b = w.getBlockAt(x, 210, z);
        org.bukkit.block.data.Directional d = (org.bukkit.block.data.Directional) m.createBlockData();
        d.setFacing(face);
        b.setBlockData(d, false);
    }

    @EventHandler
    public void onMove(org.bukkit.event.inventory.InventoryMoveItemEvent e) {
        hopperMoves++;
        if (hopperMoves == 1) {
            getLogger().info("move event: " + e.getItem().getType() + " " + e.getSource().getType() + " -> " + e.getDestination().getType());
        }
    }

    @EventHandler
    public void onHopperPickup(org.bukkit.event.inventory.InventoryPickupItemEvent e) {
        hopperPickups++;
    }

    @EventHandler
    public void onPiston(org.bukkit.event.block.BlockPistonExtendEvent e) {
        getLogger().info("piston event: extend " + e.getBlocks().size() + " " + e.getDirection());
    }

    @Override
    public void onDisable() {
        getLogger().info("onDisable");
    }

    @Override
    public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        if (command.getName().equals("up")) {
            if (!(sender instanceof Player p)) {
                return false;
            }
            int h = args.length > 0 ? Integer.parseInt(args[0]) : 5;
            org.bukkit.Location to = p.getLocation().add(0, h, 0);
            to.getBlock().getRelative(org.bukkit.block.BlockFace.DOWN).setType(Material.GLASS);
            p.teleport(to);
            getLogger().info("up: " + p.getName() + " -> " + to.getBlockY() + " below="
                    + to.getBlock().getRelative(org.bukkit.block.BlockFace.DOWN).getType());
            return true;
        }
        if (command.getName().equals("kit")) {
            if (!(sender instanceof Player p)) {
                return false;
            }
            p.setHealth(p.getMaxHealth());
            p.setFoodLevel(20);
            p.getInventory().addItem(new ItemStack(Material.BREAD, 3));
            p.sendMessage(Component.text("Kit given", NamedTextColor.GREEN));
            return true;
        }
        sender.sendMessage(Component.text("Hello, " + sender.getName() + "!", NamedTextColor.GREEN));
        return true;
    }

    @EventHandler
    public void onJoin(PlayerJoinEvent event) {
        Player p = event.getPlayer();
        getLogger().info("join state: " + p.getName() + " health=" + p.getHealth() + " food=" + p.getFoodLevel()
                + " mode=" + p.getGameMode() + " held=" + p.getInventory().getItemInMainHand()
                + " op=" + p.isOp() + " kit=" + p.hasPermission("chunkwake.test.kit"));
        event.getPlayer().sendMessage(Component.text("Welcome to chunkwake, " + event.getPlayer().getName(),
                NamedTextColor.GOLD));
        // Окно плагина: создать, наполнить, открыть, прочитать вид, закрыть.
        getServer().getScheduler().runTaskLater(this, () -> {
            var menu = getServer().createInventory(null, 18, Component.text("Test menu"));
            menu.setItem(4, new org.bukkit.inventory.ItemStack(org.bukkit.Material.DIAMOND, 3));
            var left = menu.addItem(new org.bukkit.inventory.ItemStack(org.bukkit.Material.STONE, 70));
            var view = p.openInventory(menu);
            getLogger().info("menu: open=" + (view != null) + " top=" + p.getOpenInventory().getTopInventory().getSize()
                    + " slot4=" + menu.getItem(4).getType() + "x" + menu.getItem(4).getAmount()
                    + " first=" + menu.getItem(0).getAmount() + "+" + menu.getItem(1).getAmount() + " left=" + left.size()
                    + " viewers=" + menu.getViewers().size());
            getServer().getScheduler().runTaskLater(this, () -> p.closeInventory(), 5);
            var sharp = org.bukkit.enchantments.Enchantment.SHARPNESS;
            getLogger().info("enchant rules: sword=" + sharp.canEnchantItem(new org.bukkit.inventory.ItemStack(org.bukkit.Material.DIAMOND_SWORD))
                    + " boots=" + sharp.canEnchantItem(new org.bukkit.inventory.ItemStack(org.bukkit.Material.DIAMOND_BOOTS))
                    + " smite=" + sharp.conflictsWith(org.bukkit.enchantments.Enchantment.SMITE)
                    + " unbreaking=" + sharp.conflictsWith(org.bukkit.enchantments.Enchantment.UNBREAKING));
        }, 20);
        // Эффекты: наложить, затем на следующем тике прочитать из снимка.
        boolean added = p.addPotionEffect(new org.bukkit.potion.PotionEffect(
                org.bukkit.potion.PotionEffectType.HASTE, 200, 1));
        var haste = org.bukkit.Registry.EFFECT.get(org.bukkit.NamespacedKey.minecraft("haste"));
        getLogger().info("haste color: " + (haste == null ? "?" : Integer.toHexString(haste.getColor().asRGB())));
        getServer().getScheduler().runTask(this, () -> getLogger().info("effects: " + p.getName() + " added=" + added
                + " " + p.getActivePotionEffects() + " haste=" + p.hasPotionEffect(org.bukkit.potion.PotionEffectType.HASTE)));
        // Чары и имя: в слот 8, затем прочитать обратно из снимка.
        var pick = new org.bukkit.inventory.ItemStack(org.bukkit.Material.DIAMOND_PICKAXE);
        pick.addEnchantment(org.bukkit.enchantments.Enchantment.EFFICIENCY, 5);
        pick.editMeta(m -> {
            m.displayName(net.kyori.adventure.text.Component.text("Test pick"));
            ((org.bukkit.inventory.meta.Damageable) m).setDamage(7);
            var pdc = m.getPersistentDataContainer();
            pdc.set(new org.bukkit.NamespacedKey(this, "owner"), org.bukkit.persistence.PersistentDataType.STRING, p.getName());
            pdc.set(new org.bukkit.NamespacedKey(this, "uses"), org.bukkit.persistence.PersistentDataType.LONG, 1L << 40);
            pdc.set(new org.bukkit.NamespacedKey(this, "soulbound"), org.bukkit.persistence.PersistentDataType.BOOLEAN, true);
        });
        getLogger().info("meta set: " + pick);
        p.getInventory().setItem(8, pick);
        p.giveExpLevels(30);
        p.giveExp(5);
        getServer().getScheduler().runTaskLater(this, () -> {
            var back = p.getInventory().getItem(8);
            var meta = back == null ? null : back.getItemMeta();
            getLogger().info("meta: " + p.getName() + " efficiency="
                    + (back == null ? 0 : back.getEnchantmentLevel(org.bukkit.enchantments.Enchantment.EFFICIENCY))
                    + " name=" + (meta == null || !meta.hasDisplayName() ? null
                            : net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer
                                    .plainText().serialize(meta.displayName()))
                    + " damage=" + (meta instanceof org.bukkit.inventory.meta.Damageable d ? d.getDamage() : -1)
                    + " level=" + p.getLevel() + " exp=" + p.getExp()
                    + " enchant=" + org.bukkit.Registry.ENCHANTMENT.get(org.bukkit.NamespacedKey.minecraft("sharpness")).getMaxLevel()
                    + " pdc=" + (meta == null ? null : meta.getPersistentDataContainer().get(
                            new org.bukkit.NamespacedKey(this, "owner"), org.bukkit.persistence.PersistentDataType.STRING))
                    + "/" + (meta == null ? null : meta.getPersistentDataContainer().get(
                            new org.bukkit.NamespacedKey(this, "uses"), org.bukkit.persistence.PersistentDataType.LONG))
                    + "/" + (meta == null ? null : meta.getPersistentDataContainer().get(
                            new org.bukkit.NamespacedKey(this, "soulbound"), org.bukkit.persistence.PersistentDataType.BOOLEAN)));
        }, 2);
    }

    @EventHandler
    public void onDamage(EntityDamageEvent event) {
        getLogger().info("damage: " + event.getEntity().getName() + " " + event.getCause() + " " + event.getDamage());
    }

    @EventHandler
    public void onDeath(PlayerDeathEvent event) {
        getLogger().info("death: " + event.getPlayer().getName() + " drops=" + event.getDrops().size()
                + " type=" + event.getDamageSource().getDamageType().getKey());
    }

    @EventHandler
    public void onDrop(PlayerDropItemEvent event) {
        getLogger().info("drop: " + event.getPlayer().getName() + " " + event.getItemDrop().getItemStack());
    }

    @EventHandler
    public void onGameMode(PlayerGameModeChangeEvent event) {
        getLogger().info("gamemode: " + event.getPlayer().getName() + " -> " + event.getNewGameMode()
                + " (" + event.getCause() + ")");
    }

    @Override
    public java.util.List<String> onTabComplete(CommandSender sender, Command command, String alias, String[] args) {
        return command.getName().equals("up") && args.length == 1 ? java.util.List.of("5", "10", "50") : java.util.List.of();
    }

    @EventHandler
    public void onChat(io.papermc.paper.event.player.AsyncChatEvent event) {
        getLogger().info("chat: " + event.getPlayer().getName() + ": "
                + net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(event.message()));
    }

    @EventHandler
    public void onPlace(org.bukkit.event.block.BlockPlaceEvent event) {
        if (event.getBlockPlaced().getType() == Material.TNT) {
            event.setCancelled(true);
            event.getPlayer().sendMessage(Component.text("No TNT here", NamedTextColor.RED));
        }
    }

    @EventHandler
    public void onPrepareEnchant(org.bukkit.event.enchantment.PrepareItemEnchantEvent event) {
        getLogger().info("prepare enchant: " + event.getItem().getType() + " bonus=" + event.getEnchantmentBonus());
    }

    @EventHandler
    public void onEnchant(org.bukkit.event.enchantment.EnchantItemEvent event) {
        getLogger().info("enchant: " + event.getItem().getType() + " cost=" + event.getExpLevelCost()
                + " " + event.getEnchantsToAdd());
    }

    @EventHandler
    public void onInteract(org.bukkit.event.player.PlayerInteractEvent event) {
        getLogger().info("interact: " + event.getAction() + " " + event.getClickedBlock());
    }

    @EventHandler(ignoreCancelled = true)
    public void onBreak(BlockBreakEvent event) {
        if (event.getBlock().getType() == Material.BEDROCK) {
            event.setCancelled(true);
            event.getPlayer().sendMessage(Component.text("Bedrock is protected by chunkwake-test-plugin",
                    NamedTextColor.RED));
        }
    }
}
