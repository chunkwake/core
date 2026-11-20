package net.minecraft.server.permissions;

/**
 * NMS {@code PermissionSet}: набор прав отправителя команды.
 *
 * <p>Уровень оператора у нас проверяется через Bukkit-права, поэтому
 * ванильная проверка уровня команды не должна блокировать команду сама по
 * себе; окончательное решение принимает {@code IPermissionService} по
 * Bukkit-permission.
 */
public interface PermissionSet {
    static PermissionSet all() {
        return permission -> true;
    }

    boolean hasPermission(Permission permission);
}
