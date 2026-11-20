package net.minecraft.server.permissions;

/** NMS {@code PermissionLevel}: уровни прав (Emotecraft). */
public final class PermissionLevel {
    public static final PermissionLevel ALL = new PermissionLevel(0);
    public static final PermissionLevel MODERATORS = new PermissionLevel(1);
    public static final PermissionLevel GAMEMASTERS = new PermissionLevel(2);
    public static final PermissionLevel ADMINS = new PermissionLevel(3);
    public static final PermissionLevel OWNERS = new PermissionLevel(4);

    private final int level;

    private PermissionLevel(int level) {
        this.level = level;
    }

    public int level() {
        return this.level;
    }
}
