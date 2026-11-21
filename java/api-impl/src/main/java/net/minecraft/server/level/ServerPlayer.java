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
