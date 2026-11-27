package dev.chunkwake.bridge;

/**
 * Провод {@code Player.sendMap(MapView)}:
 * {@code player <uuid> sendmap <id>}. Rust отвечает полным кадром
 * {@code map_item_data} 128×128 этой карты только этому игроку.
 */
final class MapWire {
    private MapWire() {
    }

    static String command(org.bukkit.map.MapView view) {
        return "sendmap " + view.getId();
    }
}
