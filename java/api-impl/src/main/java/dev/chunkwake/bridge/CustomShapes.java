package dev.chunkwake.bridge;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;
import org.bukkit.util.BoundingBox;

/**
 * Коллизии custom-блоков по их visual/carrier-состоянию.
 *
 * <p>CraftEngine строит custom-блок поверх ванильного состояния
 * (`cut_copper_stairs`, `oak_door`, `barrier`, `note_block`...). Bukkit
 * `Block#getCollisionShape` на Paper берёт форму именно этого состояния,
 * поэтому нельзя отдавать полный куб всем custom-блокам. Здесь реализованы
 * формы для типовых носителей; неизвестные типы получают полный куб.
 */
final class CustomShapes {
    private CustomShapes() {
    }

    static List<BoundingBox> collision(BlockState state) {
        if (state == null || state.isAir()) {
            return List.of();
        }
        String id = state.getBlock().id();
        String path = id.contains(":") ? id.substring(id.indexOf(':') + 1) : id;
        Map<String, String> values = values(state);
        if (path.endsWith("_stairs")) {
            return stairs(values);
        }
        if (path.endsWith("_door")) {
            return door(values);
        }
        if (path.endsWith("_slab")) {
            return slab(values);
        }
        if (path.endsWith("_trapdoor")) {
            return trapdoor(values);
        }
        if (path.endsWith("_fence")) {
            return fence(values);
        }
        if (path.endsWith("_wall")) {
            return wall(values);
        }
        if (path.endsWith("_button")) {
            return button(values);
        }
        // barrier, note_block, mushroom_block, ore и прочие полные носители.
        return List.of(new BoundingBox(0, 0, 0, 1, 1, 1));
    }

    private static Map<String, String> values(BlockState state) {
        java.util.LinkedHashMap<String, String> out = new java.util.LinkedHashMap<>();
        for (Map.Entry<Property<?>, Comparable<?>> entry : state.getValues().entrySet()) {
            out.put(entry.getKey().getName(),
                    entry.getValue().toString().toLowerCase(java.util.Locale.ROOT));
        }
        return out;
    }

    private static List<BoundingBox> stairs(Map<String, String> values) {
        String facing = values.getOrDefault("facing", "north");
        String half = values.getOrDefault("half", "bottom");
        String shape = values.getOrDefault("shape", "straight");
        if (!shape.equals("straight")) {
            // inner/outer углы требуют более сложной геометрии; пока полный куб
            // безопаснее, чем пустая коллизия.
            return List.of(new BoundingBox(0, 0, 0, 1, 1, 1));
        }
        // Paper отдаёт две коробки: полную со стороны facing и половинную
        // с противоположной стороны (не полный slab). Для top:
        //   east -> [0.5,0..1,0..1] + [0..0.5,0.5..1,0..1].
        double low = 0.0;
        double high = 1.0;
        double halfLow = "top".equals(half) ? 0.5 : 0.0;
        double halfHigh = "top".equals(half) ? 1.0 : 0.5;
        BoundingBox step = switch (facing) {
            case "east" -> new BoundingBox(0.5, low, 0, 1, high, 1);
            case "west" -> new BoundingBox(0, low, 0, 0.5, high, 1);
            case "south" -> new BoundingBox(0, low, 0.5, 1, high, 1);
            default -> new BoundingBox(0, low, 0, 1, high, 0.5);
        };
        BoundingBox other = switch (facing) {
            case "east" -> new BoundingBox(0, halfLow, 0, 0.5, halfHigh, 1);
            case "west" -> new BoundingBox(0.5, halfLow, 0, 1, halfHigh, 1);
            case "south" -> new BoundingBox(0, halfLow, 0, 1, halfHigh, 0.5);
            default -> new BoundingBox(0, halfLow, 0.5, 1, halfHigh, 1);
        };
        return dedupe(List.of(step, other));
    }

    private static List<BoundingBox> slab(Map<String, String> values) {
        String type = values.getOrDefault("type", "bottom");
        if ("double".equals(type)) {
            return List.of(new BoundingBox(0, 0, 0, 1, 1, 1));
        }
        if ("top".equals(type)) {
            return List.of(new BoundingBox(0, 0.5, 0, 1, 1, 1));
        }
        return List.of(new BoundingBox(0, 0, 0, 1, 0.5, 1));
    }

    private static List<BoundingBox> door(Map<String, String> values) {
        String facing = values.getOrDefault("facing", "north");
        boolean open = "true".equals(values.getOrDefault("open", "false"));
        if (!open) {
            return List.of(new BoundingBox(0, 0, 0, 1, 1, 1));
        }
        return switch (facing) {
            case "east" -> List.of(new BoundingBox(0, 0, 0.8125, 1, 1, 1));
            case "west" -> List.of(new BoundingBox(0, 0, 0, 1, 1, 0.1875));
            case "south" -> List.of(new BoundingBox(0.8125, 0, 0, 1, 1, 1));
            default -> List.of(new BoundingBox(0, 0, 0, 0.1875, 1, 1));
        };
    }

    private static List<BoundingBox> trapdoor(Map<String, String> values) {
        boolean open = "true".equals(values.getOrDefault("open", "false"));
        String half = values.getOrDefault("half", "bottom");
        if (!open) {
            return "top".equals(half)
                    ? List.of(new BoundingBox(0, 0.8125, 0, 1, 1, 1))
                    : List.of(new BoundingBox(0, 0, 0, 1, 0.1875, 1));
        }
        return List.of(new BoundingBox(0, 0, 0.8125, 1, 1, 1));
    }

    /**
     * Коллизия забора: столб 6/16..10/16 высотой 1 блок и перекладины
     * 7/16..9/16 на высоте 6/16..15/16 к подключённым сторонам.
     *
     * <p>Это collision-форма {@code CrossCollisionBlock.makeShapes(4, 16, 2, 6, 15)},
     * а не visual-форма высотой 1.5 блока.
     */
    static List<BoundingBox> fence(Map<String, String> values) {
        double px = 1.0 / 16.0;
        List<BoundingBox> boxes = new ArrayList<>(5);
        boxes.add(new BoundingBox(6 * px, 0, 6 * px, 10 * px, 1, 10 * px));
        if (isTrue(values, "north")) {
            boxes.add(new BoundingBox(7 * px, 6 * px, 0, 9 * px, 15 * px, 8 * px));
        }
        if (isTrue(values, "south")) {
            boxes.add(new BoundingBox(7 * px, 6 * px, 8 * px, 9 * px, 15 * px, 1));
        }
        if (isTrue(values, "west")) {
            boxes.add(new BoundingBox(0, 6 * px, 7 * px, 8 * px, 15 * px, 9 * px));
        }
        if (isTrue(values, "east")) {
            boxes.add(new BoundingBox(8 * px, 6 * px, 7 * px, 1, 15 * px, 9 * px));
        }
        return boxes;
    }

    /**
     * Стена: столб 8x8 (4/16..12/16) высотой 24/16 и плечи 6/16
     * (5/16..11/16) длиной 11/16, как {@code WallBlock.makeShapes(24, 24)}.
     */
    static List<BoundingBox> wall(Map<String, String> values) {
        List<BoundingBox> boxes = new ArrayList<>(5);
        boxes.add(new BoundingBox(4.0 / 16.0, 0, 4.0 / 16.0,
                12.0 / 16.0, 24.0 / 16.0, 12.0 / 16.0));
        if (isConnected(values, "north")) {
            boxes.add(new BoundingBox(5.0 / 16.0, 0, 0,
                    11.0 / 16.0, 24.0 / 16.0, 11.0 / 16.0));
        }
        if (isConnected(values, "east")) {
            boxes.add(new BoundingBox(5.0 / 16.0, 0, 5.0 / 16.0,
                    1, 24.0 / 16.0, 11.0 / 16.0));
        }
        if (isConnected(values, "south")) {
            boxes.add(new BoundingBox(5.0 / 16.0, 0, 5.0 / 16.0,
                    11.0 / 16.0, 24.0 / 16.0, 1));
        }
        if (isConnected(values, "west")) {
            boxes.add(new BoundingBox(0, 0, 5.0 / 16.0,
                    11.0 / 16.0, 24.0 / 16.0, 11.0 / 16.0));
        }
        return boxes;
    }

    /** WallSide {@code low}/{@code tall} и CE-значение {@code true}. */
    private static boolean isConnected(Map<String, String> values, String key) {
        String value = values.get(key);
        return value != null && !"false".equals(value) && !"none".equals(value)
                && !"null".equals(value);
    }

    /**
     * Кнопка: пластина 6/16 x 4/16 x 2/16 у соответствующей грани.
     * В 26.3 состояние задаётся {@code face=floor|wall|ceiling} и
     * {@code facing=north|east|south|west}.
     */
    static List<BoundingBox> button(Map<String, String> values) {
        String face = values.getOrDefault("face", "wall");
        String facing = values.getOrDefault("facing", "north");
        if ("floor".equals(face)) {
            return List.of(new BoundingBox(5.0 / 16.0, 0, 5.0 / 16.0,
                    11.0 / 16.0, 2.0 / 16.0, 11.0 / 16.0));
        }
        if ("ceiling".equals(face)) {
            return List.of(new BoundingBox(5.0 / 16.0, 14.0 / 16.0, 5.0 / 16.0,
                    11.0 / 16.0, 1, 11.0 / 16.0));
        }
        return switch (facing) {
            case "east" -> List.of(new BoundingBox(14.0 / 16.0, 5.0 / 16.0, 5.0 / 16.0,
                    1, 11.0 / 16.0, 11.0 / 16.0));
            case "west" -> List.of(new BoundingBox(0, 5.0 / 16.0, 5.0 / 16.0,
                    2.0 / 16.0, 11.0 / 16.0, 11.0 / 16.0));
            case "south" -> List.of(new BoundingBox(5.0 / 16.0, 5.0 / 16.0, 14.0 / 16.0,
                    11.0 / 16.0, 11.0 / 16.0, 1));
            default -> List.of(new BoundingBox(5.0 / 16.0, 5.0 / 16.0, 0,
                    11.0 / 16.0, 11.0 / 16.0, 2.0 / 16.0));
        };
    }

    private static boolean isTrue(Map<String, String> values, String key) {
        return "true".equals(values.getOrDefault(key, "false"));
    }

    private static List<BoundingBox> dedupe(List<BoundingBox> boxes) {
        List<BoundingBox> out = new ArrayList<>(boxes.size());
        for (BoundingBox box : boxes) {
            boolean duplicate = false;
            for (BoundingBox existing : out) {
                if (existing.getMinX() == box.getMinX() && existing.getMinY() == box.getMinY()
                        && existing.getMinZ() == box.getMinZ()
                        && existing.getMaxX() == box.getMaxX() && existing.getMaxY() == box.getMaxY()
                        && existing.getMaxZ() == box.getMaxZ()) {
                    duplicate = true;
                    break;
                }
            }
            if (!duplicate) {
                out.add(box);
            }
        }
        return out;
    }
}
