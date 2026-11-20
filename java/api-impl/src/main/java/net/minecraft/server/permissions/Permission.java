package net.minecraft.server.permissions;

/**
 * NMS {@code Permission}: проверяемое право.
 *
 * <p>Emotecraft строит {@code new Permission.HasCommandLevel(level)} и
 * спрашивает {@link PermissionSet#hasPermission(Permission)}.
 */
public interface Permission {
    /** Право «уровень команды не ниже заданного». */
    final class HasCommandLevel implements Permission {
        private final PermissionLevel level;

        public HasCommandLevel(PermissionLevel level) {
            this.level = level;
        }

        public PermissionLevel level() {
            return this.level;
        }
    }
}
