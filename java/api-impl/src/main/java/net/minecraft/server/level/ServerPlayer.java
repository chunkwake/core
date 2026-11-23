package net.minecraft.server.level;

import net.minecraft.server.network.ServerGamePacketListenerImpl;

/**
 * Минимальный NMS {@code ServerPlayer}: плагины берут его через
 * {@code CraftPlayer.getHandle()} и читают поле {@code connection}.
 */
public class ServerPlayer extends net.minecraft.world.entity.player.Player {
    /**
     * {@code ServerPlayer.sendSystemMessage(Component)}: используется
     * Simple Voice Chat через рефлексию для сообщений в чат.
     */
    public void sendSystemMessage(net.minecraft.network.chat.Component message) {
    }

    public void sendSystemMessage(net.minecraft.network.chat.Component message, boolean overlay) {
    }

    public ServerGamePacketListenerImpl connection;
    /** CraftEngine {@code FieldGetter(name="chunkLoader")}. */
    public Object chunkLoader;
    /** CraftEngine {@code FieldGetter(name="startingToFallPosition")}. */
    public Object startingToFallPosition;
    /** CraftEngine {@code FieldGetter(name="advancements")}. */
    public Object advancements;

    /** {@code ServerPlayer.lookAt(EntityAnchorArgument$Anchor, Vec3)} (CraftEngine). */
    public void lookAt(net.minecraft.commands.arguments.EntityAnchorArgument$Anchor anchor,
            net.minecraft.world.phys.Vec3 position) {
    }

    /** {@code ServerPlayer.lookAt(EntityAnchorArgument$Anchor, Entity, EntityAnchorArgument$Anchor)}. */
    public void lookAt(net.minecraft.commands.arguments.EntityAnchorArgument$Anchor anchor,
            net.minecraft.world.entity.Entity entity,
            net.minecraft.commands.arguments.EntityAnchorArgument$Anchor targetAnchor) {
    }

    public void initMenu(net.minecraft.world.inventory.AbstractContainerMenu menu) {
    }

    public int nextContainerCounter() {
        return 0;
    }

    public void closeContainer() {
    }

    public void closeContainer(org.bukkit.event.inventory.InventoryCloseEvent.Reason reason) {
    }

    public net.minecraft.world.entity.item.ItemEntity drop(net.minecraft.world.item.ItemStack stack,
            boolean dropAround, net.minecraft.util.Prediction prediction, boolean includeThrowerName,
            java.util.function.Consumer<org.bukkit.entity.Item> entityConsumer) {
        return null;
    }

    public net.minecraft.world.entity.item.ItemEntity drop(net.minecraft.world.item.ItemStack stack,
            boolean dropAround, net.minecraft.util.Prediction prediction, boolean includeThrowerName) {
        return null;
    }

    public boolean drop(boolean dropAll) {
        return false;
    }
    /** Поле, которое ищет CraftEngine (ServerPlayerProxy#setGameMode). */
    private net.minecraft.world.level.GameType gameMode = net.minecraft.world.level.GameType.SURVIVAL;

    public ServerPlayer() {
        this.connection = new ServerGamePacketListenerImpl();
    }

    public ServerPlayer(java.util.UUID uuid, String name) {
        super(uuid, name);
        this.connection = new ServerGamePacketListenerImpl();
    }

    /** {@code ServerPlayer.getBukkitEntity()} (CraftEngine). */
    @Override
    public org.bukkit.craftbukkit.entity.CraftLivingEntity getBukkitEntity() {
        return super.getBukkitEntity();
    }

    public String getScoreboardName() {
        return this.getName();
    }

    /** {@code ServerPlayer.getAdvancements()} (CraftEngine). */
    public Object getAdvancements() {
        return this.advancements;
    }

    private final net.minecraft.server.level.ServerLevel level =
            new net.minecraft.server.level.ServerLevel();

    /** {@code ServerPlayer.level()} (Orebfuscator). */
    public net.minecraft.server.level.ServerLevel level() {
        return this.level;
    }

    public net.minecraft.core.RegistryAccess registryAccess() {
        return net.minecraft.core.RegistryAccess.EMPTY_REGISTRY_ACCESS();
    }
}
