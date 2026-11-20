package dev.chunkwake.bridge;

import java.util.HashMap;
import java.util.Map;

/**
 * Таблица id компонентов предметов из Rust (`components`): в пакете
 * компонент кодируется числовым id реестра `data_component_type`, и без
 * этой таблицы Java не может разобрать патч ItemStack (SET_EQUIPMENT).
 */
public final class ComponentIds {
    private static volatile boolean loaded;
    private static volatile Map<String, Integer> byName = Map.of();
    private static volatile Map<Integer, String> byId = Map.of();

    private ComponentIds() {
    }

    public static synchronized void load() {
        if (loaded) {
            return;
        }
        loaded = true;
        Map<String, Integer> names = new HashMap<>();
        Map<Integer, String> ids = new HashMap<>();
        String answer = Bridge.world("components");
        if (answer != null) {
            for (String line : answer.split("\n")) {
                int space = line.lastIndexOf(' ');
                if (space <= 0) {
                    continue;
                }
                try {
                    int id = Integer.parseInt(line.substring(space + 1).trim());
                    String name = line.substring(0, space).trim();
                    names.put(name, id);
                    ids.put(id, name);
                } catch (NumberFormatException ignored) {
                    // пропускаем битую строку
                }
            }
        }
        byName = Map.copyOf(names);
        byId = Map.copyOf(ids);
    }

    public static int id(String name) {
        load();
        Integer id = byName.get(name);
        return id == null ? -1 : id;
    }

    public static String name(int id) {
        load();
        return byId.get(id);
    }

    public static int count() {
        load();
        return byId.size();
    }
}
