package org.bukkit.craftbukkit.inventory;

import java.util.LinkedHashMap;
import java.util.Map;

/**
 * CraftBukkit-представление {@code ItemMeta}.
 *
 * <p>item-nbt-api (встроен в DeluxeHub) читает поле {@code unhandledTags}
 * рефлексией, чтобы сохранять неизвестные NBT-ключи. Класс реальный (не
 * заглушка-интерфейс), потому что плагин делает {@code getDeclaredField}.
 */
public class CraftMetaItem {
    /** Поле, которое item-nbt-api ищет по имени. */
    final Map<String, Object> unhandledTags = new LinkedHashMap<>();

    public CraftMetaItem() {
    }
}
