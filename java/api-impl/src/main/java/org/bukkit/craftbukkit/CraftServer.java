package org.bukkit.craftbukkit;

import dev.chunkwake.bridge.PlayerCalls;

/**
 * Настоящий класс сервера (как {@code CraftServer} в CraftBukkit).
 *
 * <p>Нужен плагинам, которые определяют пакет CraftBukkit по
 * {@code Bukkit.getServer().getClass().getPackage()} (ProtocolLib:
 * {@code MinecraftReflection.getCraftBukkitPackage()} и
 * {@code getCraftBukkitClass("CraftServer")}). Все методы {@code Server}
 * делегируются в {@code ChunkwakeServer} через {@link PlayerCalls}.
 */
public class CraftServer implements org.bukkit.Server {
    private final Object handle;
    private final net.minecraft.server.dedicated.DedicatedPlayerList nmsHandle =
            new net.minecraft.server.dedicated.DedicatedPlayerList();
    private final net.minecraft.server.dedicated.DedicatedServer nmsServer =
            net.minecraft.server.dedicated.DedicatedServer.create();

    public CraftServer(Object handle) {
        this.handle = handle;
    }


    public net.minecraft.server.dedicated.DedicatedPlayerList getHandle() {
        return this.nmsHandle;
    }

    @Override public java.lang.String getName() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getName", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public void shutdown() {
        PlayerCalls.call(this.handle, "shutdown", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public org.bukkit.UnsafeValues getUnsafe() {
        return (org.bukkit.UnsafeValues) PlayerCalls.call(this.handle, "getUnsafe", new Class<?>[0], new Object[]{  }, org.bukkit.UnsafeValues.class);
    }

    @Override public java.util.logging.Logger getLogger() {
        return (java.util.logging.Logger) PlayerCalls.call(this.handle, "getLogger", new Class<?>[0], new Object[]{  }, java.util.logging.Logger.class);
    }

    @Override public int getPort() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getPort", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.map.MapView getMap(int p0) {
        return (org.bukkit.map.MapView) PlayerCalls.call(this.handle, "getMap", new Class<?>[]{ int.class }, new Object[]{ p0 }, org.bukkit.map.MapView.class);
    }

    @Override public org.bukkit.map.MapView createMap(org.bukkit.World p0) {
        return (org.bukkit.map.MapView) PlayerCalls.call(this.handle, "createMap", new Class<?>[]{ org.bukkit.World.class }, new Object[]{ p0 }, org.bukkit.map.MapView.class);
    }

    @Override public void broadcast(net.md_5.bungee.api.chat.BaseComponent p0) {
        PlayerCalls.call(this.handle, "broadcast", new Class<?>[]{ net.md_5.bungee.api.chat.BaseComponent.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int broadcast(java.lang.String p0, java.lang.String p1) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "broadcast", new Class<?>[]{ java.lang.String.class, java.lang.String.class }, new Object[]{ p0, p1 }, int.class)).intValue();
    }

    @Override public void broadcast(net.md_5.bungee.api.chat.BaseComponent[] p0) {
        PlayerCalls.call(this.handle, "broadcast", new Class<?>[]{ net.md_5.bungee.api.chat.BaseComponent[].class }, new Object[]{ p0 }, void.class);
    }

    @Override public int broadcast(net.kyori.adventure.text.Component p0, java.lang.String p1) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "broadcast", new Class<?>[]{ net.kyori.adventure.text.Component.class, java.lang.String.class }, new Object[]{ p0, p1 }, int.class)).intValue();
    }

    @Override public int broadcast(net.kyori.adventure.text.Component p0) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "broadcast", new Class<?>[]{ net.kyori.adventure.text.Component.class }, new Object[]{ p0 }, int.class)).intValue();
    }

    @Override public void reload() {
        PlayerCalls.call(this.handle, "reload", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public java.lang.String getVersion() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getVersion", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public void sendPlainMessage(java.lang.String p0) {
        PlayerCalls.call(this.handle, "sendPlainMessage", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public java.lang.String getBukkitVersion() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getBukkitVersion", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public org.bukkit.inventory.ItemStack createExplorerMap(org.bukkit.World p0, org.bukkit.Location p1, org.bukkit.StructureType p2) {
        return (org.bukkit.inventory.ItemStack) PlayerCalls.call(this.handle, "createExplorerMap", new Class<?>[]{ org.bukkit.World.class, org.bukkit.Location.class, org.bukkit.StructureType.class }, new Object[]{ p0, p1, p2 }, org.bukkit.inventory.ItemStack.class);
    }

    @Override public org.bukkit.inventory.ItemStack createExplorerMap(org.bukkit.World p0, org.bukkit.Location p1, org.bukkit.generator.structure.StructureType p2, org.bukkit.map.MapCursor.Type p3, int p4, boolean p5) {
        return (org.bukkit.inventory.ItemStack) PlayerCalls.call(this.handle, "createExplorerMap", new Class<?>[]{ org.bukkit.World.class, org.bukkit.Location.class, org.bukkit.generator.structure.StructureType.class, org.bukkit.map.MapCursor.Type.class, int.class, boolean.class }, new Object[]{ p0, p1, p2, p3, p4, p5 }, org.bukkit.inventory.ItemStack.class);
    }

    @Override public org.bukkit.inventory.ItemStack createExplorerMap(org.bukkit.World p0, org.bukkit.Location p1, org.bukkit.generator.structure.StructureType p2, org.bukkit.map.MapCursor.Type p3) {
        return (org.bukkit.inventory.ItemStack) PlayerCalls.call(this.handle, "createExplorerMap", new Class<?>[]{ org.bukkit.World.class, org.bukkit.Location.class, org.bukkit.generator.structure.StructureType.class, org.bukkit.map.MapCursor.Type.class }, new Object[]{ p0, p1, p2, p3 }, org.bukkit.inventory.ItemStack.class);
    }

    @Override public org.bukkit.inventory.ItemStack createExplorerMap(org.bukkit.World p0, org.bukkit.Location p1, org.bukkit.StructureType p2, int p3, boolean p4) {
        return (org.bukkit.inventory.ItemStack) PlayerCalls.call(this.handle, "createExplorerMap", new Class<?>[]{ org.bukkit.World.class, org.bukkit.Location.class, org.bukkit.StructureType.class, int.class, boolean.class }, new Object[]{ p0, p1, p2, p3, p4 }, org.bukkit.inventory.ItemStack.class);
    }

    @Override public int getTicksPerSpawns(org.bukkit.entity.SpawnCategory p0) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getTicksPerSpawns", new Class<?>[]{ org.bukkit.entity.SpawnCategory.class }, new Object[]{ p0 }, int.class)).intValue();
    }

    @Override public int getSpawnLimit(org.bukkit.entity.SpawnCategory p0) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getSpawnLimit", new Class<?>[]{ org.bukkit.entity.SpawnCategory.class }, new Object[]{ p0 }, int.class)).intValue();
    }

    @Override public int getMaxPlayers() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getMaxPlayers", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public boolean removeRecipe(org.bukkit.NamespacedKey p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "removeRecipe", new Class<?>[]{ org.bukkit.NamespacedKey.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean removeRecipe(org.bukkit.NamespacedKey p0, boolean p1) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "removeRecipe", new Class<?>[]{ org.bukkit.NamespacedKey.class, boolean.class }, new Object[]{ p0, p1 }, boolean.class);
    }

    @Override public org.bukkit.inventory.ItemCraftResult craftItemResult(org.bukkit.inventory.ItemStack[] p0, org.bukkit.World p1, org.bukkit.entity.Player p2) {
        return (org.bukkit.inventory.ItemCraftResult) PlayerCalls.call(this.handle, "craftItemResult", new Class<?>[]{ org.bukkit.inventory.ItemStack[].class, org.bukkit.World.class, org.bukkit.entity.Player.class }, new Object[]{ p0, p1, p2 }, org.bukkit.inventory.ItemCraftResult.class);
    }

    @Override public org.bukkit.inventory.ItemCraftResult craftItemResult(org.bukkit.inventory.ItemStack[] p0, org.bukkit.World p1) {
        return (org.bukkit.inventory.ItemCraftResult) PlayerCalls.call(this.handle, "craftItemResult", new Class<?>[]{ org.bukkit.inventory.ItemStack[].class, org.bukkit.World.class }, new Object[]{ p0, p1 }, org.bukkit.inventory.ItemCraftResult.class);
    }

    @Override public java.io.File getPluginsFolder() {
        return (java.io.File) PlayerCalls.call(this.handle, "getPluginsFolder", new Class<?>[0], new Object[]{  }, java.io.File.class);
    }

    @Override public java.util.Collection getOnlinePlayers() {
        return (java.util.Collection) PlayerCalls.call(this.handle, "getOnlinePlayers", new Class<?>[0], new Object[]{  }, java.util.Collection.class);
    }

    @Override public void setMaxPlayers(int p0) {
        PlayerCalls.call(this.handle, "setMaxPlayers", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getViewDistance() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getViewDistance", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public int getSimulationDistance() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getSimulationDistance", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public java.lang.String getWorldType() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getWorldType", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public java.lang.String getMinecraftVersion() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getMinecraftVersion", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public boolean isOwnedByCurrentRegion(org.bukkit.block.Block p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isOwnedByCurrentRegion", new Class<?>[]{ org.bukkit.block.Block.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean isOwnedByCurrentRegion(org.bukkit.World p0, int p1, int p2, int p3) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isOwnedByCurrentRegion", new Class<?>[]{ org.bukkit.World.class, int.class, int.class, int.class }, new Object[]{ p0, p1, p2, p3 }, boolean.class);
    }

    @Override public boolean isOwnedByCurrentRegion(org.bukkit.World p0, int p1, int p2, int p3, int p4) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isOwnedByCurrentRegion", new Class<?>[]{ org.bukkit.World.class, int.class, int.class, int.class, int.class }, new Object[]{ p0, p1, p2, p3, p4 }, boolean.class);
    }

    @Override public boolean isOwnedByCurrentRegion(org.bukkit.World p0, io.papermc.paper.math.Position p1, int p2) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isOwnedByCurrentRegion", new Class<?>[]{ org.bukkit.World.class, io.papermc.paper.math.Position.class, int.class }, new Object[]{ p0, p1, p2 }, boolean.class);
    }

    @Override public boolean isOwnedByCurrentRegion(org.bukkit.Location p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isOwnedByCurrentRegion", new Class<?>[]{ org.bukkit.Location.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean isOwnedByCurrentRegion(org.bukkit.World p0, io.papermc.paper.math.Position p1) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isOwnedByCurrentRegion", new Class<?>[]{ org.bukkit.World.class, io.papermc.paper.math.Position.class }, new Object[]{ p0, p1 }, boolean.class);
    }

    @Override public boolean isOwnedByCurrentRegion(org.bukkit.Location p0, int p1) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isOwnedByCurrentRegion", new Class<?>[]{ org.bukkit.Location.class, int.class }, new Object[]{ p0, p1 }, boolean.class);
    }

    @Override public boolean isOwnedByCurrentRegion(org.bukkit.World p0, int p1, int p2) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isOwnedByCurrentRegion", new Class<?>[]{ org.bukkit.World.class, int.class, int.class }, new Object[]{ p0, p1, p2 }, boolean.class);
    }

    @Override public boolean isOwnedByCurrentRegion(org.bukkit.entity.Entity p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isOwnedByCurrentRegion", new Class<?>[]{ org.bukkit.entity.Entity.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean getGenerateStructures() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "getGenerateStructures", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public int getMaxWorldSize() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getMaxWorldSize", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public boolean getAllowEnd() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "getAllowEnd", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean getAllowNether() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "getAllowNether", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public int broadcastMessage(java.lang.String p0) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "broadcastMessage", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, int.class)).intValue();
    }

    @Override public void sendRichMessage(java.lang.String p0) {
        PlayerCalls.call(this.handle, "sendRichMessage", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendRichMessage(java.lang.String p0, net.kyori.adventure.text.minimessage.tag.resolver.TagResolver[] p1) {
        PlayerCalls.call(this.handle, "sendRichMessage", new Class<?>[]{ java.lang.String.class, net.kyori.adventure.text.minimessage.tag.resolver.TagResolver[].class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public java.lang.String getResourcePackHash() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getResourcePackHash", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public org.bukkit.packs.ResourcePack getServerResourcePack() {
        return (org.bukkit.packs.ResourcePack) PlayerCalls.call(this.handle, "getServerResourcePack", new Class<?>[0], new Object[]{  }, org.bukkit.packs.ResourcePack.class);
    }

    @Override public java.lang.String getResourcePack() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getResourcePack", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public boolean isLoggingIPs() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isLoggingIPs", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public java.util.List getInitialDisabledPacks() {
        return (java.util.List) PlayerCalls.call(this.handle, "getInitialDisabledPacks", new Class<?>[0], new Object[]{  }, java.util.List.class);
    }

    @Override public org.bukkit.ServerTickManager getServerTickManager() {
        return (org.bukkit.ServerTickManager) PlayerCalls.call(this.handle, "getServerTickManager", new Class<?>[0], new Object[]{  }, org.bukkit.ServerTickManager.class);
    }

    @Override public java.lang.String getResourcePackPrompt() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getResourcePackPrompt", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public void setWhitelist(boolean p0) {
        PlayerCalls.call(this.handle, "setWhitelist", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean isWhitelistEnforced() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isWhitelistEnforced", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void setWhitelistEnforced(boolean p0) {
        PlayerCalls.call(this.handle, "setWhitelistEnforced", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean isResourcePackRequired() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isResourcePackRequired", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean hasWhitelist() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasWhitelist", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public java.util.Set getWhitelistedPlayers() {
        return (java.util.Set) PlayerCalls.call(this.handle, "getWhitelistedPlayers", new Class<?>[0], new Object[]{  }, java.util.Set.class);
    }

    @Override public void reloadWhitelist() {
        PlayerCalls.call(this.handle, "reloadWhitelist", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public java.util.List getInitialEnabledPacks() {
        return (java.util.List) PlayerCalls.call(this.handle, "getInitialEnabledPacks", new Class<?>[0], new Object[]{  }, java.util.List.class);
    }

    @Override public int getMaxChainedNeighborUpdates() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getMaxChainedNeighborUpdates", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public int getTicksPerWaterUndergroundCreatureSpawns() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getTicksPerWaterUndergroundCreatureSpawns", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public boolean suggestPlayerNamesWhenNullTabCompletions() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "suggestPlayerNamesWhenNullTabCompletions", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public int getWaterUndergroundCreatureSpawnLimit() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getWaterUndergroundCreatureSpawnLimit", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public int getTicksPerWaterAmbientSpawns() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getTicksPerWaterAmbientSpawns", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.Server.Spigot spigot() {
        return (org.bukkit.Server.Spigot) PlayerCalls.call(this.handle, "spigot", new Class<?>[0], new Object[]{  }, org.bukkit.Server.Spigot.class);
    }

    @Override public java.util.List getWorlds() {
        return (java.util.List) PlayerCalls.call(this.handle, "getWorlds", new Class<?>[0], new Object[]{  }, java.util.List.class);
    }

    @Override public void reloadData() {
        PlayerCalls.call(this.handle, "reloadData", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public java.lang.String getIp() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getIp", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public org.bukkit.World getWorld(org.bukkit.NamespacedKey p0) {
        return (org.bukkit.World) PlayerCalls.call(this.handle, "getWorld", new Class<?>[]{ org.bukkit.NamespacedKey.class }, new Object[]{ p0 }, org.bukkit.World.class);
    }

    @Override public org.bukkit.World getWorld(java.util.UUID p0) {
        return (org.bukkit.World) PlayerCalls.call(this.handle, "getWorld", new Class<?>[]{ java.util.UUID.class }, new Object[]{ p0 }, org.bukkit.World.class);
    }

    @Override public org.bukkit.World getWorld(net.kyori.adventure.key.Key p0) {
        return (org.bukkit.World) PlayerCalls.call(this.handle, "getWorld", new Class<?>[]{ net.kyori.adventure.key.Key.class }, new Object[]{ p0 }, org.bukkit.World.class);
    }

    @Override public org.bukkit.World getWorld(java.lang.String p0) {
        return (org.bukkit.World) PlayerCalls.call(this.handle, "getWorld", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, org.bukkit.World.class);
    }

    @Override public org.bukkit.entity.Player getPlayer(java.util.UUID p0) {
        return (org.bukkit.entity.Player) PlayerCalls.call(this.handle, "getPlayer", new Class<?>[]{ java.util.UUID.class }, new Object[]{ p0 }, org.bukkit.entity.Player.class);
    }

    @Override public org.bukkit.entity.Player getPlayer(java.lang.String p0) {
        return (org.bukkit.entity.Player) PlayerCalls.call(this.handle, "getPlayer", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, org.bukkit.entity.Player.class);
    }

    @Override public boolean addRecipe(org.bukkit.inventory.Recipe p0, boolean p1) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "addRecipe", new Class<?>[]{ org.bukkit.inventory.Recipe.class, boolean.class }, new Object[]{ p0, p1 }, boolean.class);
    }

    @Override public boolean addRecipe(org.bukkit.inventory.Recipe p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "addRecipe", new Class<?>[]{ org.bukkit.inventory.Recipe.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public void motd(net.kyori.adventure.text.Component p0) {
        PlayerCalls.call(this.handle, "motd", new Class<?>[]{ net.kyori.adventure.text.Component.class }, new Object[]{ p0 }, void.class);
    }

    @Override public net.kyori.adventure.text.Component motd() {
        return (net.kyori.adventure.text.Component) PlayerCalls.call(this.handle, "motd", new Class<?>[0], new Object[]{  }, net.kyori.adventure.text.Component.class);
    }

    @Override public double[] getTPS() {
        return (double[]) PlayerCalls.call(this.handle, "getTPS", new Class<?>[0], new Object[]{  }, double[].class);
    }

    @Override public boolean isHardcore() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isHardcore", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void banIP(java.net.InetAddress p0) {
        PlayerCalls.call(this.handle, "banIP", new Class<?>[]{ java.net.InetAddress.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void banIP(java.lang.String p0) {
        PlayerCalls.call(this.handle, "banIP", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public java.lang.String getMotd() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getMotd", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public void setMotd(java.lang.String p0) {
        PlayerCalls.call(this.handle, "setMotd", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void unbanIP(java.net.InetAddress p0) {
        PlayerCalls.call(this.handle, "unbanIP", new Class<?>[]{ java.net.InetAddress.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void unbanIP(java.lang.String p0) {
        PlayerCalls.call(this.handle, "unbanIP", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.BanList getBanList(org.bukkit.BanList.Type p0) {
        return (org.bukkit.BanList) PlayerCalls.call(this.handle, "getBanList", new Class<?>[]{ org.bukkit.BanList.Type.class }, new Object[]{ p0 }, org.bukkit.BanList.class);
    }

    @Override public org.bukkit.BanList getBanList(io.papermc.paper.ban.BanListType p0) {
        return (org.bukkit.BanList) PlayerCalls.call(this.handle, "getBanList", new Class<?>[]{ io.papermc.paper.ban.BanListType.class }, new Object[]{ p0 }, org.bukkit.BanList.class);
    }

    @Override public void restart() {
        PlayerCalls.call(this.handle, "restart", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public java.util.Set getIPBans() {
        return (java.util.Set) PlayerCalls.call(this.handle, "getIPBans", new Class<?>[0], new Object[]{  }, java.util.Set.class);
    }

    @Override public boolean isPaused() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isPaused", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.help.HelpMap getHelpMap() {
        return (org.bukkit.help.HelpMap) PlayerCalls.call(this.handle, "getHelpMap", new Class<?>[0], new Object[]{  }, org.bukkit.help.HelpMap.class);
    }

    @Override public org.bukkit.entity.Entity getEntity(java.util.UUID p0) {
        return (org.bukkit.entity.Entity) PlayerCalls.call(this.handle, "getEntity", new Class<?>[]{ java.util.UUID.class }, new Object[]{ p0 }, org.bukkit.entity.Entity.class);
    }

    @Override public org.bukkit.inventory.ItemStack craftItem(org.bukkit.inventory.ItemStack[] p0, org.bukkit.World p1) {
        return (org.bukkit.inventory.ItemStack) PlayerCalls.call(this.handle, "craftItem", new Class<?>[]{ org.bukkit.inventory.ItemStack[].class, org.bukkit.World.class }, new Object[]{ p0, p1 }, org.bukkit.inventory.ItemStack.class);
    }

    @Override public org.bukkit.inventory.ItemStack craftItem(org.bukkit.inventory.ItemStack[] p0, org.bukkit.World p1, org.bukkit.entity.Player p2) {
        return (org.bukkit.inventory.ItemStack) PlayerCalls.call(this.handle, "craftItem", new Class<?>[]{ org.bukkit.inventory.ItemStack[].class, org.bukkit.World.class, org.bukkit.entity.Player.class }, new Object[]{ p0, p1, p2 }, org.bukkit.inventory.ItemStack.class);
    }

    @Override public java.lang.Iterable getTags(java.lang.String p0, java.lang.Class p1) {
        return (java.lang.Iterable) PlayerCalls.call(this.handle, "getTags", new Class<?>[]{ java.lang.String.class, java.lang.Class.class }, new Object[]{ p0, p1 }, java.lang.Iterable.class);
    }

    @Override public org.bukkit.Tag getTag(java.lang.String p0, org.bukkit.NamespacedKey p1, java.lang.Class p2) {
        return (org.bukkit.Tag) PlayerCalls.call(this.handle, "getTag", new Class<?>[]{ java.lang.String.class, org.bukkit.NamespacedKey.class, java.lang.Class.class }, new Object[]{ p0, p1, p2 }, org.bukkit.Tag.class);
    }

    @Override public org.bukkit.inventory.Recipe getRecipe(org.bukkit.NamespacedKey p0) {
        return (org.bukkit.inventory.Recipe) PlayerCalls.call(this.handle, "getRecipe", new Class<?>[]{ org.bukkit.NamespacedKey.class }, new Object[]{ p0 }, org.bukkit.inventory.Recipe.class);
    }

    @Override public boolean isStopping() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isStopping", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean hasRecipe(org.bukkit.NamespacedKey p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasRecipe", new Class<?>[]{ org.bukkit.NamespacedKey.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public org.bukkit.boss.KeyedBossBar getBossBar(org.bukkit.NamespacedKey p0) {
        return (org.bukkit.boss.KeyedBossBar) PlayerCalls.call(this.handle, "getBossBar", new Class<?>[]{ org.bukkit.NamespacedKey.class }, new Object[]{ p0 }, org.bukkit.boss.KeyedBossBar.class);
    }

    @Override public boolean addRecipes(java.lang.Iterable p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "addRecipes", new Class<?>[]{ java.lang.Iterable.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean isAcceptingTransfers() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isAcceptingTransfers", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean getAllowFlight() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "getAllowFlight", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean unloadWorld(java.lang.String p0, boolean p1) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "unloadWorld", new Class<?>[]{ java.lang.String.class, boolean.class }, new Object[]{ p0, p1 }, boolean.class);
    }

    @Override public boolean unloadWorld(org.bukkit.World p0, boolean p1) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "unloadWorld", new Class<?>[]{ org.bukkit.World.class, boolean.class }, new Object[]{ p0, p1 }, boolean.class);
    }

    @Override public java.util.Set getOperators() {
        return (java.util.Set) PlayerCalls.call(this.handle, "getOperators", new Class<?>[0], new Object[]{  }, java.util.Set.class);
    }

    @Override public java.util.UUID getPlayerUniqueId(java.lang.String p0) {
        return (java.util.UUID) PlayerCalls.call(this.handle, "getPlayerUniqueId", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, java.util.UUID.class);
    }

    @Override public int getTicksPerMonsterSpawns() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getTicksPerMonsterSpawns", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void setRespawnWorld(org.bukkit.World p0) {
        PlayerCalls.call(this.handle, "setRespawnWorld", new Class<?>[]{ org.bukkit.World.class }, new Object[]{ p0 }, void.class);
    }

    @Override public java.util.List getRecipesFor(org.bukkit.inventory.ItemStack p0) {
        return (java.util.List) PlayerCalls.call(this.handle, "getRecipesFor", new Class<?>[]{ org.bukkit.inventory.ItemStack.class }, new Object[]{ p0 }, java.util.List.class);
    }

    @Override public boolean getOnlineMode() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "getOnlineMode", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void clearRecipes() {
        PlayerCalls.call(this.handle, "clearRecipes", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public io.papermc.paper.configuration.ServerConfiguration getServerConfig() {
        return (io.papermc.paper.configuration.ServerConfiguration) PlayerCalls.call(this.handle, "getServerConfig", new Class<?>[0], new Object[]{  }, io.papermc.paper.configuration.ServerConfiguration.class);
    }

    @Override public org.bukkit.GameMode getDefaultGameMode() {
        return (org.bukkit.GameMode) PlayerCalls.call(this.handle, "getDefaultGameMode", new Class<?>[0], new Object[]{  }, org.bukkit.GameMode.class);
    }

    @Override public void setDefaultGameMode(org.bukkit.GameMode p0) {
        PlayerCalls.call(this.handle, "setDefaultGameMode", new Class<?>[]{ org.bukkit.GameMode.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.plugin.ServicesManager getServicesManager() {
        return (org.bukkit.plugin.ServicesManager) PlayerCalls.call(this.handle, "getServicesManager", new Class<?>[0], new Object[]{  }, org.bukkit.plugin.ServicesManager.class);
    }

    @Override public org.bukkit.OfflinePlayer getOfflinePlayerIfCached(java.lang.String p0) {
        return (org.bukkit.OfflinePlayer) PlayerCalls.call(this.handle, "getOfflinePlayerIfCached", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, org.bukkit.OfflinePlayer.class);
    }

    @Override public java.util.List matchPlayer(java.lang.String p0) {
        return (java.util.List) PlayerCalls.call(this.handle, "matchPlayer", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, java.util.List.class);
    }

    @Override public void updateResources() {
        PlayerCalls.call(this.handle, "updateResources", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void resetRecipes() {
        PlayerCalls.call(this.handle, "resetRecipes", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public org.bukkit.World getRespawnWorld() {
        return (org.bukkit.World) PlayerCalls.call(this.handle, "getRespawnWorld", new Class<?>[0], new Object[]{  }, org.bukkit.World.class);
    }

    @Override public void updateRecipes() {
        PlayerCalls.call(this.handle, "updateRecipes", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public org.bukkit.World createWorld(org.bukkit.WorldCreator p0) {
        return (org.bukkit.World) PlayerCalls.call(this.handle, "createWorld", new Class<?>[]{ org.bukkit.WorldCreator.class }, new Object[]{ p0 }, org.bukkit.World.class);
    }

    @Override public org.bukkit.entity.Player getPlayerExact(java.lang.String p0) {
        return (org.bukkit.entity.Player) PlayerCalls.call(this.handle, "getPlayerExact", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, org.bukkit.entity.Player.class);
    }

    @Override public boolean dispatchCommand(org.bukkit.command.CommandSender p0, java.lang.String p1) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "dispatchCommand", new Class<?>[]{ org.bukkit.command.CommandSender.class, java.lang.String.class }, new Object[]{ p0, p1 }, boolean.class);
    }

    @Override public long getConnectionThrottle() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getConnectionThrottle", new Class<?>[0], new Object[]{  }, long.class)).longValue();
    }

    @Override public boolean shouldSendChatPreviews() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "shouldSendChatPreviews", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.WorldBorder createWorldBorder() {
        return (org.bukkit.WorldBorder) PlayerCalls.call(this.handle, "createWorldBorder", new Class<?>[0], new Object[]{  }, org.bukkit.WorldBorder.class);
    }

    @Override public java.io.File getUpdateFolderFile() {
        return (java.io.File) PlayerCalls.call(this.handle, "getUpdateFolderFile", new Class<?>[0], new Object[]{  }, java.io.File.class);
    }

    @Override public int getTicksPerWaterSpawns() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getTicksPerWaterSpawns", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void savePlayers() {
        PlayerCalls.call(this.handle, "savePlayers", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public org.bukkit.command.PluginCommand getPluginCommand(java.lang.String p0) {
        return (org.bukkit.command.PluginCommand) PlayerCalls.call(this.handle, "getPluginCommand", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, org.bukkit.command.PluginCommand.class);
    }

    @Override public org.bukkit.inventory.Recipe getCraftingRecipe(org.bukkit.inventory.ItemStack[] p0, org.bukkit.World p1) {
        return (org.bukkit.inventory.Recipe) PlayerCalls.call(this.handle, "getCraftingRecipe", new Class<?>[]{ org.bukkit.inventory.ItemStack[].class, org.bukkit.World.class }, new Object[]{ p0, p1 }, org.bukkit.inventory.Recipe.class);
    }

    @Override public java.util.Map getCommandAliases() {
        return (java.util.Map) PlayerCalls.call(this.handle, "getCommandAliases", new Class<?>[0], new Object[]{  }, java.util.Map.class);
    }

    @Override public void setSpawnRadius(int p0) {
        PlayerCalls.call(this.handle, "setSpawnRadius", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean isEnforcingSecureProfiles() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isEnforcingSecureProfiles", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public int getTicksPerAnimalSpawns() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getTicksPerAnimalSpawns", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public int getTicksPerAmbientSpawns() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getTicksPerAmbientSpawns", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public boolean isTickingWorlds() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isTickingWorlds", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean getHideOnlinePlayers() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "getHideOnlinePlayers", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.scheduler.BukkitScheduler getScheduler() {
        return (org.bukkit.scheduler.BukkitScheduler) PlayerCalls.call(this.handle, "getScheduler", new Class<?>[0], new Object[]{  }, org.bukkit.scheduler.BukkitScheduler.class);
    }

    @Override public org.bukkit.OfflinePlayer getOfflinePlayer(java.util.UUID p0) {
        return (org.bukkit.OfflinePlayer) PlayerCalls.call(this.handle, "getOfflinePlayer", new Class<?>[]{ java.util.UUID.class }, new Object[]{ p0 }, org.bukkit.OfflinePlayer.class);
    }

    @Override public org.bukkit.OfflinePlayer getOfflinePlayer(java.lang.String p0) {
        return (org.bukkit.OfflinePlayer) PlayerCalls.call(this.handle, "getOfflinePlayer", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, org.bukkit.OfflinePlayer.class);
    }

    @Override public org.bukkit.profile.PlayerProfile createPlayerProfile(java.lang.String p0) {
        return (org.bukkit.profile.PlayerProfile) PlayerCalls.call(this.handle, "createPlayerProfile", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, org.bukkit.profile.PlayerProfile.class);
    }

    @Override public org.bukkit.profile.PlayerProfile createPlayerProfile(java.util.UUID p0) {
        return (org.bukkit.profile.PlayerProfile) PlayerCalls.call(this.handle, "createPlayerProfile", new Class<?>[]{ java.util.UUID.class }, new Object[]{ p0 }, org.bukkit.profile.PlayerProfile.class);
    }

    @Override public org.bukkit.profile.PlayerProfile createPlayerProfile(java.util.UUID p0, java.lang.String p1) {
        return (org.bukkit.profile.PlayerProfile) PlayerCalls.call(this.handle, "createPlayerProfile", new Class<?>[]{ java.util.UUID.class, java.lang.String.class }, new Object[]{ p0, p1 }, org.bukkit.profile.PlayerProfile.class);
    }

    @Override public java.util.Set getBannedPlayers() {
        return (java.util.Set) PlayerCalls.call(this.handle, "getBannedPlayers", new Class<?>[0], new Object[]{  }, java.util.Set.class);
    }

    @Override public java.util.Iterator recipeIterator() {
        return (java.util.Iterator) PlayerCalls.call(this.handle, "recipeIterator", new Class<?>[0], new Object[]{  }, java.util.Iterator.class);
    }

    @Override public java.lang.String getUpdateFolder() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getUpdateFolder", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public boolean removeRecipes(java.lang.Iterable p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "removeRecipes", new Class<?>[]{ java.lang.Iterable.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public int getSpawnRadius() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getSpawnRadius", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.plugin.PluginManager getPluginManager() {
        return (org.bukkit.plugin.PluginManager) PlayerCalls.call(this.handle, "getPluginManager", new Class<?>[0], new Object[]{  }, org.bukkit.plugin.PluginManager.class);
    }

    @Override public org.bukkit.potion.PotionBrewer getPotionBrewer() {
        return (org.bukkit.potion.PotionBrewer) PlayerCalls.call(this.handle, "getPotionBrewer", new Class<?>[0], new Object[]{  }, org.bukkit.potion.PotionBrewer.class);
    }

    @Override public net.kyori.adventure.text.Component shutdownMessage() {
        return (net.kyori.adventure.text.Component) PlayerCalls.call(this.handle, "shutdownMessage", new Class<?>[0], new Object[]{  }, net.kyori.adventure.text.Component.class);
    }

    @Override public java.util.Iterator advancementIterator() {
        return (java.util.Iterator) PlayerCalls.call(this.handle, "advancementIterator", new Class<?>[0], new Object[]{  }, java.util.Iterator.class);
    }

    @Override public java.util.List selectEntities(org.bukkit.command.CommandSender p0, java.lang.String p1) {
        return (java.util.List) PlayerCalls.call(this.handle, "selectEntities", new Class<?>[]{ org.bukkit.command.CommandSender.class, java.lang.String.class }, new Object[]{ p0, p1 }, java.util.List.class);
    }

    @Override public org.bukkit.inventory.Merchant createMerchant() {
        return (org.bukkit.inventory.Merchant) PlayerCalls.call(this.handle, "createMerchant", new Class<?>[0], new Object[]{  }, org.bukkit.inventory.Merchant.class);
    }

    @Override public org.bukkit.inventory.Merchant createMerchant(net.kyori.adventure.text.Component p0) {
        return (org.bukkit.inventory.Merchant) PlayerCalls.call(this.handle, "createMerchant", new Class<?>[]{ net.kyori.adventure.text.Component.class }, new Object[]{ p0 }, org.bukkit.inventory.Merchant.class);
    }

    @Override public org.bukkit.inventory.Merchant createMerchant(java.lang.String p0) {
        return (org.bukkit.inventory.Merchant) PlayerCalls.call(this.handle, "createMerchant", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, org.bukkit.inventory.Merchant.class);
    }

    @Override public int getWaterAnimalSpawnLimit() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getWaterAnimalSpawnLimit", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public com.destroystokyo.paper.profile.PlayerProfile createProfileExact(java.util.UUID p0, java.lang.String p1) {
        return (com.destroystokyo.paper.profile.PlayerProfile) PlayerCalls.call(this.handle, "createProfileExact", new Class<?>[]{ java.util.UUID.class, java.lang.String.class }, new Object[]{ p0, p1 }, com.destroystokyo.paper.profile.PlayerProfile.class);
    }

    @Override public java.io.File getWorldContainer() {
        return (java.io.File) PlayerCalls.call(this.handle, "getWorldContainer", new Class<?>[0], new Object[]{  }, java.io.File.class);
    }

    @Override public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder p0, int p1, java.lang.String p2) {
        return (org.bukkit.inventory.Inventory) PlayerCalls.call(this.handle, "createInventory", new Class<?>[]{ org.bukkit.inventory.InventoryHolder.class, int.class, java.lang.String.class }, new Object[]{ p0, p1, p2 }, org.bukkit.inventory.Inventory.class);
    }

    @Override public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder p0, org.bukkit.event.inventory.InventoryType p1, java.lang.String p2) {
        return (org.bukkit.inventory.Inventory) PlayerCalls.call(this.handle, "createInventory", new Class<?>[]{ org.bukkit.inventory.InventoryHolder.class, org.bukkit.event.inventory.InventoryType.class, java.lang.String.class }, new Object[]{ p0, p1, p2 }, org.bukkit.inventory.Inventory.class);
    }

    @Override public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder p0, int p1, net.kyori.adventure.text.Component p2) {
        return (org.bukkit.inventory.Inventory) PlayerCalls.call(this.handle, "createInventory", new Class<?>[]{ org.bukkit.inventory.InventoryHolder.class, int.class, net.kyori.adventure.text.Component.class }, new Object[]{ p0, p1, p2 }, org.bukkit.inventory.Inventory.class);
    }

    @Override public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder p0, org.bukkit.event.inventory.InventoryType p1, net.kyori.adventure.text.Component p2) {
        return (org.bukkit.inventory.Inventory) PlayerCalls.call(this.handle, "createInventory", new Class<?>[]{ org.bukkit.inventory.InventoryHolder.class, org.bukkit.event.inventory.InventoryType.class, net.kyori.adventure.text.Component.class }, new Object[]{ p0, p1, p2 }, org.bukkit.inventory.Inventory.class);
    }

    @Override public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder p0, int p1) {
        return (org.bukkit.inventory.Inventory) PlayerCalls.call(this.handle, "createInventory", new Class<?>[]{ org.bukkit.inventory.InventoryHolder.class, int.class }, new Object[]{ p0, p1 }, org.bukkit.inventory.Inventory.class);
    }

    @Override public org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder p0, org.bukkit.event.inventory.InventoryType p1) {
        return (org.bukkit.inventory.Inventory) PlayerCalls.call(this.handle, "createInventory", new Class<?>[]{ org.bukkit.inventory.InventoryHolder.class, org.bukkit.event.inventory.InventoryType.class }, new Object[]{ p0, p1 }, org.bukkit.inventory.Inventory.class);
    }

    @Override public org.bukkit.scoreboard.ScoreboardManager getScoreboardManager() {
        return (org.bukkit.scoreboard.ScoreboardManager) PlayerCalls.call(this.handle, "getScoreboardManager", new Class<?>[0], new Object[]{  }, org.bukkit.scoreboard.ScoreboardManager.class);
    }

    @Override public long[] getTickTimes() {
        return (long[]) PlayerCalls.call(this.handle, "getTickTimes", new Class<?>[0], new Object[]{  }, long[].class);
    }

    @Override public org.bukkit.Registry getRegistry(java.lang.Class p0) {
        return (org.bukkit.Registry) PlayerCalls.call(this.handle, "getRegistry", new Class<?>[]{ java.lang.Class.class }, new Object[]{ p0 }, org.bukkit.Registry.class);
    }

    @Override public java.lang.String getPermissionMessage() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getPermissionMessage", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public org.bukkit.inventory.ItemFactory getItemFactory() {
        return (org.bukkit.inventory.ItemFactory) PlayerCalls.call(this.handle, "getItemFactory", new Class<?>[0], new Object[]{  }, org.bukkit.inventory.ItemFactory.class);
    }

    @Override public io.papermc.paper.datapack.DatapackManager getDatapackManager() {
        return (io.papermc.paper.datapack.DatapackManager) PlayerCalls.call(this.handle, "getDatapackManager", new Class<?>[0], new Object[]{  }, io.papermc.paper.datapack.DatapackManager.class);
    }

    @Override public int getWaterAmbientSpawnLimit() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getWaterAmbientSpawnLimit", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.util.CachedServerIcon loadServerIcon(java.awt.image.BufferedImage p0) {
        return (org.bukkit.util.CachedServerIcon) PlayerCalls.call(this.handle, "loadServerIcon", new Class<?>[]{ java.awt.image.BufferedImage.class }, new Object[]{ p0 }, org.bukkit.util.CachedServerIcon.class);
    }

    @Override public org.bukkit.util.CachedServerIcon loadServerIcon(java.io.File p0) {
        return (org.bukkit.util.CachedServerIcon) PlayerCalls.call(this.handle, "loadServerIcon", new Class<?>[]{ java.io.File.class }, new Object[]{ p0 }, org.bukkit.util.CachedServerIcon.class);
    }

    @Override public org.bukkit.ServerLinks getServerLinks() {
        return (org.bukkit.ServerLinks) PlayerCalls.call(this.handle, "getServerLinks", new Class<?>[0], new Object[]{  }, org.bukkit.ServerLinks.class);
    }

    @Override public org.bukkit.block.data.BlockData createBlockData(java.lang.String p0) {
        return (org.bukkit.block.data.BlockData) PlayerCalls.call(this.handle, "createBlockData", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, org.bukkit.block.data.BlockData.class);
    }

    @Override public org.bukkit.block.data.BlockData createBlockData(org.bukkit.Material p0) {
        return (org.bukkit.block.data.BlockData) PlayerCalls.call(this.handle, "createBlockData", new Class<?>[]{ org.bukkit.Material.class }, new Object[]{ p0 }, org.bukkit.block.data.BlockData.class);
    }

    @Override public org.bukkit.block.data.BlockData createBlockData(org.bukkit.Material p0, java.util.function.Consumer p1) {
        return (org.bukkit.block.data.BlockData) PlayerCalls.call(this.handle, "createBlockData", new Class<?>[]{ org.bukkit.Material.class, java.util.function.Consumer.class }, new Object[]{ p0, p1 }, org.bukkit.block.data.BlockData.class);
    }

    @Override public org.bukkit.block.data.BlockData createBlockData(org.bukkit.Material p0, java.lang.String p1) {
        return (org.bukkit.block.data.BlockData) PlayerCalls.call(this.handle, "createBlockData", new Class<?>[]{ org.bukkit.Material.class, java.lang.String.class }, new Object[]{ p0, p1 }, org.bukkit.block.data.BlockData.class);
    }

    @Override public java.lang.String getShutdownMessage() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getShutdownMessage", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public org.bukkit.Warning.WarningState getWarningState() {
        return (org.bukkit.Warning.WarningState) PlayerCalls.call(this.handle, "getWarningState", new Class<?>[0], new Object[]{  }, org.bukkit.Warning.WarningState.class);
    }

    @Override public int getMonsterSpawnLimit() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getMonsterSpawnLimit", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public boolean forcesDefaultGameMode() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "forcesDefaultGameMode", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.plugin.messaging.Messenger getMessenger() {
        return (org.bukkit.plugin.messaging.Messenger) PlayerCalls.call(this.handle, "getMessenger", new Class<?>[0], new Object[]{  }, org.bukkit.plugin.messaging.Messenger.class);
    }

    @Override public boolean isPrimaryThread() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isPrimaryThread", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.util.CachedServerIcon getServerIcon() {
        return (org.bukkit.util.CachedServerIcon) PlayerCalls.call(this.handle, "getServerIcon", new Class<?>[0], new Object[]{  }, org.bukkit.util.CachedServerIcon.class);
    }

    @Override public void setIdleTimeout(int p0) {
        PlayerCalls.call(this.handle, "setIdleTimeout", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getPauseWhenEmptyTime() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getPauseWhenEmptyTime", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public java.nio.file.Path getLevelDirectory() {
        return (java.nio.file.Path) PlayerCalls.call(this.handle, "getLevelDirectory", new Class<?>[0], new Object[]{  }, java.nio.file.Path.class);
    }

    @Override public org.bukkit.OfflinePlayer[] getOfflinePlayers() {
        return (org.bukkit.OfflinePlayer[]) PlayerCalls.call(this.handle, "getOfflinePlayers", new Class<?>[0], new Object[]{  }, org.bukkit.OfflinePlayer[].class);
    }

    @Override public int getIdleTimeout() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getIdleTimeout", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.generator.ChunkGenerator.ChunkData createChunkData(org.bukkit.World p0) {
        return (org.bukkit.generator.ChunkGenerator.ChunkData) PlayerCalls.call(this.handle, "createChunkData", new Class<?>[]{ org.bukkit.World.class }, new Object[]{ p0 }, org.bukkit.generator.ChunkGenerator.ChunkData.class);
    }

    @Override public org.bukkit.boss.BossBar createBossBar(java.lang.String p0, org.bukkit.boss.BarColor p1, org.bukkit.boss.BarStyle p2, org.bukkit.boss.BarFlag[] p3) {
        return (org.bukkit.boss.BossBar) PlayerCalls.call(this.handle, "createBossBar", new Class<?>[]{ java.lang.String.class, org.bukkit.boss.BarColor.class, org.bukkit.boss.BarStyle.class, org.bukkit.boss.BarFlag[].class }, new Object[]{ p0, p1, p2, p3 }, org.bukkit.boss.BossBar.class);
    }

    @Override public org.bukkit.boss.KeyedBossBar createBossBar(org.bukkit.NamespacedKey p0, java.lang.String p1, org.bukkit.boss.BarColor p2, org.bukkit.boss.BarStyle p3, org.bukkit.boss.BarFlag[] p4) {
        return (org.bukkit.boss.KeyedBossBar) PlayerCalls.call(this.handle, "createBossBar", new Class<?>[]{ org.bukkit.NamespacedKey.class, java.lang.String.class, org.bukkit.boss.BarColor.class, org.bukkit.boss.BarStyle.class, org.bukkit.boss.BarFlag[].class }, new Object[]{ p0, p1, p2, p3, p4 }, org.bukkit.boss.KeyedBossBar.class);
    }

    @Override public boolean removeBossBar(org.bukkit.NamespacedKey p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "removeBossBar", new Class<?>[]{ org.bukkit.NamespacedKey.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public org.bukkit.command.ConsoleCommandSender getConsoleSender() {
        return (org.bukkit.command.ConsoleCommandSender) PlayerCalls.call(this.handle, "getConsoleSender", new Class<?>[0], new Object[]{  }, org.bukkit.command.ConsoleCommandSender.class);
    }

    @Override public double getAverageTickTime() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getAverageTickTime", new Class<?>[0], new Object[]{  }, double.class)).doubleValue();
    }

    @Override public java.util.Iterator getBossBars() {
        return (java.util.Iterator) PlayerCalls.call(this.handle, "getBossBars", new Class<?>[0], new Object[]{  }, java.util.Iterator.class);
    }

    @Override public org.bukkit.structure.StructureManager getStructureManager() {
        return (org.bukkit.structure.StructureManager) PlayerCalls.call(this.handle, "getStructureManager", new Class<?>[0], new Object[]{  }, org.bukkit.structure.StructureManager.class);
    }

    @Override public void reloadPermissions() {
        PlayerCalls.call(this.handle, "reloadPermissions", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public boolean reloadCommandAliases() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "reloadCommandAliases", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void setPauseWhenEmptyTime(int p0) {
        PlayerCalls.call(this.handle, "setPauseWhenEmptyTime", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getAmbientSpawnLimit() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getAmbientSpawnLimit", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.scoreboard.Criteria getScoreboardCriteria(java.lang.String p0) {
        return (org.bukkit.scoreboard.Criteria) PlayerCalls.call(this.handle, "getScoreboardCriteria", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, org.bukkit.scoreboard.Criteria.class);
    }

    @Override public net.kyori.adventure.text.Component permissionMessage() {
        return (net.kyori.adventure.text.Component) PlayerCalls.call(this.handle, "permissionMessage", new Class<?>[0], new Object[]{  }, net.kyori.adventure.text.Component.class);
    }

    @Override public com.destroystokyo.paper.profile.PlayerProfile createProfile(java.lang.String p0) {
        return (com.destroystokyo.paper.profile.PlayerProfile) PlayerCalls.call(this.handle, "createProfile", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, com.destroystokyo.paper.profile.PlayerProfile.class);
    }

    @Override public com.destroystokyo.paper.profile.PlayerProfile createProfile(java.util.UUID p0, java.lang.String p1) {
        return (com.destroystokyo.paper.profile.PlayerProfile) PlayerCalls.call(this.handle, "createProfile", new Class<?>[]{ java.util.UUID.class, java.lang.String.class }, new Object[]{ p0, p1 }, com.destroystokyo.paper.profile.PlayerProfile.class);
    }

    @Override public com.destroystokyo.paper.profile.PlayerProfile createProfile(java.util.UUID p0) {
        return (com.destroystokyo.paper.profile.PlayerProfile) PlayerCalls.call(this.handle, "createProfile", new Class<?>[]{ java.util.UUID.class }, new Object[]{ p0 }, com.destroystokyo.paper.profile.PlayerProfile.class);
    }

    @Override public org.bukkit.advancement.Advancement getAdvancement(org.bukkit.NamespacedKey p0) {
        return (org.bukkit.advancement.Advancement) PlayerCalls.call(this.handle, "getAdvancement", new Class<?>[]{ org.bukkit.NamespacedKey.class }, new Object[]{ p0 }, org.bukkit.advancement.Advancement.class);
    }

    @Override public org.bukkit.loot.LootTable getLootTable(org.bukkit.NamespacedKey p0) {
        return (org.bukkit.loot.LootTable) PlayerCalls.call(this.handle, "getLootTable", new Class<?>[]{ org.bukkit.NamespacedKey.class }, new Object[]{ p0 }, org.bukkit.loot.LootTable.class);
    }

    @Override public int getCurrentTick() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getCurrentTick", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.command.CommandSender createCommandSender(java.util.function.Consumer p0) {
        return (org.bukkit.command.CommandSender) PlayerCalls.call(this.handle, "createCommandSender", new Class<?>[]{ java.util.function.Consumer.class }, new Object[]{ p0 }, org.bukkit.command.CommandSender.class);
    }

    @Override public int getAnimalSpawnLimit() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getAnimalSpawnLimit", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public com.destroystokyo.paper.entity.ai.MobGoals getMobGoals() {
        return (com.destroystokyo.paper.entity.ai.MobGoals) PlayerCalls.call(this.handle, "getMobGoals", new Class<?>[0], new Object[]{  }, com.destroystokyo.paper.entity.ai.MobGoals.class);
    }

    @Override public io.papermc.paper.threadedregions.scheduler.RegionScheduler getRegionScheduler() {
        return (io.papermc.paper.threadedregions.scheduler.RegionScheduler) PlayerCalls.call(this.handle, "getRegionScheduler", new Class<?>[0], new Object[]{  }, io.papermc.paper.threadedregions.scheduler.RegionScheduler.class);
    }

    @Override public io.papermc.paper.threadedregions.scheduler.AsyncScheduler getAsyncScheduler() {
        return (io.papermc.paper.threadedregions.scheduler.AsyncScheduler) PlayerCalls.call(this.handle, "getAsyncScheduler", new Class<?>[0], new Object[]{  }, io.papermc.paper.threadedregions.scheduler.AsyncScheduler.class);
    }

    @Override public org.bukkit.entity.EntityFactory getEntityFactory() {
        return (org.bukkit.entity.EntityFactory) PlayerCalls.call(this.handle, "getEntityFactory", new Class<?>[0], new Object[]{  }, org.bukkit.entity.EntityFactory.class);
    }

    @Override public org.bukkit.command.CommandMap getCommandMap() {
        return (org.bukkit.command.CommandMap) PlayerCalls.call(this.handle, "getCommandMap", new Class<?>[0], new Object[]{  }, org.bukkit.command.CommandMap.class);
    }

    @Override public io.papermc.paper.threadedregions.scheduler.GlobalRegionScheduler getGlobalRegionScheduler() {
        return (io.papermc.paper.threadedregions.scheduler.GlobalRegionScheduler) PlayerCalls.call(this.handle, "getGlobalRegionScheduler", new Class<?>[0], new Object[]{  }, io.papermc.paper.threadedregions.scheduler.GlobalRegionScheduler.class);
    }

    @Override public boolean isGlobalTickThread() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isGlobalTickThread", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void allowPausing(org.bukkit.plugin.Plugin p0, boolean p1) {
        PlayerCalls.call(this.handle, "allowPausing", new Class<?>[]{ org.bukkit.plugin.Plugin.class, boolean.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendPluginMessage(org.bukkit.plugin.Plugin p0, java.lang.String p1, byte[] p2) {
        PlayerCalls.call(this.handle, "sendPluginMessage", new Class<?>[]{ org.bukkit.plugin.Plugin.class, java.lang.String.class, byte[].class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public java.util.Set getListeningPluginChannels() {
        return (java.util.Set) PlayerCalls.call(this.handle, "getListeningPluginChannels", new Class<?>[0], new Object[]{  }, java.util.Set.class);
    }

    @Override public void sendMessage(net.kyori.adventure.text.Component p0, net.kyori.adventure.chat.ChatType.Bound p1) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ net.kyori.adventure.text.Component.class, net.kyori.adventure.chat.ChatType.Bound.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendMessage(net.kyori.adventure.text.Component p0) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ net.kyori.adventure.text.Component.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendMessage(net.kyori.adventure.chat.SignedMessage p0, net.kyori.adventure.chat.ChatType.Bound p1) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ net.kyori.adventure.chat.SignedMessage.class, net.kyori.adventure.chat.ChatType.Bound.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public java.lang.Iterable audiences() {
        return (java.lang.Iterable) PlayerCalls.call(this.handle, "audiences", new Class<?>[0], new Object[]{  }, java.lang.Iterable.class);
    }

    @Override public void resetTitle() {
        PlayerCalls.call(this.handle, "resetTitle", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void openBook(net.kyori.adventure.inventory.Book p0) {
        PlayerCalls.call(this.handle, "openBook", new Class<?>[]{ net.kyori.adventure.inventory.Book.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void clearTitle() {
        PlayerCalls.call(this.handle, "clearTitle", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void showDialog(net.kyori.adventure.dialog.DialogLike p0) {
        PlayerCalls.call(this.handle, "showDialog", new Class<?>[]{ net.kyori.adventure.dialog.DialogLike.class }, new Object[]{ p0 }, void.class);
    }

    @Override public net.kyori.adventure.pointer.Pointers pointers() {
        return (net.kyori.adventure.pointer.Pointers) PlayerCalls.call(this.handle, "pointers", new Class<?>[0], new Object[]{  }, net.kyori.adventure.pointer.Pointers.class);
    }

    @Override public void playSound(net.kyori.adventure.sound.Sound p0) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ net.kyori.adventure.sound.Sound.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void playSound(net.kyori.adventure.sound.Sound p0, double p1, double p2, double p3) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ net.kyori.adventure.sound.Sound.class, double.class, double.class, double.class }, new Object[]{ p0, p1, p2, p3 }, void.class);
    }

    @Override public void playSound(net.kyori.adventure.sound.Sound p0, net.kyori.adventure.sound.Sound.Emitter p1) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ net.kyori.adventure.sound.Sound.class, net.kyori.adventure.sound.Sound.Emitter.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void stopSound(net.kyori.adventure.sound.SoundStop p0) {
        PlayerCalls.call(this.handle, "stopSound", new Class<?>[]{ net.kyori.adventure.sound.SoundStop.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendTitlePart(net.kyori.adventure.title.TitlePart p0, java.lang.Object p1) {
        PlayerCalls.call(this.handle, "sendTitlePart", new Class<?>[]{ net.kyori.adventure.title.TitlePart.class, java.lang.Object.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void closeDialog() {
        PlayerCalls.call(this.handle, "closeDialog", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void clearResourcePacks() {
        PlayerCalls.call(this.handle, "clearResourcePacks", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void sendActionBar(net.kyori.adventure.text.Component p0) {
        PlayerCalls.call(this.handle, "sendActionBar", new Class<?>[]{ net.kyori.adventure.text.Component.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void forEachAudience(java.util.function.Consumer p0) {
        PlayerCalls.call(this.handle, "forEachAudience", new Class<?>[]{ java.util.function.Consumer.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendPlayerListFooter(net.kyori.adventure.text.Component p0) {
        PlayerCalls.call(this.handle, "sendPlayerListFooter", new Class<?>[]{ net.kyori.adventure.text.Component.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendResourcePacks(net.kyori.adventure.resource.ResourcePackRequest p0) {
        PlayerCalls.call(this.handle, "sendResourcePacks", new Class<?>[]{ net.kyori.adventure.resource.ResourcePackRequest.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendPlayerListHeader(net.kyori.adventure.text.Component p0) {
        PlayerCalls.call(this.handle, "sendPlayerListHeader", new Class<?>[]{ net.kyori.adventure.text.Component.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void hideBossBar(net.kyori.adventure.bossbar.BossBar p0) {
        PlayerCalls.call(this.handle, "hideBossBar", new Class<?>[]{ net.kyori.adventure.bossbar.BossBar.class }, new Object[]{ p0 }, void.class);
    }

    @Override public net.kyori.adventure.audience.Audience filterAudience(java.util.function.Predicate p0) {
        return (net.kyori.adventure.audience.Audience) PlayerCalls.call(this.handle, "filterAudience", new Class<?>[]{ java.util.function.Predicate.class }, new Object[]{ p0 }, net.kyori.adventure.audience.Audience.class);
    }

    @Override public void deleteMessage(net.kyori.adventure.chat.SignedMessage.Signature p0) {
        PlayerCalls.call(this.handle, "deleteMessage", new Class<?>[]{ net.kyori.adventure.chat.SignedMessage.Signature.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void removeResourcePacks(java.util.UUID p0, java.util.UUID[] p1) {
        PlayerCalls.call(this.handle, "removeResourcePacks", new Class<?>[]{ java.util.UUID.class, java.util.UUID[].class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void removeResourcePacks(java.lang.Iterable p0) {
        PlayerCalls.call(this.handle, "removeResourcePacks", new Class<?>[]{ java.lang.Iterable.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void showBossBar(net.kyori.adventure.bossbar.BossBar p0) {
        PlayerCalls.call(this.handle, "showBossBar", new Class<?>[]{ net.kyori.adventure.bossbar.BossBar.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendPlayerListHeaderAndFooter(net.kyori.adventure.text.Component p0, net.kyori.adventure.text.Component p1) {
        PlayerCalls.call(this.handle, "sendPlayerListHeaderAndFooter", new Class<?>[]{ net.kyori.adventure.text.Component.class, net.kyori.adventure.text.Component.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendMessage(net.kyori.adventure.text.ComponentLike p0) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ net.kyori.adventure.text.ComponentLike.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendMessage(net.kyori.adventure.text.ComponentLike p0, net.kyori.adventure.chat.ChatType.Bound p1) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ net.kyori.adventure.text.ComponentLike.class, net.kyori.adventure.chat.ChatType.Bound.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void openBook(net.kyori.adventure.inventory.Book.Builder p0) {
        PlayerCalls.call(this.handle, "openBook", new Class<?>[]{ net.kyori.adventure.inventory.Book.Builder.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void openBook(net.kyori.adventure.inventory.BookLike p0) {
        PlayerCalls.call(this.handle, "openBook", new Class<?>[]{ net.kyori.adventure.inventory.BookLike.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void stopSound(net.kyori.adventure.sound.Sound p0) {
        PlayerCalls.call(this.handle, "stopSound", new Class<?>[]{ net.kyori.adventure.sound.Sound.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void showTitle(net.kyori.adventure.title.Title p0) {
        PlayerCalls.call(this.handle, "showTitle", new Class<?>[]{ net.kyori.adventure.title.Title.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendActionBar(net.kyori.adventure.text.ComponentLike p0) {
        PlayerCalls.call(this.handle, "sendActionBar", new Class<?>[]{ net.kyori.adventure.text.ComponentLike.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendPlayerListFooter(net.kyori.adventure.text.ComponentLike p0) {
        PlayerCalls.call(this.handle, "sendPlayerListFooter", new Class<?>[]{ net.kyori.adventure.text.ComponentLike.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendResourcePacks(net.kyori.adventure.resource.ResourcePackInfoLike p0, net.kyori.adventure.resource.ResourcePackInfoLike[] p1) {
        PlayerCalls.call(this.handle, "sendResourcePacks", new Class<?>[]{ net.kyori.adventure.resource.ResourcePackInfoLike.class, net.kyori.adventure.resource.ResourcePackInfoLike[].class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendResourcePacks(net.kyori.adventure.resource.ResourcePackRequestLike p0) {
        PlayerCalls.call(this.handle, "sendResourcePacks", new Class<?>[]{ net.kyori.adventure.resource.ResourcePackRequestLike.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendPlayerListHeader(net.kyori.adventure.text.ComponentLike p0) {
        PlayerCalls.call(this.handle, "sendPlayerListHeader", new Class<?>[]{ net.kyori.adventure.text.ComponentLike.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void deleteMessage(net.kyori.adventure.chat.SignedMessage p0) {
        PlayerCalls.call(this.handle, "deleteMessage", new Class<?>[]{ net.kyori.adventure.chat.SignedMessage.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void removeResourcePacks(net.kyori.adventure.resource.ResourcePackRequestLike p0) {
        PlayerCalls.call(this.handle, "removeResourcePacks", new Class<?>[]{ net.kyori.adventure.resource.ResourcePackRequestLike.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void removeResourcePacks(net.kyori.adventure.resource.ResourcePackRequest p0) {
        PlayerCalls.call(this.handle, "removeResourcePacks", new Class<?>[]{ net.kyori.adventure.resource.ResourcePackRequest.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void removeResourcePacks(net.kyori.adventure.resource.ResourcePackInfoLike p0, net.kyori.adventure.resource.ResourcePackInfoLike[] p1) {
        PlayerCalls.call(this.handle, "removeResourcePacks", new Class<?>[]{ net.kyori.adventure.resource.ResourcePackInfoLike.class, net.kyori.adventure.resource.ResourcePackInfoLike[].class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendPlayerListHeaderAndFooter(net.kyori.adventure.text.ComponentLike p0, net.kyori.adventure.text.ComponentLike p1) {
        PlayerCalls.call(this.handle, "sendPlayerListHeaderAndFooter", new Class<?>[]{ net.kyori.adventure.text.ComponentLike.class, net.kyori.adventure.text.ComponentLike.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public java.util.Optional get(net.kyori.adventure.pointer.Pointer p0) {
        return (java.util.Optional) PlayerCalls.call(this.handle, "get", new Class<?>[]{ net.kyori.adventure.pointer.Pointer.class }, new Object[]{ p0 }, java.util.Optional.class);
    }

    @Override public java.lang.Object getOrDefault(net.kyori.adventure.pointer.Pointer p0, java.lang.Object p1) {
        return (java.lang.Object) PlayerCalls.call(this.handle, "getOrDefault", new Class<?>[]{ net.kyori.adventure.pointer.Pointer.class, java.lang.Object.class }, new Object[]{ p0, p1 }, java.lang.Object.class);
    }

    @Override public java.lang.Object getOrDefaultFrom(net.kyori.adventure.pointer.Pointer p0, java.util.function.Supplier p1) {
        return (java.lang.Object) PlayerCalls.call(this.handle, "getOrDefaultFrom", new Class<?>[]{ net.kyori.adventure.pointer.Pointer.class, java.util.function.Supplier.class }, new Object[]{ p0, p1 }, java.lang.Object.class);
    }

    /**
     * {@code CraftServer.getServer()}: NMS-сервер для Simple Voice Chat/TAB и
     * для Brigadier-диспетчера (NpcPlugin). Как в Paper, тип —
     * {@code DedicatedServer} (ковариантный возврат).
     */
    public net.minecraft.server.dedicated.DedicatedServer getServer() {
        return this.nmsServer;
    }
}
