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
