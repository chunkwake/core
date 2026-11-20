package net.minecraft.commands;

/** NMS {@code CommandSourceStack}: отправитель команды (Emotecraft). */
public class CommandSourceStack {
    private net.minecraft.server.level.ServerPlayer player;
    private org.bukkit.command.CommandSender bukkitSender;
    private net.minecraft.world.entity.Entity entity;

    public CommandSourceStack() {
        this(null);
    }

    public CommandSourceStack(net.minecraft.server.level.ServerPlayer player) {
        this.player = player;
        this.entity = player;
    }

    public CommandSourceStack(org.bukkit.command.CommandSender sender,
            net.minecraft.world.entity.Entity entity) {
        this.bukkitSender = sender;
        this.entity = entity;
    }

    /** {@code CommandSourceStack.getBukkitSender()} (CommandAPI). */
    public org.bukkit.command.CommandSender getBukkitSender() {
        return this.bukkitSender;
    }

    /** {@code CommandSourceStack.getEntity()} (CommandAPI). */
    public net.minecraft.world.entity.Entity getEntity() {
        return this.entity;
    }

    /** {@code getPosition()} (CommandAPI). */
    public net.minecraft.world.phys.Vec3 getPosition() {
        return this.entity == null
                ? net.minecraft.world.phys.Vec3.ZERO
                : new net.minecraft.world.phys.Vec3(
                        this.entity.getX(), this.entity.getY(), this.entity.getZ());
    }

    /** {@code getRotation()} (CommandAPI). */
    public net.minecraft.world.phys.Vec2 getRotation() {
        return new net.minecraft.world.phys.Vec2();
    }

    public net.minecraft.server.level.ServerLevel getLevel() {
        if (this.entity instanceof net.minecraft.server.level.ServerPlayer player) {
            return player.level();
        }
        return new net.minecraft.server.level.ServerLevel();
    }

    public CommandSourceStack withSuppressedOutput() {
        return this;
    }

    public CommandSourceStack withMaximumPermission(
            net.minecraft.server.permissions.PermissionSet permissions) {
        return this;
    }

    public CommandSourceStack withPosition(net.minecraft.world.phys.Vec3 position) {
        return this;
    }

    public CommandSourceStack withRotation(net.minecraft.world.phys.Vec2 rotation) {
        return this;
    }

    public CommandSourceStack withLevel(net.minecraft.server.level.ServerLevel level) {
        return this;
    }

    public CommandSourceStack withEntity(net.minecraft.world.entity.Entity entity) {
        return new CommandSourceStack(this.bukkitSender, entity);
    }

    public net.minecraft.world.flag.FeatureFlagSet enabledFeatures() {
        return null;
    }

    public java.util.stream.Stream<?> getAvailableSounds() {
        return java.util.stream.Stream.empty();
    }

    public net.minecraft.server.level.ServerPlayer getPlayerOrException() {
        return this.player;
    }

    public net.minecraft.server.permissions.PermissionSet permissions() {
        return net.minecraft.server.permissions.PermissionSet.all();
    }
}
