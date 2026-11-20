package dev.chunkwake.bridge;

/**
 * Разбор проводного {@code ClientEntityEquipment} (SET_EQUIPMENT):
 * entityId, затем пары «байт слота (0x80 — продолжение) + ItemStack».
 *
 * <p>Компоненты ItemStack в 26.3 — числовые id реестра
 * {@code data_component_type}; таблицу id даёт {@link ComponentIds}.
 * Поддержаны фиксированные компоненты (damage, enchantments, repair_cost,
 * trim, banner_patterns, map_id, map_post_processing, dyed_color); на
 * сложных (custom_name/lore/book/container/charged_projectiles) разбор
 * прекращается и пакет уходит passthrough.
 */
final class EquipmentWire {
    /** Итог разбора: число слотов и признак «всё разобрано». */
    record Parsed(int slots, boolean supported) {
    }

    private EquipmentWire() {
    }

    static Parsed parse(byte[] data, int offset) {
        int[] pos = {offset};
        readVarInt(data, pos);
        int slots = 0;
        while (pos[0] < data.length) {
            int slot = data[pos[0]++] & 0xFF;
            boolean more = (slot & 0x80) != 0;
            slots++;
            int count = readVarInt(data, pos);
            if (count > 0) {
                readVarInt(data, pos); // item id
                int added = readVarInt(data, pos);
                int removed = readVarInt(data, pos);
                if (added < 0 || removed < 0) {
                    return new Parsed(slots, false);
                }
                for (int i = 0; i < added; i++) {
                    int kind = readVarInt(data, pos);
                    if (pos[0] < 0) {
                        return new Parsed(slots, false);
                    }
                    if (!skipComponent(data, pos, kind)) {
                        return new Parsed(slots, false);
                    }
                }
            }
            if (!more) {
                break;
            }
        }
        return new Parsed(slots, true);
    }

    /** Пропускает payload известного фиксированного компонента. */
    private static boolean skipComponent(byte[] data, int[] pos, int kind) {
        if (kind == ComponentIds.id("minecraft:damage")
                || kind == ComponentIds.id("minecraft:repair_cost")
                || kind == ComponentIds.id("minecraft:map_id")
                || kind == ComponentIds.id("minecraft:map_post_processing")) {
            readVarInt(data, pos);
            return pos[0] >= 0;
        }
        if (kind == ComponentIds.id("minecraft:trim")) {
            readVarInt(data, pos);
            readVarInt(data, pos);
            return pos[0] >= 0;
        }
        if (kind == ComponentIds.id("minecraft:dyed_color")) {
            if (pos[0] + 4 > data.length) {
                return false;
            }
            pos[0] += 4;
            return true;
        }
        if (kind == ComponentIds.id("minecraft:enchantments")
                || kind == ComponentIds.id("minecraft:stored_enchantments")) {
            int n = readVarInt(data, pos);
            if (n < 0 || n > 4096) {
                return false;
            }
            for (int i = 0; i < n; i++) {
                readVarInt(data, pos);
                readVarInt(data, pos);
                if (pos[0] < 0) {
                    return false;
                }
            }
            return true;
        }
        if (kind == ComponentIds.id("minecraft:banner_patterns")) {
            int n = readVarInt(data, pos);
            if (n < 0 || n > 256) {
                return false;
            }
            for (int i = 0; i < n; i++) {
                readVarInt(data, pos);
                readVarInt(data, pos);
                if (pos[0] < 0) {
                    return false;
                }
            }
            return true;
        }
        // custom_name/lore/book/container/charged_projectiles/... — passthrough
        return false;
    }

    private static int readVarInt(byte[] data, int[] pos) {
        int value = 0;
        int shift = 0;
        while (true) {
            if (pos[0] >= data.length) {
                pos[0] = -1;
                return -1;
            }
            byte b = data[pos[0]++];
            value |= (b & 0x7F) << shift;
            if ((b & 0x80) == 0) {
                return value;
            }
            shift += 7;
            if (shift > 35) {
                pos[0] = -1;
                return -1;
            }
        }
    }
}
