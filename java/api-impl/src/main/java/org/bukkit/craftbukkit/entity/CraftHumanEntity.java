package org.bukkit.craftbukkit.entity;

import dev.chunkwake.bridge.PlayerCalls;
import org.bukkit.permissions.PermissibleBase;
import org.bukkit.permissions.Permission;
import org.bukkit.permissions.PermissionAttachment;
import org.bukkit.permissions.PermissionAttachmentInfo;

/**
 * Настоящий класс игрока (а не динамический прокси), как {@code CraftPlayer}
 * в CraftBukkit. Нужен плагинам, которые работают с игроком через поле
 * {@code perm} (LuckPerms: {@code PermissibleInjector}).
 *
 * <p>Все методы {@code Player} делегируются в {@code PlayerImpl} через
 * {@link PlayerCalls}; права обслуживает {@link #perm}, который плагин прав
 * может заменить своим {@code Permissible}.
 */
public class CraftHumanEntity extends CraftEntity implements org.bukkit.entity.Player {
    /** Поле, которое ищет LuckPerms (`CraftHumanEntity.perm`). */
    public PermissibleBase perm;
    /** CraftBukkit-поля: их читают NMS-плагины через FakeCraftPlayer. */
    public final org.bukkit.craftbukkit.CraftServer server;
    public final net.minecraft.server.level.ServerPlayer entity;

    /** `false`, пока `PermissibleBase` ещё в своём конструкторе. */
    private boolean ready;

    public CraftHumanEntity(Object handle) {
        this(handle, null, null);
    }

    /** Конструктор как у CraftBukkit: `CraftHumanEntity(CraftServer, Entity)`. */
    public CraftHumanEntity(org.bukkit.craftbukkit.CraftServer server,
            net.minecraft.server.level.ServerPlayer entity) {
        this(entity, server, entity);
    }

    private CraftHumanEntity(Object handle, org.bukkit.craftbukkit.CraftServer server,
            net.minecraft.server.level.ServerPlayer entity) {
        super(handle);
        this.server = server;
        this.entity = entity;
        this.perm = new PermissibleBase(this);
        this.ready = true;
    }

    @Override public boolean hasPermission(String name) { return this.perm.hasPermission(name); }
    @Override public boolean hasPermission(Permission perm) { return this.perm.hasPermission(perm); }
    @Override public boolean isPermissionSet(String name) { return this.perm.isPermissionSet(name); }
    @Override public boolean isPermissionSet(Permission perm) { return this.perm.isPermissionSet(perm); }
    @Override public PermissionAttachment addAttachment(org.bukkit.plugin.Plugin plugin) { return this.perm.addAttachment(plugin); }
    @Override public PermissionAttachment addAttachment(org.bukkit.plugin.Plugin plugin, String name, boolean value) { return this.perm.addAttachment(plugin, name, value); }
    @Override public PermissionAttachment addAttachment(org.bukkit.plugin.Plugin plugin, int ticks) { return this.perm.addAttachment(plugin, ticks); }
    @Override public PermissionAttachment addAttachment(org.bukkit.plugin.Plugin plugin, String name, boolean value, int ticks) { return this.perm.addAttachment(plugin, name, value, ticks); }
    @Override public void removeAttachment(PermissionAttachment attachment) { this.perm.removeAttachment(attachment); }
    @Override public void recalculatePermissions() { this.perm.recalculatePermissions(); }
    @Override public java.util.Set<PermissionAttachmentInfo> getEffectivePermissions() { return this.perm.getEffectivePermissions(); }
    // В CraftBukkit `isOp`/`setOp` идут в handle (сервер), а не в Permissible:
    // иначе `PermissibleBase.<init>` -> `recalculatePermissions` -> `isOp`
    // зациклится на ещё не созданном поле.
    @Override public boolean isOp() {
        if (!this.ready) {
            return false;
        }
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isOp", new Class<?>[0], new Object[0], boolean.class);
    }
    @Override public void setOp(boolean value) {
        PlayerCalls.call(this.handle, "setOp", new Class<?>[]{ boolean.class }, new Object[]{ value }, void.class);
    }

    @Override public java.lang.String getName() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getName", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public java.util.Locale locale() {
        return (java.util.Locale) PlayerCalls.call(this.handle, "locale", new Class<?>[0], new Object[]{  }, java.util.Locale.class);
    }

    @Override public net.kyori.adventure.identity.Identity identity() {
        return (net.kyori.adventure.identity.Identity) PlayerCalls.call(this.handle, "identity", new Class<?>[0], new Object[]{  }, net.kyori.adventure.identity.Identity.class);
    }

    @Override public java.net.InetSocketAddress getAddress() {
        return (java.net.InetSocketAddress) PlayerCalls.call(this.handle, "getAddress", new Class<?>[0], new Object[]{  }, java.net.InetSocketAddress.class);
    }

    @Override public void transfer(java.lang.String p0, int p1) {
        PlayerCalls.call(this.handle, "transfer", new Class<?>[]{ java.lang.String.class, int.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void displayName(net.kyori.adventure.text.Component p0) {
        PlayerCalls.call(this.handle, "displayName", new Class<?>[]{ net.kyori.adventure.text.Component.class }, new Object[]{ p0 }, void.class);
    }

    @Override public net.kyori.adventure.text.Component displayName() {
        return (net.kyori.adventure.text.Component) PlayerCalls.call(this.handle, "displayName", new Class<?>[0], new Object[]{  }, net.kyori.adventure.text.Component.class);
    }

    @Override public java.lang.String getDisplayName() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getDisplayName", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public java.lang.String getLocale() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getLocale", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public void setLevel(int p0) {
        PlayerCalls.call(this.handle, "setLevel", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getLevel() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getLevel", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void hideEntity(org.bukkit.plugin.Plugin p0, org.bukkit.entity.Entity p1) {
        PlayerCalls.call(this.handle, "hideEntity", new Class<?>[]{ org.bukkit.plugin.Plugin.class, org.bukkit.entity.Entity.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void kick() {
        PlayerCalls.call(this.handle, "kick", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void kick(net.kyori.adventure.text.Component p0, org.bukkit.event.player.PlayerKickEvent.Cause p1) {
        PlayerCalls.call(this.handle, "kick", new Class<?>[]{ net.kyori.adventure.text.Component.class, org.bukkit.event.player.PlayerKickEvent.Cause.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void kick(net.kyori.adventure.text.Component p0) {
        PlayerCalls.call(this.handle, "kick", new Class<?>[]{ net.kyori.adventure.text.Component.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.BanEntry banIp(java.lang.String p0, java.time.Duration p1, java.lang.String p2, boolean p3) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banIp", new Class<?>[]{ java.lang.String.class, java.time.Duration.class, java.lang.String.class, boolean.class }, new Object[]{ p0, p1, p2, p3 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banIp(java.lang.String p0, java.time.Instant p1, java.lang.String p2, boolean p3) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banIp", new Class<?>[]{ java.lang.String.class, java.time.Instant.class, java.lang.String.class, boolean.class }, new Object[]{ p0, p1, p2, p3 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banIp(java.lang.String p0, java.util.Date p1, java.lang.String p2, boolean p3) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banIp", new Class<?>[]{ java.lang.String.class, java.util.Date.class, java.lang.String.class, boolean.class }, new Object[]{ p0, p1, p2, p3 }, org.bukkit.BanEntry.class);
    }

    @Override public boolean isOnGround() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isOnGround", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void playNote(org.bukkit.Location p0, org.bukkit.Instrument p1, org.bukkit.Note p2) {
        PlayerCalls.call(this.handle, "playNote", new Class<?>[]{ org.bukkit.Location.class, org.bukkit.Instrument.class, org.bukkit.Note.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void playNote(org.bukkit.Location p0, byte p1, byte p2) {
        PlayerCalls.call(this.handle, "playNote", new Class<?>[]{ org.bukkit.Location.class, byte.class, byte.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void saveData() {
        PlayerCalls.call(this.handle, "saveData", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void stopSound(java.lang.String p0) {
        PlayerCalls.call(this.handle, "stopSound", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void stopSound(org.bukkit.SoundCategory p0) {
        PlayerCalls.call(this.handle, "stopSound", new Class<?>[]{ org.bukkit.SoundCategory.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void stopSound(org.bukkit.Sound p0) {
        PlayerCalls.call(this.handle, "stopSound", new Class<?>[]{ org.bukkit.Sound.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void stopSound(org.bukkit.Sound p0, org.bukkit.SoundCategory p1) {
        PlayerCalls.call(this.handle, "stopSound", new Class<?>[]{ org.bukkit.Sound.class, org.bukkit.SoundCategory.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void stopSound(java.lang.String p0, org.bukkit.SoundCategory p1) {
        PlayerCalls.call(this.handle, "stopSound", new Class<?>[]{ java.lang.String.class, org.bukkit.SoundCategory.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public io.papermc.paper.entity.PlayerGiveResult give(org.bukkit.inventory.ItemStack[] p0) {
        return (io.papermc.paper.entity.PlayerGiveResult) PlayerCalls.call(this.handle, "give", new Class<?>[]{ org.bukkit.inventory.ItemStack[].class }, new Object[]{ p0 }, io.papermc.paper.entity.PlayerGiveResult.class);
    }

    @Override public io.papermc.paper.entity.PlayerGiveResult give(java.util.Collection p0, boolean p1) {
        return (io.papermc.paper.entity.PlayerGiveResult) PlayerCalls.call(this.handle, "give", new Class<?>[]{ java.util.Collection.class, boolean.class }, new Object[]{ p0, p1 }, io.papermc.paper.entity.PlayerGiveResult.class);
    }

    @Override public io.papermc.paper.entity.PlayerGiveResult give(java.util.Collection p0) {
        return (io.papermc.paper.entity.PlayerGiveResult) PlayerCalls.call(this.handle, "give", new Class<?>[]{ java.util.Collection.class }, new Object[]{ p0 }, io.papermc.paper.entity.PlayerGiveResult.class);
    }

    @Override public boolean isSneaking() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isSneaking", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void loadData() {
        PlayerCalls.call(this.handle, "loadData", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void playSound(org.bukkit.Location p0, org.bukkit.Sound p1, org.bukkit.SoundCategory p2, float p3, float p4) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ org.bukkit.Location.class, org.bukkit.Sound.class, org.bukkit.SoundCategory.class, float.class, float.class }, new Object[]{ p0, p1, p2, p3, p4 }, void.class);
    }

    @Override public void playSound(org.bukkit.Location p0, java.lang.String p1, float p2, float p3) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ org.bukkit.Location.class, java.lang.String.class, float.class, float.class }, new Object[]{ p0, p1, p2, p3 }, void.class);
    }

    @Override public void playSound(org.bukkit.Location p0, org.bukkit.Sound p1, float p2, float p3) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ org.bukkit.Location.class, org.bukkit.Sound.class, float.class, float.class }, new Object[]{ p0, p1, p2, p3 }, void.class);
    }

    @Override public void playSound(org.bukkit.entity.Entity p0, org.bukkit.Sound p1, org.bukkit.SoundCategory p2, float p3, float p4, long p5) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ org.bukkit.entity.Entity.class, org.bukkit.Sound.class, org.bukkit.SoundCategory.class, float.class, float.class, long.class }, new Object[]{ p0, p1, p2, p3, p4, p5 }, void.class);
    }

    @Override public void playSound(org.bukkit.entity.Entity p0, org.bukkit.Sound p1, float p2, float p3) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ org.bukkit.entity.Entity.class, org.bukkit.Sound.class, float.class, float.class }, new Object[]{ p0, p1, p2, p3 }, void.class);
    }

    @Override public void playSound(org.bukkit.entity.Entity p0, java.lang.String p1, float p2, float p3) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ org.bukkit.entity.Entity.class, java.lang.String.class, float.class, float.class }, new Object[]{ p0, p1, p2, p3 }, void.class);
    }

    @Override public void playSound(org.bukkit.entity.Entity p0, org.bukkit.Sound p1, org.bukkit.SoundCategory p2, float p3, float p4) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ org.bukkit.entity.Entity.class, org.bukkit.Sound.class, org.bukkit.SoundCategory.class, float.class, float.class }, new Object[]{ p0, p1, p2, p3, p4 }, void.class);
    }

    @Override public void playSound(org.bukkit.entity.Entity p0, java.lang.String p1, org.bukkit.SoundCategory p2, float p3, float p4) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ org.bukkit.entity.Entity.class, java.lang.String.class, org.bukkit.SoundCategory.class, float.class, float.class }, new Object[]{ p0, p1, p2, p3, p4 }, void.class);
    }

    @Override public void playSound(net.kyori.adventure.sound.Sound p0, io.papermc.paper.math.Position p1) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ net.kyori.adventure.sound.Sound.class, io.papermc.paper.math.Position.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void playSound(org.bukkit.entity.Entity p0, java.lang.String p1, org.bukkit.SoundCategory p2, float p3, float p4, long p5) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ org.bukkit.entity.Entity.class, java.lang.String.class, org.bukkit.SoundCategory.class, float.class, float.class, long.class }, new Object[]{ p0, p1, p2, p3, p4, p5 }, void.class);
    }

    @Override public void playSound(org.bukkit.Location p0, java.lang.String p1, org.bukkit.SoundCategory p2, float p3, float p4) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ org.bukkit.Location.class, java.lang.String.class, org.bukkit.SoundCategory.class, float.class, float.class }, new Object[]{ p0, p1, p2, p3, p4 }, void.class);
    }

    @Override public void playSound(org.bukkit.Location p0, org.bukkit.Sound p1, org.bukkit.SoundCategory p2, float p3, float p4, long p5) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ org.bukkit.Location.class, org.bukkit.Sound.class, org.bukkit.SoundCategory.class, float.class, float.class, long.class }, new Object[]{ p0, p1, p2, p3, p4, p5 }, void.class);
    }

    @Override public void playSound(org.bukkit.Location p0, java.lang.String p1, org.bukkit.SoundCategory p2, float p3, float p4, long p5) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ org.bukkit.Location.class, java.lang.String.class, org.bukkit.SoundCategory.class, float.class, float.class, long.class }, new Object[]{ p0, p1, p2, p3, p4, p5 }, void.class);
    }

    @Override public org.bukkit.entity.Player.Spigot spigot() {
        return (org.bukkit.entity.Player.Spigot) PlayerCalls.call(this.handle, "spigot", new Class<?>[0], new Object[]{  }, org.bukkit.entity.Player.Spigot.class);
    }

    @Override public void playEffect(org.bukkit.Location p0, org.bukkit.Effect p1, int p2) {
        PlayerCalls.call(this.handle, "playEffect", new Class<?>[]{ org.bukkit.Location.class, org.bukkit.Effect.class, int.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void playEffect(org.bukkit.Location p0, org.bukkit.Effect p1, java.lang.Object p2) {
        PlayerCalls.call(this.handle, "playEffect", new Class<?>[]{ org.bukkit.Location.class, org.bukkit.Effect.class, java.lang.Object.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void giveExp(int p0, boolean p1) {
        PlayerCalls.call(this.handle, "giveExp", new Class<?>[]{ int.class, boolean.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void giveExp(int p0) {
        PlayerCalls.call(this.handle, "giveExp", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.BanEntry ban(java.lang.String p0, java.util.Date p1, java.lang.String p2, boolean p3) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "ban", new Class<?>[]{ java.lang.String.class, java.util.Date.class, java.lang.String.class, boolean.class }, new Object[]{ p0, p1, p2, p3 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry ban(java.lang.String p0, java.time.Instant p1, java.lang.String p2, boolean p3) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "ban", new Class<?>[]{ java.lang.String.class, java.time.Instant.class, java.lang.String.class, boolean.class }, new Object[]{ p0, p1, p2, p3 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry ban(java.lang.String p0, java.time.Duration p1, java.lang.String p2, boolean p3) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "ban", new Class<?>[]{ java.lang.String.class, java.time.Duration.class, java.lang.String.class, boolean.class }, new Object[]{ p0, p1, p2, p3 }, org.bukkit.BanEntry.class);
    }

    @Override public void openSign(org.bukkit.block.Sign p0, org.bukkit.block.sign.Side p1) {
        PlayerCalls.call(this.handle, "openSign", new Class<?>[]{ org.bukkit.block.Sign.class, org.bukkit.block.sign.Side.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void openSign(org.bukkit.block.Sign p0) {
        PlayerCalls.call(this.handle, "openSign", new Class<?>[]{ org.bukkit.block.Sign.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void chat(java.lang.String p0) {
        PlayerCalls.call(this.handle, "chat", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean breakBlock(org.bukkit.block.Block p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "breakBlock", new Class<?>[]{ org.bukkit.block.Block.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public void showEntity(org.bukkit.plugin.Plugin p0, org.bukkit.entity.Entity p1) {
        PlayerCalls.call(this.handle, "showEntity", new Class<?>[]{ org.bukkit.plugin.Plugin.class, org.bukkit.entity.Entity.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendMap(org.bukkit.map.MapView p0) {
        PlayerCalls.call(this.handle, "sendMap", new Class<?>[]{ org.bukkit.map.MapView.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void kickPlayer(java.lang.String p0) {
        PlayerCalls.call(this.handle, "kickPlayer", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void lookAt(org.bukkit.entity.Entity p0, io.papermc.paper.entity.LookAnchor p1, io.papermc.paper.entity.LookAnchor p2) {
        PlayerCalls.call(this.handle, "lookAt", new Class<?>[]{ org.bukkit.entity.Entity.class, io.papermc.paper.entity.LookAnchor.class, io.papermc.paper.entity.LookAnchor.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void hidePlayer(org.bukkit.entity.Player p0) {
        PlayerCalls.call(this.handle, "hidePlayer", new Class<?>[]{ org.bukkit.entity.Player.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void hidePlayer(org.bukkit.plugin.Plugin p0, org.bukkit.entity.Player p1) {
        PlayerCalls.call(this.handle, "hidePlayer", new Class<?>[]{ org.bukkit.plugin.Plugin.class, org.bukkit.entity.Player.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void showTitle(net.md_5.bungee.api.chat.BaseComponent[] p0) {
        PlayerCalls.call(this.handle, "showTitle", new Class<?>[]{ net.md_5.bungee.api.chat.BaseComponent[].class }, new Object[]{ p0 }, void.class);
    }

    @Override public void showTitle(net.md_5.bungee.api.chat.BaseComponent p0) {
        PlayerCalls.call(this.handle, "showTitle", new Class<?>[]{ net.md_5.bungee.api.chat.BaseComponent.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void showTitle(net.md_5.bungee.api.chat.BaseComponent p0, net.md_5.bungee.api.chat.BaseComponent p1, int p2, int p3, int p4) {
        PlayerCalls.call(this.handle, "showTitle", new Class<?>[]{ net.md_5.bungee.api.chat.BaseComponent.class, net.md_5.bungee.api.chat.BaseComponent.class, int.class, int.class, int.class }, new Object[]{ p0, p1, p2, p3, p4 }, void.class);
    }

    @Override public void showTitle(net.md_5.bungee.api.chat.BaseComponent[] p0, net.md_5.bungee.api.chat.BaseComponent[] p1, int p2, int p3, int p4) {
        PlayerCalls.call(this.handle, "showTitle", new Class<?>[]{ net.md_5.bungee.api.chat.BaseComponent[].class, net.md_5.bungee.api.chat.BaseComponent[].class, int.class, int.class, int.class }, new Object[]{ p0, p1, p2, p3, p4 }, void.class);
    }

    @Override public void showPlayer(org.bukkit.plugin.Plugin p0, org.bukkit.entity.Player p1) {
        PlayerCalls.call(this.handle, "showPlayer", new Class<?>[]{ org.bukkit.plugin.Plugin.class, org.bukkit.entity.Player.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void showPlayer(org.bukkit.entity.Player p0) {
        PlayerCalls.call(this.handle, "showPlayer", new Class<?>[]{ org.bukkit.entity.Player.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void hideTitle() {
        PlayerCalls.call(this.handle, "hideTitle", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void sendTitle(java.lang.String p0, java.lang.String p1, int p2, int p3, int p4) {
        PlayerCalls.call(this.handle, "sendTitle", new Class<?>[]{ java.lang.String.class, java.lang.String.class, int.class, int.class, int.class }, new Object[]{ p0, p1, p2, p3, p4 }, void.class);
    }

    @Override public void sendTitle(java.lang.String p0, java.lang.String p1) {
        PlayerCalls.call(this.handle, "sendTitle", new Class<?>[]{ java.lang.String.class, java.lang.String.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendTitle(com.destroystokyo.paper.Title p0) {
        PlayerCalls.call(this.handle, "sendTitle", new Class<?>[]{ com.destroystokyo.paper.Title.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getPing() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getPing", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void setExp(float p0) {
        PlayerCalls.call(this.handle, "setExp", new Class<?>[]{ float.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean listPlayer(org.bukkit.entity.Player p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "listPlayer", new Class<?>[]{ org.bukkit.entity.Player.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public void setFlying(boolean p0) {
        PlayerCalls.call(this.handle, "setFlying", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void resetTitle() {
        PlayerCalls.call(this.handle, "resetTitle", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void openBook(org.bukkit.inventory.ItemStack p0) {
        PlayerCalls.call(this.handle, "openBook", new Class<?>[]{ org.bukkit.inventory.ItemStack.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendLinks(org.bukkit.ServerLinks p0) {
        PlayerCalls.call(this.handle, "sendLinks", new Class<?>[]{ org.bukkit.ServerLinks.class }, new Object[]{ p0 }, void.class);
    }

    @Override public float getExp() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getExp", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public boolean isListed(org.bukkit.entity.Player p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isListed", new Class<?>[]{ org.bukkit.entity.Player.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean isFlying() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isFlying", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean canSee(org.bukkit.entity.Entity p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "canSee", new Class<?>[]{ org.bukkit.entity.Entity.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean canSee(org.bukkit.entity.Player p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "canSee", new Class<?>[]{ org.bukkit.entity.Player.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public void sendBlockUpdate(org.bukkit.Location p0, org.bukkit.block.TileState p1) {
        PlayerCalls.call(this.handle, "sendBlockUpdate", new Class<?>[]{ org.bukkit.Location.class, org.bukkit.block.TileState.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public long getPlayerTimeOffset() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getPlayerTimeOffset", new Class<?>[0], new Object[]{  }, long.class)).longValue();
    }

    @Override public float getWalkSpeed() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getWalkSpeed", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public void setTexturePack(java.lang.String p0) {
        PlayerCalls.call(this.handle, "setTexturePack", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void updateInventory() {
        PlayerCalls.call(this.handle, "updateInventory", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void showWinScreen() {
        PlayerCalls.call(this.handle, "showWinScreen", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public long getPlayerTime() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getPlayerTime", new Class<?>[0], new Object[]{  }, long.class)).longValue();
    }

    @Override public void sendHurtAnimation(float p0) {
        PlayerCalls.call(this.handle, "sendHurtAnimation", new Class<?>[]{ float.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean isPlayerTimeRelative() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isPlayerTimeRelative", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void resetPlayerWeather() {
        PlayerCalls.call(this.handle, "resetPlayerWeather", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public org.bukkit.GameMode getPreviousGameMode() {
        return (org.bukkit.GameMode) PlayerCalls.call(this.handle, "getPreviousGameMode", new Class<?>[0], new Object[]{  }, org.bukkit.GameMode.class);
    }

    @Override public int getExpCooldown() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getExpCooldown", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public float getFlySpeed() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getFlySpeed", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public boolean hasResourcePack() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasResourcePack", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void addResourcePack(java.util.UUID p0, java.lang.String p1, byte[] p2, java.lang.String p3, boolean p4) {
        PlayerCalls.call(this.handle, "addResourcePack", new Class<?>[]{ java.util.UUID.class, java.lang.String.class, byte[].class, java.lang.String.class, boolean.class }, new Object[]{ p0, p1, p2, p3, p4 }, void.class);
    }

    @Override public void removeResourcePack(java.util.UUID p0) {
        PlayerCalls.call(this.handle, "removeResourcePack", new Class<?>[]{ java.util.UUID.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setAllowFlight(boolean p0) {
        PlayerCalls.call(this.handle, "setAllowFlight", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void resetFlyingTicks() {
        PlayerCalls.call(this.handle, "resetFlyingTicks", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void setFlyingFallDamage(net.kyori.adventure.util.TriState p0) {
        PlayerCalls.call(this.handle, "setFlyingFallDamage", new Class<?>[]{ net.kyori.adventure.util.TriState.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void removeResourcePacks() {
        PlayerCalls.call(this.handle, "removeResourcePacks", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void resetPlayerTime() {
        PlayerCalls.call(this.handle, "resetPlayerTime", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public java.lang.String getResourcePackHash() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getResourcePackHash", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public void setFlySpeed(float p0) {
        PlayerCalls.call(this.handle, "setFlySpeed", new Class<?>[]{ float.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setSubtitle(net.md_5.bungee.api.chat.BaseComponent[] p0) {
        PlayerCalls.call(this.handle, "setSubtitle", new Class<?>[]{ net.md_5.bungee.api.chat.BaseComponent[].class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setSubtitle(net.md_5.bungee.api.chat.BaseComponent p0) {
        PlayerCalls.call(this.handle, "setSubtitle", new Class<?>[]{ net.md_5.bungee.api.chat.BaseComponent.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void addCustomChatCompletions(java.util.Collection p0) {
        PlayerCalls.call(this.handle, "addCustomChatCompletions", new Class<?>[]{ java.util.Collection.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendEquipmentChange(org.bukkit.entity.LivingEntity p0, java.util.Map p1) {
        PlayerCalls.call(this.handle, "sendEquipmentChange", new Class<?>[]{ org.bukkit.entity.LivingEntity.class, java.util.Map.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendEquipmentChange(org.bukkit.entity.LivingEntity p0, org.bukkit.inventory.EquipmentSlot p1, org.bukkit.inventory.ItemStack p2) {
        PlayerCalls.call(this.handle, "sendEquipmentChange", new Class<?>[]{ org.bukkit.entity.LivingEntity.class, org.bukkit.inventory.EquipmentSlot.class, org.bukkit.inventory.ItemStack.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public boolean unlistPlayer(org.bukkit.entity.Player p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "unlistPlayer", new Class<?>[]{ org.bukkit.entity.Player.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public void setWalkSpeed(float p0) {
        PlayerCalls.call(this.handle, "setWalkSpeed", new Class<?>[]{ float.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setPlayerWeather(org.bukkit.WeatherType p0) {
        PlayerCalls.call(this.handle, "setPlayerWeather", new Class<?>[]{ org.bukkit.WeatherType.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendPotionEffectChange(org.bukkit.entity.LivingEntity p0, org.bukkit.potion.PotionEffect p1) {
        PlayerCalls.call(this.handle, "sendPotionEffectChange", new Class<?>[]{ org.bukkit.entity.LivingEntity.class, org.bukkit.potion.PotionEffect.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setCustomChatCompletions(java.util.Collection p0) {
        PlayerCalls.call(this.handle, "setCustomChatCompletions", new Class<?>[]{ java.util.Collection.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean getAllowFlight() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "getAllowFlight", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void sendExperienceChange(float p0) {
        PlayerCalls.call(this.handle, "sendExperienceChange", new Class<?>[]{ float.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendExperienceChange(float p0, int p1) {
        PlayerCalls.call(this.handle, "sendExperienceChange", new Class<?>[]{ float.class, int.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setHasSeenWinScreen(boolean p0) {
        PlayerCalls.call(this.handle, "setHasSeenWinScreen", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setTitleTimes(int p0, int p1, int p2) {
        PlayerCalls.call(this.handle, "setTitleTimes", new Class<?>[]{ int.class, int.class, int.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public int getTotalExperience() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getTotalExperience", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public boolean hasSeenWinScreen() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasSeenWinScreen", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void sendBlockChange(org.bukkit.Location p0, org.bukkit.Material p1, byte p2) {
        PlayerCalls.call(this.handle, "sendBlockChange", new Class<?>[]{ org.bukkit.Location.class, org.bukkit.Material.class, byte.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void sendBlockChange(org.bukkit.Location p0, org.bukkit.block.data.BlockData p1) {
        PlayerCalls.call(this.handle, "sendBlockChange", new Class<?>[]{ org.bukkit.Location.class, org.bukkit.block.data.BlockData.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void giveExpLevels(int p0) {
        PlayerCalls.call(this.handle, "giveExpLevels", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void updateTitle(com.destroystokyo.paper.Title p0) {
        PlayerCalls.call(this.handle, "updateTitle", new Class<?>[]{ com.destroystokyo.paper.Title.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendActionBar(java.lang.String p0) {
        PlayerCalls.call(this.handle, "sendActionBar", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendActionBar(char p0, java.lang.String p1) {
        PlayerCalls.call(this.handle, "sendActionBar", new Class<?>[]{ char.class, java.lang.String.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendActionBar(net.md_5.bungee.api.chat.BaseComponent[] p0) {
        PlayerCalls.call(this.handle, "sendActionBar", new Class<?>[]{ net.md_5.bungee.api.chat.BaseComponent[].class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setExpCooldown(int p0) {
        PlayerCalls.call(this.handle, "setExpCooldown", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int applyMending(int p0) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "applyMending", new Class<?>[]{ int.class }, new Object[]{ p0 }, int.class)).intValue();
    }

    @Override public net.kyori.adventure.util.TriState hasFlyingFallDamage() {
        return (net.kyori.adventure.util.TriState) PlayerCalls.call(this.handle, "hasFlyingFallDamage", new Class<?>[0], new Object[]{  }, net.kyori.adventure.util.TriState.class);
    }

    @Override public void setTotalExperience(int p0) {
        PlayerCalls.call(this.handle, "setTotalExperience", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.WeatherType getPlayerWeather() {
        return (org.bukkit.WeatherType) PlayerCalls.call(this.handle, "getPlayerWeather", new Class<?>[0], new Object[]{  }, org.bukkit.WeatherType.class);
    }

    @Override public void setPlayerTime(long p0, boolean p1) {
        PlayerCalls.call(this.handle, "setPlayerTime", new Class<?>[]{ long.class, boolean.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void stopAllSounds() {
        PlayerCalls.call(this.handle, "stopAllSounds", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void resetCooldown() {
        PlayerCalls.call(this.handle, "resetCooldown", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void sendEntityEffect(org.bukkit.EntityEffect p0, org.bukkit.entity.Entity p1) {
        PlayerCalls.call(this.handle, "sendEntityEffect", new Class<?>[]{ org.bukkit.EntityEffect.class, org.bukkit.entity.Entity.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setWardenWarningCooldown(int p0) {
        PlayerCalls.call(this.handle, "setWardenWarningCooldown", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public java.time.Duration getIdleDuration() {
        return (java.time.Duration) PlayerCalls.call(this.handle, "getIdleDuration", new Class<?>[0], new Object[]{  }, java.time.Duration.class);
    }

    @Override public int getDeathScreenScore() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getDeathScreenScore", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void setHealthScaled(boolean p0) {
        PlayerCalls.call(this.handle, "setHealthScaled", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.WorldBorder getWorldBorder() {
        return (org.bukkit.WorldBorder) PlayerCalls.call(this.handle, "getWorldBorder", new Class<?>[0], new Object[]{  }, org.bukkit.WorldBorder.class);
    }

    @Override public int getClientViewDistance() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getClientViewDistance", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public boolean getAffectsSpawning() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "getAffectsSpawning", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void setScoreboard(org.bukkit.scoreboard.Scoreboard p0) {
        PlayerCalls.call(this.handle, "setScoreboard", new Class<?>[]{ org.bukkit.scoreboard.Scoreboard.class }, new Object[]{ p0 }, void.class);
    }

    @Override public double getHealthScale() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getHealthScale", new Class<?>[0], new Object[]{  }, double.class)).doubleValue();
    }

    @Override public int getSimulationDistance() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getSimulationDistance", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void updateCommands() {
        PlayerCalls.call(this.handle, "updateCommands", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public boolean isAllowingServerListings() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isAllowingServerListings", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.entity.Entity getSpectatorTarget() {
        return (org.bukkit.entity.Entity) PlayerCalls.call(this.handle, "getSpectatorTarget", new Class<?>[0], new Object[]{  }, org.bukkit.entity.Entity.class);
    }

    @Override public void setPlayerProfile(com.destroystokyo.paper.profile.PlayerProfile p0) {
        PlayerCalls.call(this.handle, "setPlayerProfile", new Class<?>[]{ com.destroystokyo.paper.profile.PlayerProfile.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setSendViewDistance(int p0) {
        PlayerCalls.call(this.handle, "setSendViewDistance", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean isHealthScaled() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isHealthScaled", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public int getNoTickViewDistance() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getNoTickViewDistance", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public float getCooledAttackStrength(float p0) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getCooledAttackStrength", new Class<?>[]{ float.class }, new Object[]{ p0 }, float.class)).floatValue();
    }

    @Override public void setRotation(float p0, float p1) {
        PlayerCalls.call(this.handle, "setRotation", new Class<?>[]{ float.class, float.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setRotation(io.papermc.paper.math.Angle p0, io.papermc.paper.math.Angle p1) {
        PlayerCalls.call(this.handle, "setRotation", new Class<?>[]{ io.papermc.paper.math.Angle.class, io.papermc.paper.math.Angle.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setSpectatorTarget(org.bukkit.entity.Entity p0) {
        PlayerCalls.call(this.handle, "setSpectatorTarget", new Class<?>[]{ org.bukkit.entity.Entity.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setSimulationDistance(int p0) {
        PlayerCalls.call(this.handle, "setSimulationDistance", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void openVirtualSign(io.papermc.paper.math.Position p0, org.bukkit.block.sign.Side p1) {
        PlayerCalls.call(this.handle, "openVirtualSign", new Class<?>[]{ io.papermc.paper.math.Position.class, org.bukkit.block.sign.Side.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public int getWardenWarningLevel() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getWardenWarningLevel", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public java.util.Set getSentChunkKeys() {
        return (java.util.Set) PlayerCalls.call(this.handle, "getSentChunkKeys", new Class<?>[0], new Object[]{  }, java.util.Set.class);
    }

    @Override public int getSendViewDistance() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getSendViewDistance", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void setHealthScale(double p0) {
        PlayerCalls.call(this.handle, "setHealthScale", new Class<?>[]{ double.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.advancement.AdvancementProgress getAdvancementProgress(org.bukkit.advancement.Advancement p0) {
        return (org.bukkit.advancement.AdvancementProgress) PlayerCalls.call(this.handle, "getAdvancementProgress", new Class<?>[]{ org.bukkit.advancement.Advancement.class }, new Object[]{ p0 }, org.bukkit.advancement.AdvancementProgress.class);
    }

    @Override public void setNoTickViewDistance(int p0) {
        PlayerCalls.call(this.handle, "setNoTickViewDistance", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public float getCooldownPeriod() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getCooldownPeriod", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public void sendOpLevel(byte p0) {
        PlayerCalls.call(this.handle, "sendOpLevel", new Class<?>[]{ byte.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setWardenWarningLevel(int p0) {
        PlayerCalls.call(this.handle, "setWardenWarningLevel", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendHealthUpdate(double p0, int p1, float p2) {
        PlayerCalls.call(this.handle, "sendHealthUpdate", new Class<?>[]{ double.class, int.class, float.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void sendHealthUpdate() {
        PlayerCalls.call(this.handle, "sendHealthUpdate", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public io.papermc.paper.connection.PlayerGameConnection getConnection() {
        return (io.papermc.paper.connection.PlayerGameConnection) PlayerCalls.call(this.handle, "getConnection", new Class<?>[0], new Object[]{  }, io.papermc.paper.connection.PlayerGameConnection.class);
    }

    @Override public io.papermc.paper.entity.PlayerPostEffects postEffects() {
        return (io.papermc.paper.entity.PlayerPostEffects) PlayerCalls.call(this.handle, "postEffects", new Class<?>[0], new Object[]{  }, io.papermc.paper.entity.PlayerPostEffects.class);
    }

    @Override public void increaseWardenWarningLevel() {
        PlayerCalls.call(this.handle, "increaseWardenWarningLevel", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public org.bukkit.entity.Firework boostElytra(org.bukkit.inventory.ItemStack p0) {
        return (org.bukkit.entity.Firework) PlayerCalls.call(this.handle, "boostElytra", new Class<?>[]{ org.bukkit.inventory.ItemStack.class }, new Object[]{ p0 }, org.bukkit.entity.Firework.class);
    }

    @Override public int getWardenWarningCooldown() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getWardenWarningCooldown", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public net.kyori.adventure.text.event.HoverEvent asHoverEvent(java.util.function.UnaryOperator p0) {
        return (net.kyori.adventure.text.event.HoverEvent) PlayerCalls.call(this.handle, "asHoverEvent", new Class<?>[]{ java.util.function.UnaryOperator.class }, new Object[]{ p0 }, net.kyori.adventure.text.event.HoverEvent.class);
    }

    @Override public void resetIdleDuration() {
        PlayerCalls.call(this.handle, "resetIdleDuration", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public java.util.Set getSentChunks() {
        return (java.util.Set) PlayerCalls.call(this.handle, "getSentChunks", new Class<?>[0], new Object[]{  }, java.util.Set.class);
    }

    @Override public void setDeathScreenScore(int p0) {
        PlayerCalls.call(this.handle, "setDeathScreenScore", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.scoreboard.Scoreboard getScoreboard() {
        return (org.bukkit.scoreboard.Scoreboard) PlayerCalls.call(this.handle, "getScoreboard", new Class<?>[0], new Object[]{  }, org.bukkit.scoreboard.Scoreboard.class);
    }

    @Override public void setAffectsSpawning(boolean p0) {
        PlayerCalls.call(this.handle, "setAffectsSpawning", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void showDemoScreen() {
        PlayerCalls.call(this.handle, "showDemoScreen", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void unsetFixedPose() {
        PlayerCalls.call(this.handle, "unsetFixedPose", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void setWorldBorder(org.bukkit.WorldBorder p0) {
        PlayerCalls.call(this.handle, "setWorldBorder", new Class<?>[]{ org.bukkit.WorldBorder.class }, new Object[]{ p0 }, void.class);
    }

    @Override public java.lang.String getClientBrandName() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getClientBrandName", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public void setBedSpawnLocation(org.bukkit.Location p0) {
        PlayerCalls.call(this.handle, "setBedSpawnLocation", new Class<?>[]{ org.bukkit.Location.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setBedSpawnLocation(org.bukkit.Location p0, boolean p1) {
        PlayerCalls.call(this.handle, "setBedSpawnLocation", new Class<?>[]{ org.bukkit.Location.class, boolean.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setRespawnLocation(org.bukkit.Location p0) {
        PlayerCalls.call(this.handle, "setRespawnLocation", new Class<?>[]{ org.bukkit.Location.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setRespawnLocation(org.bukkit.Location p0, boolean p1) {
        PlayerCalls.call(this.handle, "setRespawnLocation", new Class<?>[]{ org.bukkit.Location.class, boolean.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public org.bukkit.Location getRespawnLocation() {
        return (org.bukkit.Location) PlayerCalls.call(this.handle, "getRespawnLocation", new Class<?>[0], new Object[]{  }, org.bukkit.Location.class);
    }

    @Override public void spawnParticle(org.bukkit.Particle p0, org.bukkit.Location p1, int p2, java.lang.Object p3) {
        PlayerCalls.call(this.handle, "spawnParticle", new Class<?>[]{ org.bukkit.Particle.class, org.bukkit.Location.class, int.class, java.lang.Object.class }, new Object[]{ p0, p1, p2, p3 }, void.class);
    }

    @Override public void spawnParticle(org.bukkit.Particle p0, org.bukkit.Location p1, int p2, double p3, double p4, double p5, double p6) {
        PlayerCalls.call(this.handle, "spawnParticle", new Class<?>[]{ org.bukkit.Particle.class, org.bukkit.Location.class, int.class, double.class, double.class, double.class, double.class }, new Object[]{ p0, p1, p2, p3, p4, p5, p6 }, void.class);
    }

    @Override public void spawnParticle(org.bukkit.Particle p0, org.bukkit.Location p1, int p2, double p3, double p4, double p5) {
        PlayerCalls.call(this.handle, "spawnParticle", new Class<?>[]{ org.bukkit.Particle.class, org.bukkit.Location.class, int.class, double.class, double.class, double.class }, new Object[]{ p0, p1, p2, p3, p4, p5 }, void.class);
    }

    @Override public void spawnParticle(org.bukkit.Particle p0, double p1, double p2, double p3, int p4) {
        PlayerCalls.call(this.handle, "spawnParticle", new Class<?>[]{ org.bukkit.Particle.class, double.class, double.class, double.class, int.class }, new Object[]{ p0, p1, p2, p3, p4 }, void.class);
    }

    @Override public void spawnParticle(org.bukkit.Particle p0, double p1, double p2, double p3, int p4, double p5, double p6, double p7) {
        PlayerCalls.call(this.handle, "spawnParticle", new Class<?>[]{ org.bukkit.Particle.class, double.class, double.class, double.class, int.class, double.class, double.class, double.class }, new Object[]{ p0, p1, p2, p3, p4, p5, p6, p7 }, void.class);
    }

    @Override public void spawnParticle(org.bukkit.Particle p0, org.bukkit.Location p1, int p2, double p3, double p4, double p5, java.lang.Object p6) {
        PlayerCalls.call(this.handle, "spawnParticle", new Class<?>[]{ org.bukkit.Particle.class, org.bukkit.Location.class, int.class, double.class, double.class, double.class, java.lang.Object.class }, new Object[]{ p0, p1, p2, p3, p4, p5, p6 }, void.class);
    }

    @Override public void spawnParticle(org.bukkit.Particle p0, double p1, double p2, double p3, int p4, double p5, double p6, double p7, java.lang.Object p8) {
        PlayerCalls.call(this.handle, "spawnParticle", new Class<?>[]{ org.bukkit.Particle.class, double.class, double.class, double.class, int.class, double.class, double.class, double.class, java.lang.Object.class }, new Object[]{ p0, p1, p2, p3, p4, p5, p6, p7, p8 }, void.class);
    }

    @Override public void spawnParticle(org.bukkit.Particle p0, double p1, double p2, double p3, int p4, java.lang.Object p5) {
        PlayerCalls.call(this.handle, "spawnParticle", new Class<?>[]{ org.bukkit.Particle.class, double.class, double.class, double.class, int.class, java.lang.Object.class }, new Object[]{ p0, p1, p2, p3, p4, p5 }, void.class);
    }

    @Override public void spawnParticle(org.bukkit.Particle p0, org.bukkit.Location p1, int p2, double p3, double p4, double p5, double p6, java.lang.Object p7, boolean p8) {
        PlayerCalls.call(this.handle, "spawnParticle", new Class<?>[]{ org.bukkit.Particle.class, org.bukkit.Location.class, int.class, double.class, double.class, double.class, double.class, java.lang.Object.class, boolean.class }, new Object[]{ p0, p1, p2, p3, p4, p5, p6, p7, p8 }, void.class);
    }

    @Override public void spawnParticle(org.bukkit.Particle p0, double p1, double p2, double p3, int p4, double p5, double p6, double p7, double p8, java.lang.Object p9, boolean p10) {
        PlayerCalls.call(this.handle, "spawnParticle", new Class<?>[]{ org.bukkit.Particle.class, double.class, double.class, double.class, int.class, double.class, double.class, double.class, double.class, java.lang.Object.class, boolean.class }, new Object[]{ p0, p1, p2, p3, p4, p5, p6, p7, p8, p9, p10 }, void.class);
    }

    @Override public void spawnParticle(org.bukkit.Particle p0, double p1, double p2, double p3, int p4, double p5, double p6, double p7, double p8, double p9, double p10, java.lang.Object p11, boolean p12, org.bukkit.Particle.RandomizationType p13) {
        PlayerCalls.call(this.handle, "spawnParticle", new Class<?>[]{ org.bukkit.Particle.class, double.class, double.class, double.class, int.class, double.class, double.class, double.class, double.class, double.class, double.class, java.lang.Object.class, boolean.class, org.bukkit.Particle.RandomizationType.class }, new Object[]{ p0, p1, p2, p3, p4, p5, p6, p7, p8, p9, p10, p11, p12, p13 }, void.class);
    }

    @Override public void spawnParticle(org.bukkit.Particle p0, org.bukkit.Location p1, int p2) {
        PlayerCalls.call(this.handle, "spawnParticle", new Class<?>[]{ org.bukkit.Particle.class, org.bukkit.Location.class, int.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void spawnParticle(org.bukkit.Particle p0, double p1, double p2, double p3, int p4, double p5, double p6, double p7, double p8, java.lang.Object p9) {
        PlayerCalls.call(this.handle, "spawnParticle", new Class<?>[]{ org.bukkit.Particle.class, double.class, double.class, double.class, int.class, double.class, double.class, double.class, double.class, java.lang.Object.class }, new Object[]{ p0, p1, p2, p3, p4, p5, p6, p7, p8, p9 }, void.class);
    }

    @Override public void spawnParticle(org.bukkit.Particle p0, org.bukkit.Location p1, int p2, double p3, double p4, double p5, double p6, java.lang.Object p7) {
        PlayerCalls.call(this.handle, "spawnParticle", new Class<?>[]{ org.bukkit.Particle.class, org.bukkit.Location.class, int.class, double.class, double.class, double.class, double.class, java.lang.Object.class }, new Object[]{ p0, p1, p2, p3, p4, p5, p6, p7 }, void.class);
    }

    @Override public void spawnParticle(org.bukkit.Particle p0, double p1, double p2, double p3, int p4, double p5, double p6, double p7, double p8) {
        PlayerCalls.call(this.handle, "spawnParticle", new Class<?>[]{ org.bukkit.Particle.class, double.class, double.class, double.class, int.class, double.class, double.class, double.class, double.class }, new Object[]{ p0, p1, p2, p3, p4, p5, p6, p7, p8 }, void.class);
    }

    @Override public int getViewDistance() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getViewDistance", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public net.kyori.adventure.text.Component playerListFooter() {
        return (net.kyori.adventure.text.Component) PlayerCalls.call(this.handle, "playerListFooter", new Class<?>[0], new Object[]{  }, net.kyori.adventure.text.Component.class);
    }

    @Override public boolean isChunkSent(org.bukkit.Chunk p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isChunkSent", new Class<?>[]{ org.bukkit.Chunk.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean isChunkSent(long p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isChunkSent", new Class<?>[]{ long.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public java.lang.String getPlayerListName() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getPlayerListName", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public void setPlayerListHeaderFooter(net.md_5.bungee.api.chat.BaseComponent p0, net.md_5.bungee.api.chat.BaseComponent p1) {
        PlayerCalls.call(this.handle, "setPlayerListHeaderFooter", new Class<?>[]{ net.md_5.bungee.api.chat.BaseComponent.class, net.md_5.bungee.api.chat.BaseComponent.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setPlayerListHeaderFooter(net.md_5.bungee.api.chat.BaseComponent[] p0, net.md_5.bungee.api.chat.BaseComponent[] p1) {
        PlayerCalls.call(this.handle, "setPlayerListHeaderFooter", new Class<?>[]{ net.md_5.bungee.api.chat.BaseComponent[].class, net.md_5.bungee.api.chat.BaseComponent[].class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setPlayerListHeaderFooter(java.lang.String p0, java.lang.String p1) {
        PlayerCalls.call(this.handle, "setPlayerListHeaderFooter", new Class<?>[]{ java.lang.String.class, java.lang.String.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setCompassTarget(org.bukkit.Location p0) {
        PlayerCalls.call(this.handle, "setCompassTarget", new Class<?>[]{ org.bukkit.Location.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.event.player.PlayerResourcePackStatusEvent.Status getResourcePackStatus() {
        return (org.bukkit.event.player.PlayerResourcePackStatusEvent.Status) PlayerCalls.call(this.handle, "getResourcePackStatus", new Class<?>[0], new Object[]{  }, org.bukkit.event.player.PlayerResourcePackStatusEvent.Status.class);
    }

    @Override public java.lang.Iterable activeBossBars() {
        return (java.lang.Iterable) PlayerCalls.call(this.handle, "activeBossBars", new Class<?>[0], new Object[]{  }, java.lang.Iterable.class);
    }

    @Override public void sendMessage(net.md_5.bungee.api.ChatMessageType p0, net.md_5.bungee.api.chat.BaseComponent[] p1) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ net.md_5.bungee.api.ChatMessageType.class, net.md_5.bungee.api.chat.BaseComponent[].class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendMessage(net.md_5.bungee.api.chat.BaseComponent[] p0) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ net.md_5.bungee.api.chat.BaseComponent[].class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendMessage(net.md_5.bungee.api.chat.BaseComponent p0) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ net.md_5.bungee.api.chat.BaseComponent.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setViewDistance(int p0) {
        PlayerCalls.call(this.handle, "setViewDistance", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public java.lang.Object getClientOption(com.destroystokyo.paper.ClientOption p0) {
        return (java.lang.Object) PlayerCalls.call(this.handle, "getClientOption", new Class<?>[]{ com.destroystokyo.paper.ClientOption.class }, new Object[]{ p0 }, java.lang.Object.class);
    }

    @Override public int getPlayerListOrder() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getPlayerListOrder", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.BanEntry banPlayerIP(java.lang.String p0, java.lang.String p1) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayerIP", new Class<?>[]{ java.lang.String.class, java.lang.String.class }, new Object[]{ p0, p1 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banPlayerIP(java.lang.String p0, java.util.Date p1) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayerIP", new Class<?>[]{ java.lang.String.class, java.util.Date.class }, new Object[]{ p0, p1 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banPlayerIP(java.lang.String p0, java.util.Date p1, java.lang.String p2) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayerIP", new Class<?>[]{ java.lang.String.class, java.util.Date.class, java.lang.String.class }, new Object[]{ p0, p1, p2 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banPlayerIP(java.lang.String p0, java.util.Date p1, java.lang.String p2, boolean p3) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayerIP", new Class<?>[]{ java.lang.String.class, java.util.Date.class, java.lang.String.class, boolean.class }, new Object[]{ p0, p1, p2, p3 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banPlayerIP(java.lang.String p0, java.util.Date p1, boolean p2) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayerIP", new Class<?>[]{ java.lang.String.class, java.util.Date.class, boolean.class }, new Object[]{ p0, p1, p2 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banPlayerIP(java.lang.String p0) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayerIP", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banPlayerIP(java.lang.String p0, boolean p1) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayerIP", new Class<?>[]{ java.lang.String.class, boolean.class }, new Object[]{ p0, p1 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banPlayerIP(java.lang.String p0, java.lang.String p1, boolean p2) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayerIP", new Class<?>[]{ java.lang.String.class, java.lang.String.class, boolean.class }, new Object[]{ p0, p1, p2 }, org.bukkit.BanEntry.class);
    }

    @Override public void setDisplayName(java.lang.String p0) {
        PlayerCalls.call(this.handle, "setDisplayName", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setPlayerListOrder(int p0) {
        PlayerCalls.call(this.handle, "setPlayerListOrder", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendMultiBlockChange(java.util.Map p0, boolean p1) {
        PlayerCalls.call(this.handle, "sendMultiBlockChange", new Class<?>[]{ java.util.Map.class, boolean.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendMultiBlockChange(java.util.Map p0) {
        PlayerCalls.call(this.handle, "sendMultiBlockChange", new Class<?>[]{ java.util.Map.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.BanEntry banPlayerFull(java.lang.String p0, java.lang.String p1) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayerFull", new Class<?>[]{ java.lang.String.class, java.lang.String.class }, new Object[]{ p0, p1 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banPlayerFull(java.lang.String p0) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayerFull", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banPlayerFull(java.lang.String p0, java.util.Date p1, java.lang.String p2) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayerFull", new Class<?>[]{ java.lang.String.class, java.util.Date.class, java.lang.String.class }, new Object[]{ p0, p1, p2 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banPlayerFull(java.lang.String p0, java.util.Date p1) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayerFull", new Class<?>[]{ java.lang.String.class, java.util.Date.class }, new Object[]{ p0, p1 }, org.bukkit.BanEntry.class);
    }

    @Override public void sendBlockChanges(java.util.Collection p0) {
        PlayerCalls.call(this.handle, "sendBlockChanges", new Class<?>[]{ java.util.Collection.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendBlockChanges(java.util.Collection p0, boolean p1) {
        PlayerCalls.call(this.handle, "sendBlockChanges", new Class<?>[]{ java.util.Collection.class, boolean.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void showElderGuardian() {
        PlayerCalls.call(this.handle, "showElderGuardian", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void showElderGuardian(boolean p0) {
        PlayerCalls.call(this.handle, "showElderGuardian", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public net.kyori.adventure.text.object.ObjectContents asObjectContents() {
        return (net.kyori.adventure.text.object.ObjectContents) PlayerCalls.call(this.handle, "asObjectContents", new Class<?>[0], new Object[]{  }, net.kyori.adventure.text.object.ObjectContents.class);
    }

    @Override public net.kyori.adventure.text.Component playerListName() {
        return (net.kyori.adventure.text.Component) PlayerCalls.call(this.handle, "playerListName", new Class<?>[0], new Object[]{  }, net.kyori.adventure.text.Component.class);
    }

    @Override public void playerListName(net.kyori.adventure.text.Component p0) {
        PlayerCalls.call(this.handle, "playerListName", new Class<?>[]{ net.kyori.adventure.text.Component.class }, new Object[]{ p0 }, void.class);
    }

    @Override public net.kyori.adventure.text.Component playerListHeader() {
        return (net.kyori.adventure.text.Component) PlayerCalls.call(this.handle, "playerListHeader", new Class<?>[0], new Object[]{  }, net.kyori.adventure.text.Component.class);
    }

    @Override public void setPlayerListName(java.lang.String p0) {
        PlayerCalls.call(this.handle, "setPlayerListName", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setPlayerListHeader(java.lang.String p0) {
        PlayerCalls.call(this.handle, "setPlayerListHeader", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public java.lang.String getPlayerListHeader() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getPlayerListHeader", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public com.destroystokyo.paper.profile.PlayerProfile getPlayerProfile() {
        return (com.destroystokyo.paper.profile.PlayerProfile) PlayerCalls.call(this.handle, "getPlayerProfile", new Class<?>[0], new Object[]{  }, com.destroystokyo.paper.profile.PlayerProfile.class);
    }

    @Override public void sendSignChange(org.bukkit.Location p0, java.lang.String[] p1) {
        PlayerCalls.call(this.handle, "sendSignChange", new Class<?>[]{ org.bukkit.Location.class, java.lang.String[].class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendSignChange(org.bukkit.Location p0, java.util.List p1, org.bukkit.DyeColor p2, boolean p3) {
        PlayerCalls.call(this.handle, "sendSignChange", new Class<?>[]{ org.bukkit.Location.class, java.util.List.class, org.bukkit.DyeColor.class, boolean.class }, new Object[]{ p0, p1, p2, p3 }, void.class);
    }

    @Override public void sendSignChange(org.bukkit.Location p0, java.util.List p1, boolean p2) {
        PlayerCalls.call(this.handle, "sendSignChange", new Class<?>[]{ org.bukkit.Location.class, java.util.List.class, boolean.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void sendSignChange(org.bukkit.Location p0, java.lang.String[] p1, org.bukkit.DyeColor p2) {
        PlayerCalls.call(this.handle, "sendSignChange", new Class<?>[]{ org.bukkit.Location.class, java.lang.String[].class, org.bukkit.DyeColor.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void sendSignChange(org.bukkit.Location p0, java.lang.String[] p1, org.bukkit.DyeColor p2, boolean p3) {
        PlayerCalls.call(this.handle, "sendSignChange", new Class<?>[]{ org.bukkit.Location.class, java.lang.String[].class, org.bukkit.DyeColor.class, boolean.class }, new Object[]{ p0, p1, p2, p3 }, void.class);
    }

    @Override public void sendSignChange(org.bukkit.Location p0, java.util.List p1, org.bukkit.DyeColor p2) {
        PlayerCalls.call(this.handle, "sendSignChange", new Class<?>[]{ org.bukkit.Location.class, java.util.List.class, org.bukkit.DyeColor.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void sendSignChange(org.bukkit.Location p0, java.util.List p1) {
        PlayerCalls.call(this.handle, "sendSignChange", new Class<?>[]{ org.bukkit.Location.class, java.util.List.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setPlayerListFooter(java.lang.String p0) {
        PlayerCalls.call(this.handle, "setPlayerListFooter", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public java.lang.String getPlayerListFooter() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getPlayerListFooter", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public void setResourcePack(java.lang.String p0, byte[] p1, java.lang.String p2, boolean p3) {
        PlayerCalls.call(this.handle, "setResourcePack", new Class<?>[]{ java.lang.String.class, byte[].class, java.lang.String.class, boolean.class }, new Object[]{ p0, p1, p2, p3 }, void.class);
    }

    @Override public void setResourcePack(java.lang.String p0, byte[] p1, net.kyori.adventure.text.Component p2, boolean p3) {
        PlayerCalls.call(this.handle, "setResourcePack", new Class<?>[]{ java.lang.String.class, byte[].class, net.kyori.adventure.text.Component.class, boolean.class }, new Object[]{ p0, p1, p2, p3 }, void.class);
    }

    @Override public void setResourcePack(java.util.UUID p0, java.lang.String p1, byte[] p2, java.lang.String p3, boolean p4) {
        PlayerCalls.call(this.handle, "setResourcePack", new Class<?>[]{ java.util.UUID.class, java.lang.String.class, byte[].class, java.lang.String.class, boolean.class }, new Object[]{ p0, p1, p2, p3, p4 }, void.class);
    }

    @Override public void setResourcePack(java.util.UUID p0, java.lang.String p1, byte[] p2, net.kyori.adventure.text.Component p3, boolean p4) {
        PlayerCalls.call(this.handle, "setResourcePack", new Class<?>[]{ java.util.UUID.class, java.lang.String.class, byte[].class, net.kyori.adventure.text.Component.class, boolean.class }, new Object[]{ p0, p1, p2, p3, p4 }, void.class);
    }

    @Override public void setResourcePack(java.lang.String p0, byte[] p1) {
        PlayerCalls.call(this.handle, "setResourcePack", new Class<?>[]{ java.lang.String.class, byte[].class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setResourcePack(java.lang.String p0, byte[] p1, java.lang.String p2) {
        PlayerCalls.call(this.handle, "setResourcePack", new Class<?>[]{ java.lang.String.class, byte[].class, java.lang.String.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void setResourcePack(java.lang.String p0, byte[] p1, net.kyori.adventure.text.Component p2) {
        PlayerCalls.call(this.handle, "setResourcePack", new Class<?>[]{ java.lang.String.class, byte[].class, net.kyori.adventure.text.Component.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void setResourcePack(java.lang.String p0, byte[] p1, boolean p2) {
        PlayerCalls.call(this.handle, "setResourcePack", new Class<?>[]{ java.lang.String.class, byte[].class, boolean.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void setResourcePack(java.lang.String p0, java.lang.String p1, boolean p2, net.kyori.adventure.text.Component p3) {
        PlayerCalls.call(this.handle, "setResourcePack", new Class<?>[]{ java.lang.String.class, java.lang.String.class, boolean.class, net.kyori.adventure.text.Component.class }, new Object[]{ p0, p1, p2, p3 }, void.class);
    }

    @Override public void setResourcePack(java.util.UUID p0, java.lang.String p1, java.lang.String p2, net.kyori.adventure.text.Component p3, boolean p4) {
        PlayerCalls.call(this.handle, "setResourcePack", new Class<?>[]{ java.util.UUID.class, java.lang.String.class, java.lang.String.class, net.kyori.adventure.text.Component.class, boolean.class }, new Object[]{ p0, p1, p2, p3, p4 }, void.class);
    }

    @Override public void setResourcePack(java.lang.String p0, java.lang.String p1) {
        PlayerCalls.call(this.handle, "setResourcePack", new Class<?>[]{ java.lang.String.class, java.lang.String.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setResourcePack(java.lang.String p0, java.lang.String p1, boolean p2) {
        PlayerCalls.call(this.handle, "setResourcePack", new Class<?>[]{ java.lang.String.class, java.lang.String.class, boolean.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void setResourcePack(java.lang.String p0) {
        PlayerCalls.call(this.handle, "setResourcePack", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.Location getCompassTarget() {
        return (org.bukkit.Location) PlayerCalls.call(this.handle, "getCompassTarget", new Class<?>[0], new Object[]{  }, org.bukkit.Location.class);
    }

    @Override public void sendBlockDamage(org.bukkit.Location p0, float p1, int p2) {
        PlayerCalls.call(this.handle, "sendBlockDamage", new Class<?>[]{ org.bukkit.Location.class, float.class, int.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void sendBlockDamage(org.bukkit.Location p0, float p1) {
        PlayerCalls.call(this.handle, "sendBlockDamage", new Class<?>[]{ org.bukkit.Location.class, float.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendBlockDamage(org.bukkit.Location p0, float p1, org.bukkit.entity.Entity p2) {
        PlayerCalls.call(this.handle, "sendBlockDamage", new Class<?>[]{ org.bukkit.Location.class, float.class, org.bukkit.entity.Entity.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public java.util.concurrent.CompletableFuture retrieveCookie(org.bukkit.NamespacedKey p0) {
        return (java.util.concurrent.CompletableFuture) PlayerCalls.call(this.handle, "retrieveCookie", new Class<?>[]{ org.bukkit.NamespacedKey.class }, new Object[]{ p0 }, java.util.concurrent.CompletableFuture.class);
    }

    @Override public boolean isSprinting() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isSprinting", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void setSneaking(boolean p0) {
        PlayerCalls.call(this.handle, "setSneaking", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void storeCookie(org.bukkit.NamespacedKey p0, byte[] p1) {
        PlayerCalls.call(this.handle, "storeCookie", new Class<?>[]{ org.bukkit.NamespacedKey.class, byte[].class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public boolean isTransferred() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isTransferred", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void sendRawMessage(java.lang.String p0) {
        PlayerCalls.call(this.handle, "sendRawMessage", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean performCommand(java.lang.String p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "performCommand", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public void setSleepingIgnored(boolean p0) {
        PlayerCalls.call(this.handle, "setSleepingIgnored", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean isSleepingIgnored() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isSleepingIgnored", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public java.net.SocketAddress getSocketAddress() {
        return (java.net.SocketAddress) PlayerCalls.call(this.handle, "getSocketAddress", new Class<?>[0], new Object[]{  }, java.net.SocketAddress.class);
    }

    @Override public void setSprinting(boolean p0) {
        PlayerCalls.call(this.handle, "setSprinting", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public java.util.Collection getEnderPearls() {
        return (java.util.Collection) PlayerCalls.call(this.handle, "getEnderPearls", new Class<?>[0], new Object[]{  }, java.util.Collection.class);
    }

    @Override public java.net.InetSocketAddress getHAProxyAddress() {
        return (java.net.InetSocketAddress) PlayerCalls.call(this.handle, "getHAProxyAddress", new Class<?>[0], new Object[]{  }, java.net.InetSocketAddress.class);
    }

    @Override public org.bukkit.Input getCurrentInput() {
        return (org.bukkit.Input) PlayerCalls.call(this.handle, "getCurrentInput", new Class<?>[0], new Object[]{  }, org.bukkit.Input.class);
    }

    @Override public int calculateTotalExperiencePoints() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "calculateTotalExperiencePoints", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void setExperienceLevelAndProgress(int p0) {
        PlayerCalls.call(this.handle, "setExperienceLevelAndProgress", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void addAdditionalChatCompletions(java.util.Collection p0) {
        PlayerCalls.call(this.handle, "addAdditionalChatCompletions", new Class<?>[]{ java.util.Collection.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getExperiencePointsNeededForNextLevel() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getExperiencePointsNeededForNextLevel", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void removeAdditionalChatCompletions(java.util.Collection p0) {
        PlayerCalls.call(this.handle, "removeAdditionalChatCompletions", new Class<?>[]{ java.util.Collection.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getWardenTimeSinceLastWarning() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getWardenTimeSinceLastWarning", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void setWardenTimeSinceLastWarning(int p0) {
        PlayerCalls.call(this.handle, "setWardenTimeSinceLastWarning", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void applySkinToPlayerHeadContents(net.kyori.adventure.text.object.PlayerHeadObjectContents.Builder p0) {
        PlayerCalls.call(this.handle, "applySkinToPlayerHeadContents", new Class<?>[]{ net.kyori.adventure.text.object.PlayerHeadObjectContents.Builder.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendPotionEffectChangeRemove(org.bukkit.entity.LivingEntity p0, org.bukkit.potion.PotionEffectType p1) {
        PlayerCalls.call(this.handle, "sendPotionEffectChangeRemove", new Class<?>[]{ org.bukkit.entity.LivingEntity.class, org.bukkit.potion.PotionEffectType.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void removeCustomChatCompletions(java.util.Collection p0) {
        PlayerCalls.call(this.handle, "removeCustomChatCompletions", new Class<?>[]{ java.util.Collection.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean sleep(org.bukkit.Location p0, boolean p1) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "sleep", new Class<?>[]{ org.bukkit.Location.class, boolean.class }, new Object[]{ p0, p1 }, boolean.class);
    }

    @Override public boolean isBlocking() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isBlocking", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public int discoverRecipes(java.util.Collection p0) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "discoverRecipes", new Class<?>[]{ java.util.Collection.class }, new Object[]{ p0 }, int.class)).intValue();
    }

    @Override public void wakeup(boolean p0) {
        PlayerCalls.call(this.handle, "wakeup", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.inventory.InventoryView openLoom(org.bukkit.Location p0, boolean p1) {
        return (org.bukkit.inventory.InventoryView) PlayerCalls.call(this.handle, "openLoom", new Class<?>[]{ org.bukkit.Location.class, boolean.class }, new Object[]{ p0, p1 }, org.bukkit.inventory.InventoryView.class);
    }

    @Override public org.bukkit.inventory.InventoryView openAnvil(org.bukkit.Location p0, boolean p1) {
        return (org.bukkit.inventory.InventoryView) PlayerCalls.call(this.handle, "openAnvil", new Class<?>[]{ org.bukkit.Location.class, boolean.class }, new Object[]{ p0, p1 }, org.bukkit.inventory.InventoryView.class);
    }

    @Override public org.bukkit.entity.Item dropItem(int p0, int p1, boolean p2, java.util.function.Consumer p3) {
        return (org.bukkit.entity.Item) PlayerCalls.call(this.handle, "dropItem", new Class<?>[]{ int.class, int.class, boolean.class, java.util.function.Consumer.class }, new Object[]{ p0, p1, p2, p3 }, org.bukkit.entity.Item.class);
    }

    @Override public org.bukkit.entity.Item dropItem(org.bukkit.inventory.EquipmentSlot p0, int p1, boolean p2, java.util.function.Consumer p3) {
        return (org.bukkit.entity.Item) PlayerCalls.call(this.handle, "dropItem", new Class<?>[]{ org.bukkit.inventory.EquipmentSlot.class, int.class, boolean.class, java.util.function.Consumer.class }, new Object[]{ p0, p1, p2, p3 }, org.bukkit.entity.Item.class);
    }

    @Override public org.bukkit.entity.Item dropItem(org.bukkit.inventory.ItemStack p0) {
        return (org.bukkit.entity.Item) PlayerCalls.call(this.handle, "dropItem", new Class<?>[]{ org.bukkit.inventory.ItemStack.class }, new Object[]{ p0 }, org.bukkit.entity.Item.class);
    }

    @Override public org.bukkit.entity.Item dropItem(org.bukkit.inventory.ItemStack p0, boolean p1, java.util.function.Consumer p2) {
        return (org.bukkit.entity.Item) PlayerCalls.call(this.handle, "dropItem", new Class<?>[]{ org.bukkit.inventory.ItemStack.class, boolean.class, java.util.function.Consumer.class }, new Object[]{ p0, p1, p2 }, org.bukkit.entity.Item.class);
    }

    @Override public org.bukkit.entity.Item dropItem(int p0) {
        return (org.bukkit.entity.Item) PlayerCalls.call(this.handle, "dropItem", new Class<?>[]{ int.class }, new Object[]{ p0 }, org.bukkit.entity.Item.class);
    }

    @Override public org.bukkit.entity.Item dropItem(org.bukkit.inventory.EquipmentSlot p0) {
        return (org.bukkit.entity.Item) PlayerCalls.call(this.handle, "dropItem", new Class<?>[]{ org.bukkit.inventory.EquipmentSlot.class }, new Object[]{ p0 }, org.bukkit.entity.Item.class);
    }

    @Override public org.bukkit.entity.Item dropItem(org.bukkit.inventory.EquipmentSlot p0, int p1) {
        return (org.bukkit.entity.Item) PlayerCalls.call(this.handle, "dropItem", new Class<?>[]{ org.bukkit.inventory.EquipmentSlot.class, int.class }, new Object[]{ p0, p1 }, org.bukkit.entity.Item.class);
    }

    @Override public org.bukkit.entity.Item dropItem(int p0, int p1) {
        return (org.bukkit.entity.Item) PlayerCalls.call(this.handle, "dropItem", new Class<?>[]{ int.class, int.class }, new Object[]{ p0, p1 }, org.bukkit.entity.Item.class);
    }

    @Override public boolean dropItem(boolean p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "dropItem", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public org.bukkit.inventory.InventoryView openMerchant(org.bukkit.inventory.Merchant p0, boolean p1) {
        return (org.bukkit.inventory.InventoryView) PlayerCalls.call(this.handle, "openMerchant", new Class<?>[]{ org.bukkit.inventory.Merchant.class, boolean.class }, new Object[]{ p0, p1 }, org.bukkit.inventory.InventoryView.class);
    }

    @Override public org.bukkit.inventory.InventoryView openMerchant(org.bukkit.entity.Villager p0, boolean p1) {
        return (org.bukkit.inventory.InventoryView) PlayerCalls.call(this.handle, "openMerchant", new Class<?>[]{ org.bukkit.entity.Villager.class, boolean.class }, new Object[]{ p0, p1 }, org.bukkit.inventory.InventoryView.class);
    }

    @Override public void closeInventory(org.bukkit.event.inventory.InventoryCloseEvent.Reason p0) {
        PlayerCalls.call(this.handle, "closeInventory", new Class<?>[]{ org.bukkit.event.inventory.InventoryCloseEvent.Reason.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void closeInventory() {
        PlayerCalls.call(this.handle, "closeInventory", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void setCooldown(org.bukkit.Material p0, int p1) {
        PlayerCalls.call(this.handle, "setCooldown", new Class<?>[]{ org.bukkit.Material.class, int.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setCooldown(org.bukkit.inventory.ItemStack p0, int p1) {
        PlayerCalls.call(this.handle, "setCooldown", new Class<?>[]{ org.bukkit.inventory.ItemStack.class, int.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setCooldown(net.kyori.adventure.key.Key p0, int p1) {
        PlayerCalls.call(this.handle, "setCooldown", new Class<?>[]{ net.kyori.adventure.key.Key.class, int.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public org.bukkit.entity.Firework fireworkBoost() {
        return (org.bukkit.entity.Firework) PlayerCalls.call(this.handle, "fireworkBoost", new Class<?>[0], new Object[]{  }, org.bukkit.entity.Firework.class);
    }

    @Override public org.bukkit.entity.Firework fireworkBoost(org.bukkit.inventory.ItemStack p0) {
        return (org.bukkit.entity.Firework) PlayerCalls.call(this.handle, "fireworkBoost", new Class<?>[]{ org.bukkit.inventory.ItemStack.class }, new Object[]{ p0 }, org.bukkit.entity.Firework.class);
    }

    @Override public org.bukkit.Location getPotentialRespawnLocation() {
        return (org.bukkit.Location) PlayerCalls.call(this.handle, "getPotentialRespawnLocation", new Class<?>[0], new Object[]{  }, org.bukkit.Location.class);
    }

    @Override public void startRiptideAttack(int p0, float p1, org.bukkit.inventory.ItemStack p2) {
        PlayerCalls.call(this.handle, "startRiptideAttack", new Class<?>[]{ int.class, float.class, org.bukkit.inventory.ItemStack.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public int getCooldown(net.kyori.adventure.key.Key p0) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getCooldown", new Class<?>[]{ net.kyori.adventure.key.Key.class }, new Object[]{ p0 }, int.class)).intValue();
    }

    @Override public int getCooldown(org.bukkit.inventory.ItemStack p0) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getCooldown", new Class<?>[]{ org.bukkit.inventory.ItemStack.class }, new Object[]{ p0 }, int.class)).intValue();
    }

    @Override public int getCooldown(org.bukkit.Material p0) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getCooldown", new Class<?>[]{ org.bukkit.Material.class }, new Object[]{ p0 }, int.class)).intValue();
    }

    @Override public org.bukkit.inventory.InventoryView openGrindstone(org.bukkit.Location p0, boolean p1) {
        return (org.bukkit.inventory.InventoryView) PlayerCalls.call(this.handle, "openGrindstone", new Class<?>[]{ org.bukkit.Location.class, boolean.class }, new Object[]{ p0, p1 }, org.bukkit.inventory.InventoryView.class);
    }

    @Override public int getSleepTicks() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getSleepTicks", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.GameMode getGameMode() {
        return (org.bukkit.GameMode) PlayerCalls.call(this.handle, "getGameMode", new Class<?>[0], new Object[]{  }, org.bukkit.GameMode.class);
    }

    @Override public org.bukkit.inventory.InventoryView openSmithingTable(org.bukkit.Location p0, boolean p1) {
        return (org.bukkit.inventory.InventoryView) PlayerCalls.call(this.handle, "openSmithingTable", new Class<?>[]{ org.bukkit.Location.class, boolean.class }, new Object[]{ p0, p1 }, org.bukkit.inventory.InventoryView.class);
    }

    @Override public void setHurtDirection(float p0) {
        PlayerCalls.call(this.handle, "setHurtDirection", new Class<?>[]{ float.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.inventory.InventoryView openEnchanting(org.bukkit.Location p0, boolean p1) {
        return (org.bukkit.inventory.InventoryView) PlayerCalls.call(this.handle, "openEnchanting", new Class<?>[]{ org.bukkit.Location.class, boolean.class }, new Object[]{ p0, p1 }, org.bukkit.inventory.InventoryView.class);
    }

    @Override public void setItemInHand(org.bukkit.inventory.ItemStack p0) {
        PlayerCalls.call(this.handle, "setItemInHand", new Class<?>[]{ org.bukkit.inventory.ItemStack.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.inventory.InventoryView openWorkbench(org.bukkit.Location p0, boolean p1) {
        return (org.bukkit.inventory.InventoryView) PlayerCalls.call(this.handle, "openWorkbench", new Class<?>[]{ org.bukkit.Location.class, boolean.class }, new Object[]{ p0, p1 }, org.bukkit.inventory.InventoryView.class);
    }

    @Override public org.bukkit.entity.Entity releaseLeftShoulderEntity() {
        return (org.bukkit.entity.Entity) PlayerCalls.call(this.handle, "releaseLeftShoulderEntity", new Class<?>[0], new Object[]{  }, org.bukkit.entity.Entity.class);
    }

    @Override public org.bukkit.inventory.ItemStack getItemInHand() {
        return (org.bukkit.inventory.ItemStack) PlayerCalls.call(this.handle, "getItemInHand", new Class<?>[0], new Object[]{  }, org.bukkit.inventory.ItemStack.class);
    }

    @Override public org.bukkit.entity.Entity getShoulderEntityLeft() {
        return (org.bukkit.entity.Entity) PlayerCalls.call(this.handle, "getShoulderEntityLeft", new Class<?>[0], new Object[]{  }, org.bukkit.entity.Entity.class);
    }

    @Override public void setShoulderEntityLeft(org.bukkit.entity.Entity p0) {
        PlayerCalls.call(this.handle, "setShoulderEntityLeft", new Class<?>[]{ org.bukkit.entity.Entity.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setShoulderEntityRight(org.bukkit.entity.Entity p0) {
        PlayerCalls.call(this.handle, "setShoulderEntityRight", new Class<?>[]{ org.bukkit.entity.Entity.class }, new Object[]{ p0 }, void.class);
    }

    @Override public float getExhaustion() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getExhaustion", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public org.bukkit.entity.Entity getShoulderEntityRight() {
        return (org.bukkit.entity.Entity) PlayerCalls.call(this.handle, "getShoulderEntityRight", new Class<?>[0], new Object[]{  }, org.bukkit.entity.Entity.class);
    }

    @Override public void setExhaustion(float p0) {
        PlayerCalls.call(this.handle, "setExhaustion", new Class<?>[]{ float.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getEnchantmentSeed() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getEnchantmentSeed", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.inventory.InventoryView openStonecutter(org.bukkit.Location p0, boolean p1) {
        return (org.bukkit.inventory.InventoryView) PlayerCalls.call(this.handle, "openStonecutter", new Class<?>[]{ org.bukkit.Location.class, boolean.class }, new Object[]{ p0, p1 }, org.bukkit.inventory.InventoryView.class);
    }

    @Override public void setEnchantmentSeed(int p0) {
        PlayerCalls.call(this.handle, "setEnchantmentSeed", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.inventory.ItemStack getItemOnCursor() {
        return (org.bukkit.inventory.ItemStack) PlayerCalls.call(this.handle, "getItemOnCursor", new Class<?>[0], new Object[]{  }, org.bukkit.inventory.ItemStack.class);
    }

    @Override public int undiscoverRecipes(java.util.Collection p0) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "undiscoverRecipes", new Class<?>[]{ java.util.Collection.class }, new Object[]{ p0 }, int.class)).intValue();
    }

    @Override public void setGameMode(org.bukkit.GameMode p0) {
        PlayerCalls.call(this.handle, "setGameMode", new Class<?>[]{ org.bukkit.GameMode.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean isHandRaised() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isHandRaised", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.Location getPotentialBedLocation() {
        return (org.bukkit.Location) PlayerCalls.call(this.handle, "getPotentialBedLocation", new Class<?>[0], new Object[]{  }, org.bukkit.Location.class);
    }

    @Override public org.bukkit.entity.Entity releaseRightShoulderEntity() {
        return (org.bukkit.entity.Entity) PlayerCalls.call(this.handle, "releaseRightShoulderEntity", new Class<?>[0], new Object[]{  }, org.bukkit.entity.Entity.class);
    }

    @Override public boolean undiscoverRecipe(org.bukkit.NamespacedKey p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "undiscoverRecipe", new Class<?>[]{ org.bukkit.NamespacedKey.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public org.bukkit.inventory.InventoryView openCartographyTable(org.bukkit.Location p0, boolean p1) {
        return (org.bukkit.inventory.InventoryView) PlayerCalls.call(this.handle, "openCartographyTable", new Class<?>[]{ org.bukkit.Location.class, boolean.class }, new Object[]{ p0, p1 }, org.bukkit.inventory.InventoryView.class);
    }

    @Override public org.bukkit.Location getBedLocation() {
        return (org.bukkit.Location) PlayerCalls.call(this.handle, "getBedLocation", new Class<?>[0], new Object[]{  }, org.bukkit.Location.class);
    }

    @Override public boolean isDeeplySleeping() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isDeeplySleeping", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void setSaturation(float p0) {
        PlayerCalls.call(this.handle, "setSaturation", new Class<?>[]{ float.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setFoodLevel(int p0) {
        PlayerCalls.call(this.handle, "setFoodLevel", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean hasDiscoveredRecipe(org.bukkit.NamespacedKey p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasDiscoveredRecipe", new Class<?>[]{ org.bukkit.NamespacedKey.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean canCriticalAttack(org.bukkit.entity.Entity p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "canCriticalAttack", new Class<?>[]{ org.bukkit.entity.Entity.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public java.util.Set getDiscoveredRecipes() {
        return (java.util.Set) PlayerCalls.call(this.handle, "getDiscoveredRecipes", new Class<?>[0], new Object[]{  }, java.util.Set.class);
    }

    @Override public int getSaturatedRegenRate() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getSaturatedRegenRate", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.inventory.MainHand getMainHand() {
        return (org.bukkit.inventory.MainHand) PlayerCalls.call(this.handle, "getMainHand", new Class<?>[0], new Object[]{  }, org.bukkit.inventory.MainHand.class);
    }

    @Override public void setSaturatedRegenRate(int p0) {
        PlayerCalls.call(this.handle, "setSaturatedRegenRate", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean setWindowProperty(org.bukkit.inventory.InventoryView.Property p0, int p1) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "setWindowProperty", new Class<?>[]{ org.bukkit.inventory.InventoryView.Property.class, int.class }, new Object[]{ p0, p1 }, boolean.class);
    }

    @Override public org.bukkit.inventory.EntityEquipment getEquipment() {
        return (org.bukkit.inventory.EntityEquipment) PlayerCalls.call(this.handle, "getEquipment", new Class<?>[0], new Object[]{  }, org.bukkit.inventory.EntityEquipment.class);
    }

    @Override public org.bukkit.inventory.InventoryView getOpenInventory() {
        return (org.bukkit.inventory.InventoryView) PlayerCalls.call(this.handle, "getOpenInventory", new Class<?>[0], new Object[]{  }, org.bukkit.inventory.InventoryView.class);
    }

    @Override public void setItemOnCursor(org.bukkit.inventory.ItemStack p0) {
        PlayerCalls.call(this.handle, "setItemOnCursor", new Class<?>[]{ org.bukkit.inventory.ItemStack.class }, new Object[]{ p0 }, void.class);
    }

    @Override public float getAttackCooldown() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getAttackCooldown", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public int getUnsaturatedRegenRate() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getUnsaturatedRegenRate", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void setUnsaturatedRegenRate(int p0) {
        PlayerCalls.call(this.handle, "setUnsaturatedRegenRate", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getFoodLevel() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getFoodLevel", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public float getSaturation() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getSaturation", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public boolean hasCooldown(org.bukkit.Material p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasCooldown", new Class<?>[]{ org.bukkit.Material.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean hasCooldown(org.bukkit.inventory.ItemStack p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasCooldown", new Class<?>[]{ org.bukkit.inventory.ItemStack.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public org.bukkit.inventory.PlayerInventory getInventory() {
        return (org.bukkit.inventory.PlayerInventory) PlayerCalls.call(this.handle, "getInventory", new Class<?>[0], new Object[]{  }, org.bukkit.inventory.PlayerInventory.class);
    }

    @Override public boolean discoverRecipe(org.bukkit.NamespacedKey p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "discoverRecipe", new Class<?>[]{ org.bukkit.NamespacedKey.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public int getStarvationRate() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getStarvationRate", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.inventory.Inventory getEnderChest() {
        return (org.bukkit.inventory.Inventory) PlayerCalls.call(this.handle, "getEnderChest", new Class<?>[0], new Object[]{  }, org.bukkit.inventory.Inventory.class);
    }

    @Override public org.bukkit.inventory.InventoryView openInventory(org.bukkit.inventory.Inventory p0) {
        return (org.bukkit.inventory.InventoryView) PlayerCalls.call(this.handle, "openInventory", new Class<?>[]{ org.bukkit.inventory.Inventory.class }, new Object[]{ p0 }, org.bukkit.inventory.InventoryView.class);
    }

    @Override public void openInventory(org.bukkit.inventory.InventoryView p0) {
        PlayerCalls.call(this.handle, "openInventory", new Class<?>[]{ org.bukkit.inventory.InventoryView.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setStarvationRate(int p0) {
        PlayerCalls.call(this.handle, "setStarvationRate", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getExpToLevel() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getExpToLevel", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.entity.FishHook getFishHook() {
        return (org.bukkit.entity.FishHook) PlayerCalls.call(this.handle, "getFishHook", new Class<?>[0], new Object[]{  }, org.bukkit.entity.FishHook.class);
    }

    @Override public org.bukkit.Location getLastDeathLocation() {
        return (org.bukkit.Location) PlayerCalls.call(this.handle, "getLastDeathLocation", new Class<?>[0], new Object[]{  }, org.bukkit.Location.class);
    }

    @Override public void setLastDeathLocation(org.bukkit.Location p0) {
        PlayerCalls.call(this.handle, "setLastDeathLocation", new Class<?>[]{ org.bukkit.Location.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setMemory(org.bukkit.entity.memory.MemoryKey p0, java.lang.Object p1) {
        PlayerCalls.call(this.handle, "setMemory", new Class<?>[]{ org.bukkit.entity.memory.MemoryKey.class, java.lang.Object.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public boolean isGliding() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isGliding", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.Sound getFallDamageSoundBig() {
        return (org.bukkit.Sound) PlayerCalls.call(this.handle, "getFallDamageSoundBig", new Class<?>[0], new Object[]{  }, org.bukkit.Sound.class);
    }

    @Override public void setArrowsStuck(int p0) {
        PlayerCalls.call(this.handle, "setArrowsStuck", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean canBreatheUnderwater() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "canBreatheUnderwater", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.Sound getDrinkingSound(org.bukkit.inventory.ItemStack p0) {
        return (org.bukkit.Sound) PlayerCalls.call(this.handle, "getDrinkingSound", new Class<?>[]{ org.bukkit.inventory.ItemStack.class }, new Object[]{ p0 }, org.bukkit.Sound.class);
    }

    @Override public void setShieldBlockingDelay(int p0) {
        PlayerCalls.call(this.handle, "setShieldBlockingDelay", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public io.papermc.paper.world.damagesource.CombatTracker getCombatTracker() {
        return (io.papermc.paper.world.damagesource.CombatTracker) PlayerCalls.call(this.handle, "getCombatTracker", new Class<?>[0], new Object[]{  }, io.papermc.paper.world.damagesource.CombatTracker.class);
    }

    @Override public void setWaypointColor(org.bukkit.Color p0) {
        PlayerCalls.call(this.handle, "setWaypointColor", new Class<?>[]{ org.bukkit.Color.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getHandRaisedTime() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getHandRaisedTime", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.inventory.ItemStack damageItemStack(org.bukkit.inventory.ItemStack p0, int p1) {
        return (org.bukkit.inventory.ItemStack) PlayerCalls.call(this.handle, "damageItemStack", new Class<?>[]{ org.bukkit.inventory.ItemStack.class, int.class }, new Object[]{ p0, p1 }, org.bukkit.inventory.ItemStack.class);
    }

    @Override public void damageItemStack(org.bukkit.inventory.EquipmentSlot p0, int p1) {
        PlayerCalls.call(this.handle, "damageItemStack", new Class<?>[]{ org.bukkit.inventory.EquipmentSlot.class, int.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public net.kyori.adventure.key.Key getWaypointStyle() {
        return (net.kyori.adventure.key.Key) PlayerCalls.call(this.handle, "getWaypointStyle", new Class<?>[0], new Object[]{  }, net.kyori.adventure.key.Key.class);
    }

    @Override public int getShieldBlockingDelay() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getShieldBlockingDelay", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public int getItemUseRemainingTime() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getItemUseRemainingTime", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void setActiveItemRemainingTime(int p0) {
        PlayerCalls.call(this.handle, "setActiveItemRemainingTime", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.inventory.EquipmentSlot getHandRaised() {
        return (org.bukkit.inventory.EquipmentSlot) PlayerCalls.call(this.handle, "getHandRaised", new Class<?>[0], new Object[]{  }, org.bukkit.inventory.EquipmentSlot.class);
    }

    @Override public void completeUsingActiveItem() {
        PlayerCalls.call(this.handle, "completeUsingActiveItem", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public float getHurtDirection() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getHurtDirection", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public void startUsingItem(org.bukkit.inventory.EquipmentSlot p0) {
        PlayerCalls.call(this.handle, "startUsingItem", new Class<?>[]{ org.bukkit.inventory.EquipmentSlot.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getArrowsStuck() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getArrowsStuck", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public float getForwardsMovement() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getForwardsMovement", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public void broadcastSlotBreak(org.bukkit.inventory.EquipmentSlot p0, java.util.Collection p1) {
        PlayerCalls.call(this.handle, "broadcastSlotBreak", new Class<?>[]{ org.bukkit.inventory.EquipmentSlot.class, java.util.Collection.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void broadcastSlotBreak(org.bukkit.inventory.EquipmentSlot p0) {
        PlayerCalls.call(this.handle, "broadcastSlotBreak", new Class<?>[]{ org.bukkit.inventory.EquipmentSlot.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean canUseEquipmentSlot(org.bukkit.inventory.EquipmentSlot p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "canUseEquipmentSlot", new Class<?>[]{ org.bukkit.inventory.EquipmentSlot.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public float getUpwardsMovement() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getUpwardsMovement", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public void setWaypointStyle(net.kyori.adventure.key.Key p0) {
        PlayerCalls.call(this.handle, "setWaypointStyle", new Class<?>[]{ net.kyori.adventure.key.Key.class }, new Object[]{ p0 }, void.class);
    }

    @Override public float getSidewaysMovement() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getSidewaysMovement", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public org.bukkit.Color getWaypointColor() {
        return (org.bukkit.Color) PlayerCalls.call(this.handle, "getWaypointColor", new Class<?>[0], new Object[]{  }, org.bukkit.Color.class);
    }

    @Override public org.bukkit.inventory.ItemStack getActiveItem() {
        return (org.bukkit.inventory.ItemStack) PlayerCalls.call(this.handle, "getActiveItem", new Class<?>[0], new Object[]{  }, org.bukkit.inventory.ItemStack.class);
    }

    @Override public void clearActiveItem() {
        PlayerCalls.call(this.handle, "clearActiveItem", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public org.bukkit.Sound getEatingSound(org.bukkit.inventory.ItemStack p0) {
        return (org.bukkit.Sound) PlayerCalls.call(this.handle, "getEatingSound", new Class<?>[]{ org.bukkit.inventory.ItemStack.class }, new Object[]{ p0 }, org.bukkit.Sound.class);
    }

    @Override public org.bukkit.entity.EntityCategory getCategory() {
        return (org.bukkit.entity.EntityCategory) PlayerCalls.call(this.handle, "getCategory", new Class<?>[0], new Object[]{  }, org.bukkit.entity.EntityCategory.class);
    }

    @Override public int getMaximumAir() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getMaximumAir", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void setBeeStingersInBody(int p0) {
        PlayerCalls.call(this.handle, "setBeeStingersInBody", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getNextBeeStingerRemoval() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getNextBeeStingerRemoval", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.block.Block getTargetBlockExact(int p0, org.bukkit.FluidCollisionMode p1) {
        return (org.bukkit.block.Block) PlayerCalls.call(this.handle, "getTargetBlockExact", new Class<?>[]{ int.class, org.bukkit.FluidCollisionMode.class }, new Object[]{ p0, p1 }, org.bukkit.block.Block.class);
    }

    @Override public org.bukkit.block.Block getTargetBlockExact(int p0) {
        return (org.bukkit.block.Block) PlayerCalls.call(this.handle, "getTargetBlockExact", new Class<?>[]{ int.class }, new Object[]{ p0 }, org.bukkit.block.Block.class);
    }

    @Override public double getLastDamage() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getLastDamage", new Class<?>[0], new Object[]{  }, double.class)).doubleValue();
    }

    @Override public org.bukkit.potion.PotionEffect getPotionEffect(org.bukkit.potion.PotionEffectType p0) {
        return (org.bukkit.potion.PotionEffect) PlayerCalls.call(this.handle, "getPotionEffect", new Class<?>[]{ org.bukkit.potion.PotionEffectType.class }, new Object[]{ p0 }, org.bukkit.potion.PotionEffect.class);
    }

    @Override public void removePotionEffect(org.bukkit.potion.PotionEffectType p0) {
        PlayerCalls.call(this.handle, "removePotionEffect", new Class<?>[]{ org.bukkit.potion.PotionEffectType.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setArrowCooldown(int p0) {
        PlayerCalls.call(this.handle, "setArrowCooldown", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getActiveItemRemainingTime() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getActiveItemRemainingTime", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public java.util.Collection getActivePotionEffects() {
        return (java.util.Collection) PlayerCalls.call(this.handle, "getActivePotionEffects", new Class<?>[0], new Object[]{  }, java.util.Collection.class);
    }

    @Override public org.bukkit.block.Block getTargetBlock(int p0) {
        return (org.bukkit.block.Block) PlayerCalls.call(this.handle, "getTargetBlock", new Class<?>[]{ int.class }, new Object[]{ p0 }, org.bukkit.block.Block.class);
    }

    @Override public org.bukkit.block.Block getTargetBlock(int p0, com.destroystokyo.paper.block.TargetBlockInfo.FluidMode p1) {
        return (org.bukkit.block.Block) PlayerCalls.call(this.handle, "getTargetBlock", new Class<?>[]{ int.class, com.destroystokyo.paper.block.TargetBlockInfo.FluidMode.class }, new Object[]{ p0, p1 }, org.bukkit.block.Block.class);
    }

    @Override public org.bukkit.block.Block getTargetBlock(java.util.Set p0, int p1) {
        return (org.bukkit.block.Block) PlayerCalls.call(this.handle, "getTargetBlock", new Class<?>[]{ java.util.Set.class, int.class }, new Object[]{ p0, p1 }, org.bukkit.block.Block.class);
    }

    @Override public void swingMainHand() {
        PlayerCalls.call(this.handle, "swingMainHand", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public boolean clearActivePotionEffects() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "clearActivePotionEffects", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean hasLineOfSight(org.bukkit.Location p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasLineOfSight", new Class<?>[]{ org.bukkit.Location.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean hasLineOfSight(org.bukkit.entity.Entity p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasLineOfSight", new Class<?>[]{ org.bukkit.entity.Entity.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean hasActiveItem() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasActiveItem", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public int getBeeStingersInBody() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getBeeStingersInBody", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public boolean addPotionEffects(java.util.Collection p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "addPotionEffects", new Class<?>[]{ java.util.Collection.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean getRemoveWhenFarAway() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "getRemoveWhenFarAway", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void setRemoveWhenFarAway(boolean p0) {
        PlayerCalls.call(this.handle, "setRemoveWhenFarAway", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setBeeStingerCooldown(int p0) {
        PlayerCalls.call(this.handle, "setBeeStingerCooldown", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.inventory.EquipmentSlot getActiveItemHand() {
        return (org.bukkit.inventory.EquipmentSlot) PlayerCalls.call(this.handle, "getActiveItemHand", new Class<?>[0], new Object[]{  }, org.bukkit.inventory.EquipmentSlot.class);
    }

    @Override public void swingOffHand() {
        PlayerCalls.call(this.handle, "swingOffHand", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public com.destroystokyo.paper.entity.TargetEntityInfo getTargetEntityInfo(int p0, boolean p1) {
        return (com.destroystokyo.paper.entity.TargetEntityInfo) PlayerCalls.call(this.handle, "getTargetEntityInfo", new Class<?>[]{ int.class, boolean.class }, new Object[]{ p0, p1 }, com.destroystokyo.paper.entity.TargetEntityInfo.class);
    }

    @Override public com.destroystokyo.paper.entity.TargetEntityInfo getTargetEntityInfo(int p0) {
        return (com.destroystokyo.paper.entity.TargetEntityInfo) PlayerCalls.call(this.handle, "getTargetEntityInfo", new Class<?>[]{ int.class }, new Object[]{ p0 }, com.destroystokyo.paper.entity.TargetEntityInfo.class);
    }

    @Override public java.util.List getLastTwoTargetBlocks(java.util.Set p0, int p1) {
        return (java.util.List) PlayerCalls.call(this.handle, "getLastTwoTargetBlocks", new Class<?>[]{ java.util.Set.class, int.class }, new Object[]{ p0, p1 }, java.util.List.class);
    }

    @Override public int getItemInUseTicks() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getItemInUseTicks", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public com.destroystokyo.paper.block.TargetBlockInfo getTargetBlockInfo(int p0) {
        return (com.destroystokyo.paper.block.TargetBlockInfo) PlayerCalls.call(this.handle, "getTargetBlockInfo", new Class<?>[]{ int.class }, new Object[]{ p0 }, com.destroystokyo.paper.block.TargetBlockInfo.class);
    }

    @Override public com.destroystokyo.paper.block.TargetBlockInfo getTargetBlockInfo(int p0, com.destroystokyo.paper.block.TargetBlockInfo.FluidMode p1) {
        return (com.destroystokyo.paper.block.TargetBlockInfo) PlayerCalls.call(this.handle, "getTargetBlockInfo", new Class<?>[]{ int.class, com.destroystokyo.paper.block.TargetBlockInfo.FluidMode.class }, new Object[]{ p0, p1 }, com.destroystokyo.paper.block.TargetBlockInfo.class);
    }

    @Override public double getEyeHeight(boolean p0) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getEyeHeight", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, double.class)).doubleValue();
    }

    @Override public double getEyeHeight() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getEyeHeight", new Class<?>[0], new Object[]{  }, double.class)).doubleValue();
    }

    @Override public void setRemainingAir(int p0) {
        PlayerCalls.call(this.handle, "setRemainingAir", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setMaximumAir(int p0) {
        PlayerCalls.call(this.handle, "setMaximumAir", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.util.RayTraceResult rayTraceEntities(int p0, boolean p1) {
        return (org.bukkit.util.RayTraceResult) PlayerCalls.call(this.handle, "rayTraceEntities", new Class<?>[]{ int.class, boolean.class }, new Object[]{ p0, p1 }, org.bukkit.util.RayTraceResult.class);
    }

    @Override public org.bukkit.util.RayTraceResult rayTraceEntities(int p0) {
        return (org.bukkit.util.RayTraceResult) PlayerCalls.call(this.handle, "rayTraceEntities", new Class<?>[]{ int.class }, new Object[]{ p0 }, org.bukkit.util.RayTraceResult.class);
    }

    @Override public void setItemInUseTicks(int p0) {
        PlayerCalls.call(this.handle, "setItemInUseTicks", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void playPickupItemAnimation(org.bukkit.entity.Item p0, int p1) {
        PlayerCalls.call(this.handle, "playPickupItemAnimation", new Class<?>[]{ org.bukkit.entity.Item.class, int.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void playPickupItemAnimation(org.bukkit.entity.Item p0) {
        PlayerCalls.call(this.handle, "playPickupItemAnimation", new Class<?>[]{ org.bukkit.entity.Item.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setLastDamage(double p0) {
        PlayerCalls.call(this.handle, "setLastDamage", new Class<?>[]{ double.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setNoDamageTicks(int p0) {
        PlayerCalls.call(this.handle, "setNoDamageTicks", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public java.util.List getLineOfSight(java.util.Set p0, int p1) {
        return (java.util.List) PlayerCalls.call(this.handle, "getLineOfSight", new Class<?>[]{ java.util.Set.class, int.class }, new Object[]{ p0, p1 }, java.util.List.class);
    }

    @Override public org.bukkit.entity.Entity getTargetEntity(int p0, boolean p1) {
        return (org.bukkit.entity.Entity) PlayerCalls.call(this.handle, "getTargetEntity", new Class<?>[]{ int.class, boolean.class }, new Object[]{ p0, p1 }, org.bukkit.entity.Entity.class);
    }

    @Override public org.bukkit.entity.Entity getTargetEntity(int p0) {
        return (org.bukkit.entity.Entity) PlayerCalls.call(this.handle, "getTargetEntity", new Class<?>[]{ int.class }, new Object[]{ p0 }, org.bukkit.entity.Entity.class);
    }

    @Override public boolean addPotionEffect(org.bukkit.potion.PotionEffect p0, boolean p1) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "addPotionEffect", new Class<?>[]{ org.bukkit.potion.PotionEffect.class, boolean.class }, new Object[]{ p0, p1 }, boolean.class);
    }

    @Override public boolean addPotionEffect(org.bukkit.potion.PotionEffect p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "addPotionEffect", new Class<?>[]{ org.bukkit.potion.PotionEffect.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public org.bukkit.Location getEyeLocation() {
        return (org.bukkit.Location) PlayerCalls.call(this.handle, "getEyeLocation", new Class<?>[0], new Object[]{  }, org.bukkit.Location.class);
    }

    @Override public int getRemainingAir() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getRemainingAir", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.util.RayTraceResult rayTraceBlocks(double p0) {
        return (org.bukkit.util.RayTraceResult) PlayerCalls.call(this.handle, "rayTraceBlocks", new Class<?>[]{ double.class }, new Object[]{ p0 }, org.bukkit.util.RayTraceResult.class);
    }

    @Override public org.bukkit.util.RayTraceResult rayTraceBlocks(double p0, org.bukkit.FluidCollisionMode p1) {
        return (org.bukkit.util.RayTraceResult) PlayerCalls.call(this.handle, "rayTraceBlocks", new Class<?>[]{ double.class, org.bukkit.FluidCollisionMode.class }, new Object[]{ p0, p1 }, org.bukkit.util.RayTraceResult.class);
    }

    @Override public org.bukkit.block.BlockFace getTargetBlockFace(int p0, com.destroystokyo.paper.block.TargetBlockInfo.FluidMode p1) {
        return (org.bukkit.block.BlockFace) PlayerCalls.call(this.handle, "getTargetBlockFace", new Class<?>[]{ int.class, com.destroystokyo.paper.block.TargetBlockInfo.FluidMode.class }, new Object[]{ p0, p1 }, org.bukkit.block.BlockFace.class);
    }

    @Override public org.bukkit.block.BlockFace getTargetBlockFace(int p0) {
        return (org.bukkit.block.BlockFace) PlayerCalls.call(this.handle, "getTargetBlockFace", new Class<?>[]{ int.class }, new Object[]{ p0 }, org.bukkit.block.BlockFace.class);
    }

    @Override public org.bukkit.block.BlockFace getTargetBlockFace(int p0, org.bukkit.FluidCollisionMode p1) {
        return (org.bukkit.block.BlockFace) PlayerCalls.call(this.handle, "getTargetBlockFace", new Class<?>[]{ int.class, org.bukkit.FluidCollisionMode.class }, new Object[]{ p0, p1 }, org.bukkit.block.BlockFace.class);
    }

    @Override public void setNextArrowRemoval(int p0) {
        PlayerCalls.call(this.handle, "setNextArrowRemoval", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getNextArrowRemoval() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getNextArrowRemoval", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public int getBeeStingerCooldown() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getBeeStingerCooldown", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public int getArrowsInBody() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getArrowsInBody", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void setArrowsInBody(int p0, boolean p1) {
        PlayerCalls.call(this.handle, "setArrowsInBody", new Class<?>[]{ int.class, boolean.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setArrowsInBody(int p0) {
        PlayerCalls.call(this.handle, "setArrowsInBody", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getActiveItemUsedTime() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getActiveItemUsedTime", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public int getArrowCooldown() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getArrowCooldown", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public org.bukkit.inventory.ItemStack getItemInUse() {
        return (org.bukkit.inventory.ItemStack) PlayerCalls.call(this.handle, "getItemInUse", new Class<?>[0], new Object[]{  }, org.bukkit.inventory.ItemStack.class);
    }

    @Override public int getMaximumNoDamageTicks() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getMaximumNoDamageTicks", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void setMaximumNoDamageTicks(int p0) {
        PlayerCalls.call(this.handle, "setMaximumNoDamageTicks", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getNoActionTicks() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getNoActionTicks", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void setNoActionTicks(int p0) {
        PlayerCalls.call(this.handle, "setNoActionTicks", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public int getNoDamageTicks() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getNoDamageTicks", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public boolean hasPotionEffect(org.bukkit.potion.PotionEffectType p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasPotionEffect", new Class<?>[]{ org.bukkit.potion.PotionEffectType.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public void setNextBeeStingerRemoval(int p0) {
        PlayerCalls.call(this.handle, "setNextBeeStingerRemoval", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public float getSoundVolume() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getSoundVolume", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public org.bukkit.Sound getDeathSound() {
        return (org.bukkit.Sound) PlayerCalls.call(this.handle, "getDeathSound", new Class<?>[0], new Object[]{  }, org.bukkit.Sound.class);
    }

    @Override public org.bukkit.Sound getFallDamageSound(int p0) {
        return (org.bukkit.Sound) PlayerCalls.call(this.handle, "getFallDamageSound", new Class<?>[]{ int.class }, new Object[]{ p0 }, org.bukkit.Sound.class);
    }

    @Override public org.bukkit.Sound getHurtSound() {
        return (org.bukkit.Sound) PlayerCalls.call(this.handle, "getHurtSound", new Class<?>[0], new Object[]{  }, org.bukkit.Sound.class);
    }

    @Override public org.bukkit.Sound getHurtSound(org.bukkit.damage.DamageSource p0) {
        return (org.bukkit.Sound) PlayerCalls.call(this.handle, "getHurtSound", new Class<?>[]{ org.bukkit.damage.DamageSource.class }, new Object[]{ p0 }, org.bukkit.Sound.class);
    }

    @Override public float getSoundPitch() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getSoundPitch", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public org.bukkit.Sound getFallDamageSoundSmall() {
        return (org.bukkit.Sound) PlayerCalls.call(this.handle, "getFallDamageSoundSmall", new Class<?>[0], new Object[]{  }, org.bukkit.Sound.class);
    }

    @Override public boolean setLeashHolder(org.bukkit.entity.Entity p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "setLeashHolder", new Class<?>[]{ org.bukkit.entity.Entity.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean isRiptiding() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isRiptiding", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean getCanPickupItems() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "getCanPickupItems", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void setRiptiding(boolean p0) {
        PlayerCalls.call(this.handle, "setRiptiding", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean isCollidable() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isCollidable", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void playHurtAnimation(float p0) {
        PlayerCalls.call(this.handle, "playHurtAnimation", new Class<?>[]{ float.class }, new Object[]{ p0 }, void.class);
    }

    @Override public java.util.Set getCollidableExemptions() {
        return (java.util.Set) PlayerCalls.call(this.handle, "getCollidableExemptions", new Class<?>[0], new Object[]{  }, java.util.Set.class);
    }

    @Override public void setSwimming(boolean p0) {
        PlayerCalls.call(this.handle, "setSwimming", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setCollidable(boolean p0) {
        PlayerCalls.call(this.handle, "setCollidable", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setCanPickupItems(boolean p0) {
        PlayerCalls.call(this.handle, "setCanPickupItems", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.entity.Entity getLeashHolder() {
        return (org.bukkit.entity.Entity) PlayerCalls.call(this.handle, "getLeashHolder", new Class<?>[0], new Object[]{  }, org.bukkit.entity.Entity.class);
    }

    @Override public org.bukkit.entity.Player getKiller() {
        return (org.bukkit.entity.Player) PlayerCalls.call(this.handle, "getKiller", new Class<?>[0], new Object[]{  }, org.bukkit.entity.Player.class);
    }

    @Override public boolean isJumping() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isJumping", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void setGliding(boolean p0) {
        PlayerCalls.call(this.handle, "setGliding", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setBodyYaw(float p0) {
        PlayerCalls.call(this.handle, "setBodyYaw", new Class<?>[]{ float.class }, new Object[]{ p0 }, void.class);
    }

    @Override public java.lang.Object getMemory(org.bukkit.entity.memory.MemoryKey p0) {
        return (java.lang.Object) PlayerCalls.call(this.handle, "getMemory", new Class<?>[]{ org.bukkit.entity.memory.MemoryKey.class }, new Object[]{ p0 }, java.lang.Object.class);
    }

    @Override public boolean isLeashed() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isLeashed", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void knockback(double p0, double p1, double p2) {
        PlayerCalls.call(this.handle, "knockback", new Class<?>[]{ double.class, double.class, double.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public boolean isClimbing() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isClimbing", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void setJumping(boolean p0) {
        PlayerCalls.call(this.handle, "setJumping", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean isSwimming() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isSwimming", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public float getBodyYaw() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getBodyYaw", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public void setKiller(org.bukkit.entity.Player p0) {
        PlayerCalls.call(this.handle, "setKiller", new Class<?>[]{ org.bukkit.entity.Player.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setAI(boolean p0) {
        PlayerCalls.call(this.handle, "setAI", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean isSleeping() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isSleeping", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void attack(org.bukkit.entity.Entity p0) {
        PlayerCalls.call(this.handle, "attack", new Class<?>[]{ org.bukkit.entity.Entity.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void swingHand(org.bukkit.inventory.EquipmentSlot p0) {
        PlayerCalls.call(this.handle, "swingHand", new Class<?>[]{ org.bukkit.inventory.EquipmentSlot.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean hasAI() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasAI", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.attribute.AttributeInstance getAttribute(org.bukkit.attribute.Attribute p0) {
        return (org.bukkit.attribute.AttributeInstance) PlayerCalls.call(this.handle, "getAttribute", new Class<?>[]{ org.bukkit.attribute.Attribute.class }, new Object[]{ p0 }, org.bukkit.attribute.AttributeInstance.class);
    }

    @Override public void registerAttribute(org.bukkit.attribute.Attribute p0) {
        PlayerCalls.call(this.handle, "registerAttribute", new Class<?>[]{ org.bukkit.attribute.Attribute.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void kill(org.bukkit.damage.DamageSource p0) {
        PlayerCalls.call(this.handle, "kill", new Class<?>[]{ org.bukkit.damage.DamageSource.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void kill() {
        PlayerCalls.call(this.handle, "kill", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void setAbsorptionAmount(double p0) {
        PlayerCalls.call(this.handle, "setAbsorptionAmount", new Class<?>[]{ double.class }, new Object[]{ p0 }, void.class);
    }

    @Override public double getMaxHealth() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getMaxHealth", new Class<?>[0], new Object[]{  }, double.class)).doubleValue();
    }

    @Override public void resetMaxHealth() {
        PlayerCalls.call(this.handle, "resetMaxHealth", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void setMaxHealth(double p0) {
        PlayerCalls.call(this.handle, "setMaxHealth", new Class<?>[]{ double.class }, new Object[]{ p0 }, void.class);
    }

    @Override public double getAbsorptionAmount() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getAbsorptionAmount", new Class<?>[0], new Object[]{  }, double.class)).doubleValue();
    }

    @Override public double getHealth() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getHealth", new Class<?>[0], new Object[]{  }, double.class)).doubleValue();
    }

    @Override public void setHealth(double p0) {
        PlayerCalls.call(this.handle, "setHealth", new Class<?>[]{ double.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void damage(double p0, org.bukkit.entity.Entity p1) {
        PlayerCalls.call(this.handle, "damage", new Class<?>[]{ double.class, org.bukkit.entity.Entity.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void damage(double p0) {
        PlayerCalls.call(this.handle, "damage", new Class<?>[]{ double.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void damage(double p0, org.bukkit.damage.DamageSource p1) {
        PlayerCalls.call(this.handle, "damage", new Class<?>[]{ double.class, org.bukkit.damage.DamageSource.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void heal(double p0) {
        PlayerCalls.call(this.handle, "heal", new Class<?>[]{ double.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void heal(double p0, org.bukkit.event.entity.EntityRegainHealthEvent.RegainReason p1) {
        PlayerCalls.call(this.handle, "heal", new Class<?>[]{ double.class, org.bukkit.event.entity.EntityRegainHealthEvent.RegainReason.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void remove() {
        PlayerCalls.call(this.handle, "remove", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public org.bukkit.entity.Entity copy(org.bukkit.Location p0) {
        return (org.bukkit.entity.Entity) PlayerCalls.call(this.handle, "copy", new Class<?>[]{ org.bukkit.Location.class }, new Object[]{ p0 }, org.bukkit.entity.Entity.class);
    }

    @Override public org.bukkit.entity.Entity copy() {
        return (org.bukkit.entity.Entity) PlayerCalls.call(this.handle, "copy", new Class<?>[0], new Object[]{  }, org.bukkit.entity.Entity.class);
    }

    @Override public boolean isEmpty() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isEmpty", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.Location getLocation(org.bukkit.Location p0) {
        return (org.bukkit.Location) PlayerCalls.call(this.handle, "getLocation", new Class<?>[]{ org.bukkit.Location.class }, new Object[]{ p0 }, org.bukkit.Location.class);
    }

    @Override public org.bukkit.Location getLocation() {
        return (org.bukkit.Location) PlayerCalls.call(this.handle, "getLocation", new Class<?>[0], new Object[]{  }, org.bukkit.Location.class);
    }

    @Override public org.bukkit.entity.EntityType getType() {
        return (org.bukkit.entity.EntityType) PlayerCalls.call(this.handle, "getType", new Class<?>[0], new Object[]{  }, org.bukkit.entity.EntityType.class);
    }

    @Override public boolean isFrozen() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isFrozen", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean isValid() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isValid", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public double getX() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getX", new Class<?>[0], new Object[]{  }, double.class)).doubleValue();
    }

    @Override public double getY() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getY", new Class<?>[0], new Object[]{  }, double.class)).doubleValue();
    }

    @Override public double getZ() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getZ", new Class<?>[0], new Object[]{  }, double.class)).doubleValue();
    }

    @Override public void playEffect(org.bukkit.EntityEffect p0) {
        PlayerCalls.call(this.handle, "playEffect", new Class<?>[]{ org.bukkit.EntityEffect.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.Server getServer() {
        return (org.bukkit.Server) PlayerCalls.call(this.handle, "getServer", new Class<?>[0], new Object[]{  }, org.bukkit.Server.class);
    }

    @Override public void lookAt(double p0, double p1, double p2, io.papermc.paper.entity.LookAnchor p3) {
        PlayerCalls.call(this.handle, "lookAt", new Class<?>[]{ double.class, double.class, double.class, io.papermc.paper.entity.LookAnchor.class }, new Object[]{ p0, p1, p2, p3 }, void.class);
    }

    @Override public void lookAt(io.papermc.paper.math.Position p0, io.papermc.paper.entity.LookAnchor p1) {
        PlayerCalls.call(this.handle, "lookAt", new Class<?>[]{ io.papermc.paper.math.Position.class, io.papermc.paper.entity.LookAnchor.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public java.util.UUID getUniqueId() {
        return (java.util.UUID) PlayerCalls.call(this.handle, "getUniqueId", new Class<?>[0], new Object[]{  }, java.util.UUID.class);
    }

    @Override public int getEntityId() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getEntityId", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public java.util.concurrent.CompletableFuture teleportAsync(org.bukkit.Location p0) {
        return (java.util.concurrent.CompletableFuture) PlayerCalls.call(this.handle, "teleportAsync", new Class<?>[]{ org.bukkit.Location.class }, new Object[]{ p0 }, java.util.concurrent.CompletableFuture.class);
    }

    @Override public java.util.concurrent.CompletableFuture teleportAsync(org.bukkit.Location p0, io.papermc.paper.entity.TeleportFlag[] p1) {
        return (java.util.concurrent.CompletableFuture) PlayerCalls.call(this.handle, "teleportAsync", new Class<?>[]{ org.bukkit.Location.class, io.papermc.paper.entity.TeleportFlag[].class }, new Object[]{ p0, p1 }, java.util.concurrent.CompletableFuture.class);
    }

    @Override public java.util.concurrent.CompletableFuture teleportAsync(org.bukkit.Location p0, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause p1, io.papermc.paper.entity.TeleportFlag[] p2) {
        return (java.util.concurrent.CompletableFuture) PlayerCalls.call(this.handle, "teleportAsync", new Class<?>[]{ org.bukkit.Location.class, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause.class, io.papermc.paper.entity.TeleportFlag[].class }, new Object[]{ p0, p1, p2 }, java.util.concurrent.CompletableFuture.class);
    }

    @Override public java.util.concurrent.CompletableFuture teleportAsync(org.bukkit.Location p0, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause p1) {
        return (java.util.concurrent.CompletableFuture) PlayerCalls.call(this.handle, "teleportAsync", new Class<?>[]{ org.bukkit.Location.class, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause.class }, new Object[]{ p0, p1 }, java.util.concurrent.CompletableFuture.class);
    }

    @Override public boolean isInvulnerableTo(org.bukkit.damage.DamageSource p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isInvulnerableTo", new Class<?>[]{ org.bukkit.damage.DamageSource.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean isInWaterOrRain() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isInWaterOrRain", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void setVelocity(org.bukkit.util.Vector p0) {
        PlayerCalls.call(this.handle, "setVelocity", new Class<?>[]{ org.bukkit.util.Vector.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean setPassenger(org.bukkit.entity.Entity p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "setPassenger", new Class<?>[]{ org.bukkit.entity.Entity.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public void setTicksLived(int p0) {
        PlayerCalls.call(this.handle, "setTicksLived", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public java.util.List getPassengers() {
        return (java.util.List) PlayerCalls.call(this.handle, "getPassengers", new Class<?>[0], new Object[]{  }, java.util.List.class);
    }

    @Override public void lockFreezeTicks(boolean p0) {
        PlayerCalls.call(this.handle, "lockFreezeTicks", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setInvisible(boolean p0) {
        PlayerCalls.call(this.handle, "setInvisible", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.inventory.ItemStack getPickItemStack() {
        return (org.bukkit.inventory.ItemStack) PlayerCalls.call(this.handle, "getPickItemStack", new Class<?>[0], new Object[]{  }, org.bukkit.inventory.ItemStack.class);
    }

    @Override public java.util.List getNearbyEntities(double p0, double p1, double p2) {
        return (java.util.List) PlayerCalls.call(this.handle, "getNearbyEntities", new Class<?>[]{ double.class, double.class, double.class }, new Object[]{ p0, p1, p2 }, java.util.List.class);
    }

    @Override public org.bukkit.SoundCategory getSoundCategory() {
        return (org.bukkit.SoundCategory) PlayerCalls.call(this.handle, "getSoundCategory", new Class<?>[0], new Object[]{  }, org.bukkit.SoundCategory.class);
    }

    @Override public float getFallDistance() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getFallDistance", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public int getFreezeTicks() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getFreezeTicks", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public boolean isPersistent() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isPersistent", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void setVisualFire(boolean p0) {
        PlayerCalls.call(this.handle, "setVisualFire", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setVisualFire(net.kyori.adventure.util.TriState p0) {
        PlayerCalls.call(this.handle, "setVisualFire", new Class<?>[]{ net.kyori.adventure.util.TriState.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.Sound getSwimSplashSound() {
        return (org.bukkit.Sound) PlayerCalls.call(this.handle, "getSwimSplashSound", new Class<?>[0], new Object[]{  }, org.bukkit.Sound.class);
    }

    @Override public org.bukkit.Sound getSwimSound() {
        return (org.bukkit.Sound) PlayerCalls.call(this.handle, "getSwimSound", new Class<?>[0], new Object[]{  }, org.bukkit.Sound.class);
    }

    @Override public boolean isInsideVehicle() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isInsideVehicle", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void setNoPhysics(boolean p0) {
        PlayerCalls.call(this.handle, "setNoPhysics", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setLastDamageCause(org.bukkit.event.entity.EntityDamageEvent p0) {
        PlayerCalls.call(this.handle, "setLastDamageCause", new Class<?>[]{ org.bukkit.event.entity.EntityDamageEvent.class }, new Object[]{ p0 }, void.class);
    }

    @Override public double getDefaultGravity() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getDefaultGravity", new Class<?>[0], new Object[]{  }, double.class)).doubleValue();
    }

    @Override public boolean isMarkedInvulnerable() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isMarkedInvulnerable", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public int getPortalCooldown() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getPortalCooldown", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void setInvulnerable(boolean p0) {
        PlayerCalls.call(this.handle, "setInvulnerable", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean addPassenger(org.bukkit.entity.Entity p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "addPassenger", new Class<?>[]{ org.bukkit.entity.Entity.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public org.bukkit.block.PistonMoveReaction getPistonMoveReaction() {
        return (org.bukkit.block.PistonMoveReaction) PlayerCalls.call(this.handle, "getPistonMoveReaction", new Class<?>[0], new Object[]{  }, org.bukkit.block.PistonMoveReaction.class);
    }

    @Override public void setFireTicks(int p0) {
        PlayerCalls.call(this.handle, "setFireTicks", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public io.papermc.paper.entity.RemovalReason getRemovalReason() {
        return (io.papermc.paper.entity.RemovalReason) PlayerCalls.call(this.handle, "getRemovalReason", new Class<?>[0], new Object[]{  }, io.papermc.paper.entity.RemovalReason.class);
    }

    @Override public void setPersistent(boolean p0) {
        PlayerCalls.call(this.handle, "setPersistent", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.event.entity.EntityRemoveEvent.Cause getRemoveEventCause() {
        return (org.bukkit.event.entity.EntityRemoveEvent.Cause) PlayerCalls.call(this.handle, "getRemoveEventCause", new Class<?>[0], new Object[]{  }, org.bukkit.event.entity.EntityRemoveEvent.Cause.class);
    }

    @Override public void setPortalCooldown(int p0) {
        PlayerCalls.call(this.handle, "setPortalCooldown", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.event.entity.EntityDamageEvent getLastDamageCause() {
        return (org.bukkit.event.entity.EntityDamageEvent) PlayerCalls.call(this.handle, "getLastDamageCause", new Class<?>[0], new Object[]{  }, org.bukkit.event.entity.EntityDamageEvent.class);
    }

    @Override public boolean removeScoreboardTag(java.lang.String p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "removeScoreboardTag", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public void setCustomNameVisible(boolean p0) {
        PlayerCalls.call(this.handle, "setCustomNameVisible", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public net.kyori.adventure.util.TriState getVisualFire() {
        return (net.kyori.adventure.util.TriState) PlayerCalls.call(this.handle, "getVisualFire", new Class<?>[0], new Object[]{  }, net.kyori.adventure.util.TriState.class);
    }

    @Override public int getMaxFreezeTicks() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getMaxFreezeTicks", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public boolean isFreezeTickingLocked() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isFreezeTickingLocked", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public int getTicksLived() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getTicksLived", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public void setFallDistance(float p0) {
        PlayerCalls.call(this.handle, "setFallDistance", new Class<?>[]{ float.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setVisibleByDefault(boolean p0) {
        PlayerCalls.call(this.handle, "setVisibleByDefault", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean isVisibleByDefault() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isVisibleByDefault", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean removePassenger(org.bukkit.entity.Entity p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "removePassenger", new Class<?>[]{ org.bukkit.entity.Entity.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public void setFreezeTicks(int p0) {
        PlayerCalls.call(this.handle, "setFreezeTicks", new Class<?>[]{ int.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean hasNoPhysics() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasNoPhysics", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public java.util.Set getTrackedBy() {
        return (java.util.Set) PlayerCalls.call(this.handle, "getTrackedBy", new Class<?>[0], new Object[]{  }, java.util.Set.class);
    }

    @Override public boolean isInvulnerable() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isInvulnerable", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean hasFixedPose() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasFixedPose", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.entity.SpawnCategory getSpawnCategory() {
        return (org.bukkit.entity.SpawnCategory) PlayerCalls.call(this.handle, "getSpawnCategory", new Class<?>[0], new Object[]{  }, org.bukkit.entity.SpawnCategory.class);
    }

    @Override public org.bukkit.entity.Entity getPassenger() {
        return (org.bukkit.entity.Entity) PlayerCalls.call(this.handle, "getPassenger", new Class<?>[0], new Object[]{  }, org.bukkit.entity.Entity.class);
    }

    @Override public org.bukkit.util.Vector getVelocity() {
        return (org.bukkit.util.Vector) PlayerCalls.call(this.handle, "getVelocity", new Class<?>[0], new Object[]{  }, org.bukkit.util.Vector.class);
    }

    @Override public org.bukkit.util.BoundingBox getBoundingBox() {
        return (org.bukkit.util.BoundingBox) PlayerCalls.call(this.handle, "getBoundingBox", new Class<?>[0], new Object[]{  }, org.bukkit.util.BoundingBox.class);
    }

    @Override public int getFireTicks() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getFireTicks", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public boolean isVisualFire() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isVisualFire", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean leaveVehicle() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "leaveVehicle", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean isTrackedBy(org.bukkit.entity.Player p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isTrackedBy", new Class<?>[]{ org.bukkit.entity.Player.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public java.util.Set getScoreboardTags() {
        return (java.util.Set) PlayerCalls.call(this.handle, "getScoreboardTags", new Class<?>[0], new Object[]{  }, java.util.Set.class);
    }

    @Override public int getMaxFireTicks() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getMaxFireTicks", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public boolean addScoreboardTag(java.lang.String p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "addScoreboardTag", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public java.lang.String getAsString() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getAsString", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public boolean isCustomNameVisible() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isCustomNameVisible", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean isInvisible() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isInvisible", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public io.papermc.paper.threadedregions.scheduler.EntityScheduler getScheduler() {
        return (io.papermc.paper.threadedregions.scheduler.EntityScheduler) PlayerCalls.call(this.handle, "getScheduler", new Class<?>[0], new Object[]{  }, io.papermc.paper.threadedregions.scheduler.EntityScheduler.class);
    }

    @Override public java.util.Map getAdditionalAlliedRules() {
        return (java.util.Map) PlayerCalls.call(this.handle, "getAdditionalAlliedRules", new Class<?>[0], new Object[]{  }, java.util.Map.class);
    }

    @Override public org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason getEntitySpawnReason() {
        return (org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason) PlayerCalls.call(this.handle, "getEntitySpawnReason", new Class<?>[0], new Object[]{  }, org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason.class);
    }

    @Override public boolean isInPowderedSnow() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isInPowderedSnow", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean isUnderWater() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isUnderWater", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.entity.EntitySnapshot createSnapshot() {
        return (org.bukkit.entity.EntitySnapshot) PlayerCalls.call(this.handle, "createSnapshot", new Class<?>[0], new Object[]{  }, org.bukkit.entity.EntitySnapshot.class);
    }

    @Override public void broadcastHurtAnimation(java.util.Collection p0) {
        PlayerCalls.call(this.handle, "broadcastHurtAnimation", new Class<?>[]{ java.util.Collection.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void addAdditionalAlliedRule(net.kyori.adventure.key.Key p0, java.util.function.Predicate p1) {
        PlayerCalls.call(this.handle, "addAdditionalAlliedRule", new Class<?>[]{ net.kyori.adventure.key.Key.class, java.util.function.Predicate.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public java.util.Set getTrackedPlayers() {
        return (java.util.Set) PlayerCalls.call(this.handle, "getTrackedPlayers", new Class<?>[0], new Object[]{  }, java.util.Set.class);
    }

    @Override public boolean isInWaterOrBubbleColumn() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isInWaterOrBubbleColumn", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public net.kyori.adventure.text.Component teamDisplayName() {
        return (net.kyori.adventure.text.Component) PlayerCalls.call(this.handle, "teamDisplayName", new Class<?>[0], new Object[]{  }, net.kyori.adventure.text.Component.class);
    }

    @Override public boolean isInBubbleColumn() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isInBubbleColumn", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean fromMobSpawner() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "fromMobSpawner", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean wouldCollideUsing(org.bukkit.util.BoundingBox p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "wouldCollideUsing", new Class<?>[]{ org.bukkit.util.BoundingBox.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public void removeAdditionalAlliedRule(net.kyori.adventure.key.Namespaced p0) {
        PlayerCalls.call(this.handle, "removeAdditionalAlliedRule", new Class<?>[]{ net.kyori.adventure.key.Namespaced.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void removeAdditionalAlliedRule(net.kyori.adventure.key.Key p0) {
        PlayerCalls.call(this.handle, "removeAdditionalAlliedRule", new Class<?>[]{ net.kyori.adventure.key.Key.class }, new Object[]{ p0 }, void.class);
    }

    @Override public java.lang.String getScoreboardEntryName() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getScoreboardEntryName", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public boolean teleport(org.bukkit.Location p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "teleport", new Class<?>[]{ org.bukkit.Location.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean teleport(org.bukkit.Location p0, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause p1) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "teleport", new Class<?>[]{ org.bukkit.Location.class, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause.class }, new Object[]{ p0, p1 }, boolean.class);
    }

    @Override public boolean teleport(org.bukkit.Location p0, io.papermc.paper.entity.TeleportFlag[] p1) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "teleport", new Class<?>[]{ org.bukkit.Location.class, io.papermc.paper.entity.TeleportFlag[].class }, new Object[]{ p0, p1 }, boolean.class);
    }

    @Override public boolean teleport(org.bukkit.entity.Entity p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "teleport", new Class<?>[]{ org.bukkit.entity.Entity.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean teleport(org.bukkit.entity.Entity p0, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause p1) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "teleport", new Class<?>[]{ org.bukkit.entity.Entity.class, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause.class }, new Object[]{ p0, p1 }, boolean.class);
    }

    @Override public boolean teleport(org.bukkit.Location p0, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause p1, io.papermc.paper.entity.TeleportFlag[] p2) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "teleport", new Class<?>[]{ org.bukkit.Location.class, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause.class, io.papermc.paper.entity.TeleportFlag[].class }, new Object[]{ p0, p1, p2 }, boolean.class);
    }

    @Override public boolean isDead() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isDead", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.entity.Entity getVehicle() {
        return (org.bukkit.entity.Entity) PlayerCalls.call(this.handle, "getVehicle", new Class<?>[0], new Object[]{  }, org.bukkit.entity.Entity.class);
    }

    @Override public void setGlowing(boolean p0) {
        PlayerCalls.call(this.handle, "setGlowing", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setPose(org.bukkit.entity.Pose p0, boolean p1) {
        PlayerCalls.call(this.handle, "setPose", new Class<?>[]{ org.bukkit.entity.Pose.class, boolean.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setPose(org.bukkit.entity.Pose p0) {
        PlayerCalls.call(this.handle, "setPose", new Class<?>[]{ org.bukkit.entity.Pose.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.Chunk getChunk() {
        return (org.bukkit.Chunk) PlayerCalls.call(this.handle, "getChunk", new Class<?>[0], new Object[]{  }, org.bukkit.Chunk.class);
    }

    @Override public double getHeight() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getHeight", new Class<?>[0], new Object[]{  }, double.class)).doubleValue();
    }

    @Override public boolean isGlowing() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isGlowing", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean spawnAt(org.bukkit.Location p0, org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason p1) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "spawnAt", new Class<?>[]{ org.bukkit.Location.class, org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason.class }, new Object[]{ p0, p1 }, boolean.class);
    }

    @Override public boolean spawnAt(org.bukkit.Location p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "spawnAt", new Class<?>[]{ org.bukkit.Location.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean isInRain() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isInRain", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.inventory.ItemStack getWeapon() {
        return (org.bukkit.inventory.ItemStack) PlayerCalls.call(this.handle, "getWeapon", new Class<?>[0], new Object[]{  }, org.bukkit.inventory.ItemStack.class);
    }

    @Override public boolean isSilent() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isSilent", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void setSilent(boolean p0) {
        PlayerCalls.call(this.handle, "setSilent", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public org.bukkit.World getWorld() {
        return (org.bukkit.World) PlayerCalls.call(this.handle, "getWorld", new Class<?>[0], new Object[]{  }, org.bukkit.World.class);
    }

    @Override public boolean isInWater() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isInWater", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public double getWidth() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getWidth", new Class<?>[0], new Object[]{  }, double.class)).doubleValue();
    }

    @Override public boolean eject() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "eject", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public float getPitch() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getPitch", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public float getYaw() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getYaw", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public boolean collidesAt(org.bukkit.Location p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "collidesAt", new Class<?>[]{ org.bukkit.Location.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public double getGravity() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getGravity", new Class<?>[0], new Object[]{  }, double.class)).doubleValue();
    }

    @Override public org.bukkit.block.BlockFace getFacing() {
        return (org.bukkit.block.BlockFace) PlayerCalls.call(this.handle, "getFacing", new Class<?>[0], new Object[]{  }, org.bukkit.block.BlockFace.class);
    }

    @Override public boolean isInWorld() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isInWorld", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void setGravity(boolean p0) {
        PlayerCalls.call(this.handle, "setGravity", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public float getAirDrag() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getAirDrag", new Class<?>[0], new Object[]{  }, float.class)).floatValue();
    }

    @Override public boolean hasGravity() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasGravity", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean isAlliedTo(org.bukkit.entity.Entity p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isAlliedTo", new Class<?>[]{ org.bukkit.entity.Entity.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean isInLava() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isInLava", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.Location getOrigin() {
        return (org.bukkit.Location) PlayerCalls.call(this.handle, "getOrigin", new Class<?>[0], new Object[]{  }, org.bukkit.Location.class);
    }

    @Override public org.bukkit.entity.Pose getPose() {
        return (org.bukkit.entity.Pose) PlayerCalls.call(this.handle, "getPose", new Class<?>[0], new Object[]{  }, org.bukkit.entity.Pose.class);
    }

    @Override public boolean isTicking() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isTicking", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean isInWaterOrRainOrBubbleColumn() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isInWaterOrRainOrBubbleColumn", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.Sound getSwimHighSpeedSplashSound() {
        return (org.bukkit.Sound) PlayerCalls.call(this.handle, "getSwimHighSpeedSplashSound", new Class<?>[0], new Object[]{  }, org.bukkit.Sound.class);
    }

    @Override public java.util.List getMetadata(java.lang.String p0) {
        return (java.util.List) PlayerCalls.call(this.handle, "getMetadata", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, java.util.List.class);
    }

    @Override public void setMetadata(java.lang.String p0, org.bukkit.metadata.MetadataValue p1) {
        PlayerCalls.call(this.handle, "setMetadata", new Class<?>[]{ java.lang.String.class, org.bukkit.metadata.MetadataValue.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void removeMetadata(java.lang.String p0, org.bukkit.plugin.Plugin p1) {
        PlayerCalls.call(this.handle, "removeMetadata", new Class<?>[]{ java.lang.String.class, org.bukkit.plugin.Plugin.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public boolean hasMetadata(java.lang.String p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasMetadata", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public net.kyori.adventure.text.Component name() {
        return (net.kyori.adventure.text.Component) PlayerCalls.call(this.handle, "name", new Class<?>[0], new Object[]{  }, net.kyori.adventure.text.Component.class);
    }

    @Override public void sendMessage(net.kyori.adventure.text.Component p0) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ net.kyori.adventure.text.Component.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendMessage(net.kyori.adventure.chat.SignedMessage p0, net.kyori.adventure.chat.ChatType.Bound p1) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ net.kyori.adventure.chat.SignedMessage.class, net.kyori.adventure.chat.ChatType.Bound.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendMessage(net.kyori.adventure.text.Component p0, net.kyori.adventure.chat.ChatType.Bound p1) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ net.kyori.adventure.text.Component.class, net.kyori.adventure.chat.ChatType.Bound.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendMessage(java.lang.String[] p0) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ java.lang.String[].class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendMessage(java.lang.String p0) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendMessage(java.util.UUID p0, java.lang.String p1) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ java.util.UUID.class, java.lang.String.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendMessage(java.util.UUID p0, java.lang.String[] p1) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ java.util.UUID.class, java.lang.String[].class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendPlainMessage(java.lang.String p0) {
        PlayerCalls.call(this.handle, "sendPlainMessage", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendRichMessage(java.lang.String p0) {
        PlayerCalls.call(this.handle, "sendRichMessage", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendRichMessage(java.lang.String p0, net.kyori.adventure.text.minimessage.tag.resolver.TagResolver[] p1) {
        PlayerCalls.call(this.handle, "sendRichMessage", new Class<?>[]{ java.lang.String.class, net.kyori.adventure.text.minimessage.tag.resolver.TagResolver[].class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void stopSound(net.kyori.adventure.sound.Sound p0) {
        PlayerCalls.call(this.handle, "stopSound", new Class<?>[]{ net.kyori.adventure.sound.Sound.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void stopSound(net.kyori.adventure.sound.SoundStop p0) {
        PlayerCalls.call(this.handle, "stopSound", new Class<?>[]{ net.kyori.adventure.sound.SoundStop.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void playSound(net.kyori.adventure.sound.Sound p0, double p1, double p2, double p3) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ net.kyori.adventure.sound.Sound.class, double.class, double.class, double.class }, new Object[]{ p0, p1, p2, p3 }, void.class);
    }

    @Override public void playSound(net.kyori.adventure.sound.Sound p0) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ net.kyori.adventure.sound.Sound.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void playSound(net.kyori.adventure.sound.Sound p0, net.kyori.adventure.sound.Sound.Emitter p1) {
        PlayerCalls.call(this.handle, "playSound", new Class<?>[]{ net.kyori.adventure.sound.Sound.class, net.kyori.adventure.sound.Sound.Emitter.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void showTitle(net.kyori.adventure.title.Title p0) {
        PlayerCalls.call(this.handle, "showTitle", new Class<?>[]{ net.kyori.adventure.title.Title.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void openBook(net.kyori.adventure.inventory.Book.Builder p0) {
        PlayerCalls.call(this.handle, "openBook", new Class<?>[]{ net.kyori.adventure.inventory.Book.Builder.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void openBook(net.kyori.adventure.inventory.BookLike p0) {
        PlayerCalls.call(this.handle, "openBook", new Class<?>[]{ net.kyori.adventure.inventory.BookLike.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void openBook(net.kyori.adventure.inventory.Book p0) {
        PlayerCalls.call(this.handle, "openBook", new Class<?>[]{ net.kyori.adventure.inventory.Book.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void removeResourcePacks(net.kyori.adventure.resource.ResourcePackInfoLike p0, net.kyori.adventure.resource.ResourcePackInfoLike[] p1) {
        PlayerCalls.call(this.handle, "removeResourcePacks", new Class<?>[]{ net.kyori.adventure.resource.ResourcePackInfoLike.class, net.kyori.adventure.resource.ResourcePackInfoLike[].class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void removeResourcePacks(java.lang.Iterable p0) {
        PlayerCalls.call(this.handle, "removeResourcePacks", new Class<?>[]{ java.lang.Iterable.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void removeResourcePacks(net.kyori.adventure.resource.ResourcePackRequestLike p0) {
        PlayerCalls.call(this.handle, "removeResourcePacks", new Class<?>[]{ net.kyori.adventure.resource.ResourcePackRequestLike.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void removeResourcePacks(net.kyori.adventure.resource.ResourcePackRequest p0) {
        PlayerCalls.call(this.handle, "removeResourcePacks", new Class<?>[]{ net.kyori.adventure.resource.ResourcePackRequest.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void removeResourcePacks(java.util.UUID p0, java.util.UUID[] p1) {
        PlayerCalls.call(this.handle, "removeResourcePacks", new Class<?>[]{ java.util.UUID.class, java.util.UUID[].class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendActionBar(net.kyori.adventure.text.Component p0) {
        PlayerCalls.call(this.handle, "sendActionBar", new Class<?>[]{ net.kyori.adventure.text.Component.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendActionBar(net.kyori.adventure.text.ComponentLike p0) {
        PlayerCalls.call(this.handle, "sendActionBar", new Class<?>[]{ net.kyori.adventure.text.ComponentLike.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendResourcePacks(net.kyori.adventure.resource.ResourcePackInfoLike p0, net.kyori.adventure.resource.ResourcePackInfoLike[] p1) {
        PlayerCalls.call(this.handle, "sendResourcePacks", new Class<?>[]{ net.kyori.adventure.resource.ResourcePackInfoLike.class, net.kyori.adventure.resource.ResourcePackInfoLike[].class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void sendResourcePacks(net.kyori.adventure.resource.ResourcePackRequest p0) {
        PlayerCalls.call(this.handle, "sendResourcePacks", new Class<?>[]{ net.kyori.adventure.resource.ResourcePackRequest.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendResourcePacks(net.kyori.adventure.resource.ResourcePackRequestLike p0) {
        PlayerCalls.call(this.handle, "sendResourcePacks", new Class<?>[]{ net.kyori.adventure.resource.ResourcePackRequestLike.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendMessage(net.kyori.adventure.text.ComponentLike p0) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ net.kyori.adventure.text.ComponentLike.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendMessage(net.kyori.adventure.text.ComponentLike p0, net.kyori.adventure.chat.ChatType.Bound p1) {
        PlayerCalls.call(this.handle, "sendMessage", new Class<?>[]{ net.kyori.adventure.text.ComponentLike.class, net.kyori.adventure.chat.ChatType.Bound.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void deleteMessage(net.kyori.adventure.chat.SignedMessage.Signature p0) {
        PlayerCalls.call(this.handle, "deleteMessage", new Class<?>[]{ net.kyori.adventure.chat.SignedMessage.Signature.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void deleteMessage(net.kyori.adventure.chat.SignedMessage p0) {
        PlayerCalls.call(this.handle, "deleteMessage", new Class<?>[]{ net.kyori.adventure.chat.SignedMessage.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendPlayerListFooter(net.kyori.adventure.text.Component p0) {
        PlayerCalls.call(this.handle, "sendPlayerListFooter", new Class<?>[]{ net.kyori.adventure.text.Component.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendPlayerListFooter(net.kyori.adventure.text.ComponentLike p0) {
        PlayerCalls.call(this.handle, "sendPlayerListFooter", new Class<?>[]{ net.kyori.adventure.text.ComponentLike.class }, new Object[]{ p0 }, void.class);
    }

    @Override public net.kyori.adventure.audience.Audience filterAudience(java.util.function.Predicate p0) {
        return (net.kyori.adventure.audience.Audience) PlayerCalls.call(this.handle, "filterAudience", new Class<?>[]{ java.util.function.Predicate.class }, new Object[]{ p0 }, net.kyori.adventure.audience.Audience.class);
    }

    @Override public void forEachAudience(java.util.function.Consumer p0) {
        PlayerCalls.call(this.handle, "forEachAudience", new Class<?>[]{ java.util.function.Consumer.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void showBossBar(net.kyori.adventure.bossbar.BossBar p0) {
        PlayerCalls.call(this.handle, "showBossBar", new Class<?>[]{ net.kyori.adventure.bossbar.BossBar.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendPlayerListHeader(net.kyori.adventure.text.ComponentLike p0) {
        PlayerCalls.call(this.handle, "sendPlayerListHeader", new Class<?>[]{ net.kyori.adventure.text.ComponentLike.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendPlayerListHeader(net.kyori.adventure.text.Component p0) {
        PlayerCalls.call(this.handle, "sendPlayerListHeader", new Class<?>[]{ net.kyori.adventure.text.Component.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendTitlePart(net.kyori.adventure.title.TitlePart p0, java.lang.Object p1) {
        PlayerCalls.call(this.handle, "sendTitlePart", new Class<?>[]{ net.kyori.adventure.title.TitlePart.class, java.lang.Object.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void hideBossBar(net.kyori.adventure.bossbar.BossBar p0) {
        PlayerCalls.call(this.handle, "hideBossBar", new Class<?>[]{ net.kyori.adventure.bossbar.BossBar.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void clearResourcePacks() {
        PlayerCalls.call(this.handle, "clearResourcePacks", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void closeDialog() {
        PlayerCalls.call(this.handle, "closeDialog", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void clearTitle() {
        PlayerCalls.call(this.handle, "clearTitle", new Class<?>[0], new Object[]{  }, void.class);
    }

    @Override public void showDialog(net.kyori.adventure.dialog.DialogLike p0) {
        PlayerCalls.call(this.handle, "showDialog", new Class<?>[]{ net.kyori.adventure.dialog.DialogLike.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendPlayerListHeaderAndFooter(net.kyori.adventure.text.Component p0, net.kyori.adventure.text.Component p1) {
        PlayerCalls.call(this.handle, "sendPlayerListHeaderAndFooter", new Class<?>[]{ net.kyori.adventure.text.Component.class, net.kyori.adventure.text.Component.class }, new Object[]{ p0, p1 }, void.class);
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

    @Override public net.kyori.adventure.pointer.Pointers pointers() {
        return (net.kyori.adventure.pointer.Pointers) PlayerCalls.call(this.handle, "pointers", new Class<?>[0], new Object[]{  }, net.kyori.adventure.pointer.Pointers.class);
    }

    @Override public java.lang.Object getOrDefaultFrom(net.kyori.adventure.pointer.Pointer p0, java.util.function.Supplier p1) {
        return (java.lang.Object) PlayerCalls.call(this.handle, "getOrDefaultFrom", new Class<?>[]{ net.kyori.adventure.pointer.Pointer.class, java.util.function.Supplier.class }, new Object[]{ p0, p1 }, java.lang.Object.class);
    }

    @Override public net.kyori.adventure.util.TriState permissionValue(java.lang.String p0) {
        return (net.kyori.adventure.util.TriState) PlayerCalls.call(this.handle, "permissionValue", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, net.kyori.adventure.util.TriState.class);
    }

    @Override public net.kyori.adventure.util.TriState permissionValue(org.bukkit.permissions.Permission p0) {
        return (net.kyori.adventure.util.TriState) PlayerCalls.call(this.handle, "permissionValue", new Class<?>[]{ org.bukkit.permissions.Permission.class }, new Object[]{ p0 }, net.kyori.adventure.util.TriState.class);
    }

    @Override public net.kyori.adventure.text.Component customName() {
        return (net.kyori.adventure.text.Component) PlayerCalls.call(this.handle, "customName", new Class<?>[0], new Object[]{  }, net.kyori.adventure.text.Component.class);
    }

    @Override public void customName(net.kyori.adventure.text.Component p0) {
        PlayerCalls.call(this.handle, "customName", new Class<?>[]{ net.kyori.adventure.text.Component.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void setCustomName(java.lang.String p0) {
        PlayerCalls.call(this.handle, "setCustomName", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public java.lang.String getCustomName() {
        return (java.lang.String) PlayerCalls.call(this.handle, "getCustomName", new Class<?>[0], new Object[]{  }, java.lang.String.class);
    }

    @Override public org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        return (org.bukkit.persistence.PersistentDataContainer) PlayerCalls.call(this.handle, "getPersistentDataContainer", new Class<?>[0], new Object[]{  }, org.bukkit.persistence.PersistentDataContainer.class);
    }

    @Override public net.kyori.adventure.text.event.HoverEvent asHoverEvent() {
        return (net.kyori.adventure.text.event.HoverEvent) PlayerCalls.call(this.handle, "asHoverEvent", new Class<?>[0], new Object[]{  }, net.kyori.adventure.text.event.HoverEvent.class);
    }

    @Override public net.kyori.adventure.sound.Sound.Source soundSource() {
        return (net.kyori.adventure.sound.Sound.Source) PlayerCalls.call(this.handle, "soundSource", new Class<?>[0], new Object[]{  }, net.kyori.adventure.sound.Sound.Source.class);
    }

    @Override public java.lang.Object getData(io.papermc.paper.datacomponent.DataComponentType.Valued p0) {
        return (java.lang.Object) PlayerCalls.call(this.handle, "getData", new Class<?>[]{ io.papermc.paper.datacomponent.DataComponentType.Valued.class }, new Object[]{ p0 }, java.lang.Object.class);
    }

    @Override public boolean hasData(io.papermc.paper.datacomponent.DataComponentType p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasData", new Class<?>[]{ io.papermc.paper.datacomponent.DataComponentType.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public java.lang.Object getDataOrDefault(io.papermc.paper.datacomponent.DataComponentType.Valued p0, java.lang.Object p1) {
        return (java.lang.Object) PlayerCalls.call(this.handle, "getDataOrDefault", new Class<?>[]{ io.papermc.paper.datacomponent.DataComponentType.Valued.class, java.lang.Object.class }, new Object[]{ p0, p1 }, java.lang.Object.class);
    }

    @Override public org.bukkit.entity.Projectile launchProjectile(java.lang.Class p0) {
        return (org.bukkit.entity.Projectile) PlayerCalls.call(this.handle, "launchProjectile", new Class<?>[]{ java.lang.Class.class }, new Object[]{ p0 }, org.bukkit.entity.Projectile.class);
    }

    @Override public org.bukkit.entity.Projectile launchProjectile(java.lang.Class p0, org.bukkit.util.Vector p1) {
        return (org.bukkit.entity.Projectile) PlayerCalls.call(this.handle, "launchProjectile", new Class<?>[]{ java.lang.Class.class, org.bukkit.util.Vector.class }, new Object[]{ p0, p1 }, org.bukkit.entity.Projectile.class);
    }

    @Override public org.bukkit.entity.Projectile launchProjectile(java.lang.Class p0, org.bukkit.util.Vector p1, java.util.function.Consumer p2) {
        return (org.bukkit.entity.Projectile) PlayerCalls.call(this.handle, "launchProjectile", new Class<?>[]{ java.lang.Class.class, org.bukkit.util.Vector.class, java.util.function.Consumer.class }, new Object[]{ p0, p1, p2 }, org.bukkit.entity.Projectile.class);
    }

    @Override public net.kyori.adventure.util.TriState getFrictionState() {
        return (net.kyori.adventure.util.TriState) PlayerCalls.call(this.handle, "getFrictionState", new Class<?>[0], new Object[]{  }, net.kyori.adventure.util.TriState.class);
    }

    @Override public void setFrictionState(net.kyori.adventure.util.TriState p0) {
        PlayerCalls.call(this.handle, "setFrictionState", new Class<?>[]{ net.kyori.adventure.util.TriState.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void sendRawMessage(java.util.UUID p0, java.lang.String p1) {
        PlayerCalls.call(this.handle, "sendRawMessage", new Class<?>[]{ java.util.UUID.class, java.lang.String.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public boolean isConversing() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isConversing", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void abandonConversation(org.bukkit.conversations.Conversation p0) {
        PlayerCalls.call(this.handle, "abandonConversation", new Class<?>[]{ org.bukkit.conversations.Conversation.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void abandonConversation(org.bukkit.conversations.Conversation p0, org.bukkit.conversations.ConversationAbandonedEvent p1) {
        PlayerCalls.call(this.handle, "abandonConversation", new Class<?>[]{ org.bukkit.conversations.Conversation.class, org.bukkit.conversations.ConversationAbandonedEvent.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void acceptConversationInput(java.lang.String p0) {
        PlayerCalls.call(this.handle, "acceptConversationInput", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, void.class);
    }

    @Override public boolean beginConversation(org.bukkit.conversations.Conversation p0) {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "beginConversation", new Class<?>[]{ org.bukkit.conversations.Conversation.class }, new Object[]{ p0 }, boolean.class);
    }

    @Override public boolean isConnected() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isConnected", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public boolean isOnline() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isOnline", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public org.bukkit.entity.Player getPlayer() {
        return (org.bukkit.entity.Player) PlayerCalls.call(this.handle, "getPlayer", new Class<?>[0], new Object[]{  }, org.bukkit.entity.Player.class);
    }

    @Override public org.bukkit.BanEntry ban(java.lang.String p0, java.util.Date p1, java.lang.String p2) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "ban", new Class<?>[]{ java.lang.String.class, java.util.Date.class, java.lang.String.class }, new Object[]{ p0, p1, p2 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry ban(java.lang.String p0, java.time.Instant p1, java.lang.String p2) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "ban", new Class<?>[]{ java.lang.String.class, java.time.Instant.class, java.lang.String.class }, new Object[]{ p0, p1, p2 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry ban(java.lang.String p0, java.time.Duration p1, java.lang.String p2) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "ban", new Class<?>[]{ java.lang.String.class, java.time.Duration.class, java.lang.String.class }, new Object[]{ p0, p1, p2 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banPlayer(java.lang.String p0, java.util.Date p1) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayer", new Class<?>[]{ java.lang.String.class, java.util.Date.class }, new Object[]{ p0, p1 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banPlayer(java.lang.String p0, java.util.Date p1, java.lang.String p2) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayer", new Class<?>[]{ java.lang.String.class, java.util.Date.class, java.lang.String.class }, new Object[]{ p0, p1, p2 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banPlayer(java.lang.String p0) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayer", new Class<?>[]{ java.lang.String.class }, new Object[]{ p0 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banPlayer(java.lang.String p0, java.lang.String p1) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayer", new Class<?>[]{ java.lang.String.class, java.lang.String.class }, new Object[]{ p0, p1 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.BanEntry banPlayer(java.lang.String p0, java.util.Date p1, java.lang.String p2, boolean p3) {
        return (org.bukkit.BanEntry) PlayerCalls.call(this.handle, "banPlayer", new Class<?>[]{ java.lang.String.class, java.util.Date.class, java.lang.String.class, boolean.class }, new Object[]{ p0, p1, p2, p3 }, org.bukkit.BanEntry.class);
    }

    @Override public org.bukkit.Location getRespawnLocation(boolean p0) {
        return (org.bukkit.Location) PlayerCalls.call(this.handle, "getRespawnLocation", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, org.bukkit.Location.class);
    }

    @Override public boolean isBanned() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isBanned", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void setWhitelisted(boolean p0) {
        PlayerCalls.call(this.handle, "setWhitelisted", new Class<?>[]{ boolean.class }, new Object[]{ p0 }, void.class);
    }

    @Override public long getLastLogin() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getLastLogin", new Class<?>[0], new Object[]{  }, long.class)).longValue();
    }

    @Override public int getStatistic(org.bukkit.Statistic p0, org.bukkit.entity.EntityType p1) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getStatistic", new Class<?>[]{ org.bukkit.Statistic.class, org.bukkit.entity.EntityType.class }, new Object[]{ p0, p1 }, int.class)).intValue();
    }

    @Override public int getStatistic(org.bukkit.Statistic p0, org.bukkit.Material p1) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getStatistic", new Class<?>[]{ org.bukkit.Statistic.class, org.bukkit.Material.class }, new Object[]{ p0, p1 }, int.class)).intValue();
    }

    @Override public int getStatistic(org.bukkit.Statistic p0) {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getStatistic", new Class<?>[]{ org.bukkit.Statistic.class }, new Object[]{ p0 }, int.class)).intValue();
    }

    @Override public boolean hasPlayedBefore() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "hasPlayedBefore", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public void decrementStatistic(org.bukkit.Statistic p0, org.bukkit.entity.EntityType p1) {
        PlayerCalls.call(this.handle, "decrementStatistic", new Class<?>[]{ org.bukkit.Statistic.class, org.bukkit.entity.EntityType.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void decrementStatistic(org.bukkit.Statistic p0, org.bukkit.entity.EntityType p1, int p2) {
        PlayerCalls.call(this.handle, "decrementStatistic", new Class<?>[]{ org.bukkit.Statistic.class, org.bukkit.entity.EntityType.class, int.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void decrementStatistic(org.bukkit.Statistic p0, org.bukkit.Material p1) {
        PlayerCalls.call(this.handle, "decrementStatistic", new Class<?>[]{ org.bukkit.Statistic.class, org.bukkit.Material.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void decrementStatistic(org.bukkit.Statistic p0, org.bukkit.Material p1, int p2) {
        PlayerCalls.call(this.handle, "decrementStatistic", new Class<?>[]{ org.bukkit.Statistic.class, org.bukkit.Material.class, int.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void decrementStatistic(org.bukkit.Statistic p0) {
        PlayerCalls.call(this.handle, "decrementStatistic", new Class<?>[]{ org.bukkit.Statistic.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void decrementStatistic(org.bukkit.Statistic p0, int p1) {
        PlayerCalls.call(this.handle, "decrementStatistic", new Class<?>[]{ org.bukkit.Statistic.class, int.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public boolean isWhitelisted() {
        return (java.lang.Boolean) PlayerCalls.call(this.handle, "isWhitelisted", new Class<?>[0], new Object[]{  }, boolean.class);
    }

    @Override public long getFirstPlayed() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getFirstPlayed", new Class<?>[0], new Object[]{  }, long.class)).longValue();
    }

    @Override public void setStatistic(org.bukkit.Statistic p0, org.bukkit.Material p1, int p2) {
        PlayerCalls.call(this.handle, "setStatistic", new Class<?>[]{ org.bukkit.Statistic.class, org.bukkit.Material.class, int.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void setStatistic(org.bukkit.Statistic p0, int p1) {
        PlayerCalls.call(this.handle, "setStatistic", new Class<?>[]{ org.bukkit.Statistic.class, int.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void setStatistic(org.bukkit.Statistic p0, org.bukkit.entity.EntityType p1, int p2) {
        PlayerCalls.call(this.handle, "setStatistic", new Class<?>[]{ org.bukkit.Statistic.class, org.bukkit.entity.EntityType.class, int.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public org.bukkit.Location getBedSpawnLocation() {
        return (org.bukkit.Location) PlayerCalls.call(this.handle, "getBedSpawnLocation", new Class<?>[0], new Object[]{  }, org.bukkit.Location.class);
    }

    @Override public long getLastPlayed() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getLastPlayed", new Class<?>[0], new Object[]{  }, long.class)).longValue();
    }

    @Override public void incrementStatistic(org.bukkit.Statistic p0, org.bukkit.entity.EntityType p1, int p2) {
        PlayerCalls.call(this.handle, "incrementStatistic", new Class<?>[]{ org.bukkit.Statistic.class, org.bukkit.entity.EntityType.class, int.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void incrementStatistic(org.bukkit.Statistic p0, org.bukkit.Material p1, int p2) {
        PlayerCalls.call(this.handle, "incrementStatistic", new Class<?>[]{ org.bukkit.Statistic.class, org.bukkit.Material.class, int.class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public void incrementStatistic(org.bukkit.Statistic p0) {
        PlayerCalls.call(this.handle, "incrementStatistic", new Class<?>[]{ org.bukkit.Statistic.class }, new Object[]{ p0 }, void.class);
    }

    @Override public void incrementStatistic(org.bukkit.Statistic p0, int p1) {
        PlayerCalls.call(this.handle, "incrementStatistic", new Class<?>[]{ org.bukkit.Statistic.class, int.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void incrementStatistic(org.bukkit.Statistic p0, org.bukkit.Material p1) {
        PlayerCalls.call(this.handle, "incrementStatistic", new Class<?>[]{ org.bukkit.Statistic.class, org.bukkit.Material.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public void incrementStatistic(org.bukkit.Statistic p0, org.bukkit.entity.EntityType p1) {
        PlayerCalls.call(this.handle, "incrementStatistic", new Class<?>[]{ org.bukkit.Statistic.class, org.bukkit.entity.EntityType.class }, new Object[]{ p0, p1 }, void.class);
    }

    @Override public long getLastSeen() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getLastSeen", new Class<?>[0], new Object[]{  }, long.class)).longValue();
    }

    @Override public java.util.Map serialize() {
        return (java.util.Map) PlayerCalls.call(this.handle, "serialize", new Class<?>[0], new Object[]{  }, java.util.Map.class);
    }

    @Override public void sendPluginMessage(org.bukkit.plugin.Plugin p0, java.lang.String p1, byte[] p2) {
        PlayerCalls.call(this.handle, "sendPluginMessage", new Class<?>[]{ org.bukkit.plugin.Plugin.class, java.lang.String.class, byte[].class }, new Object[]{ p0, p1, p2 }, void.class);
    }

    @Override public java.util.Set getListeningPluginChannels() {
        return (java.util.Set) PlayerCalls.call(this.handle, "getListeningPluginChannels", new Class<?>[0], new Object[]{  }, java.util.Set.class);
    }

    @Override public int getProtocolVersion() {
        return ((java.lang.Number) PlayerCalls.call(this.handle, "getProtocolVersion", new Class<?>[0], new Object[]{  }, int.class)).intValue();
    }

    @Override public java.net.InetSocketAddress getVirtualHost() {
        return (java.net.InetSocketAddress) PlayerCalls.call(this.handle, "getVirtualHost", new Class<?>[0], new Object[]{  }, java.net.InetSocketAddress.class);
    }
}
