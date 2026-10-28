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
        getServer().getPluginManager().registerEvents(this, this);
        ItemStack bread = new ItemStack(Material.BREAD, 3);
        getLogger().info("item api: " + bread.getType() + " x" + bread.getAmount() + " max " + bread.getMaxStackSize());
        getServer().getScheduler().runTaskTimer(this, () -> {
            ticks += 100;
            getLogger().info("scheduler: " + ticks + " ticks");
        }, 100, 100);
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
        // Эффекты: наложить, затем на следующем тике прочитать из снимка.
        boolean added = p.addPotionEffect(new org.bukkit.potion.PotionEffect(
                org.bukkit.potion.PotionEffectType.HASTE, 200, 1));
        getServer().getScheduler().runTask(this, () -> getLogger().info("effects: " + p.getName() + " added=" + added
                + " " + p.getActivePotionEffects() + " haste=" + p.hasPotionEffect(org.bukkit.potion.PotionEffectType.HASTE)));
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
