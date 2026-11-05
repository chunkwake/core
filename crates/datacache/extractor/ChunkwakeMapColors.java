// Выгрузка цвета блока на карте: `reports/map_colors.csv`.
//
// `BlockStateBase.getMapColor(EmptyBlockGetter, BlockPos.ZERO).id` — номер
// `MapColor` (0 — `NONE`, такой блок карта пропускает вниз). Цвет обычно один
// на блок, но не всегда: изголовье и изножье кровати, торцы брёвен по оси,
// растущие стебли. Поэтому строк два вида:
//   `имя;цвет` — у всех состояний блока один цвет;
//   `имя[k=v,...];цвет` — иначе, на каждое состояние, свойства по ключу.
//
// Ключ — имя и свойства, а не id: порядок id в реестре состояний не
// совпадает с `blocks.json`.
//
// Только рефлексия и java.base, как у ChunkwakeLight:
//   javac --release 21 -d . ChunkwakeMapColors.java

import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

public final class ChunkwakeMapColors {
    public static void main(String[] args) throws Exception {
        Class.forName("net.minecraft.SharedConstants").getMethod("tryDetectVersion").invoke(null);
        Class.forName("net.minecraft.server.Bootstrap").getMethod("bootStrap").invoke(null);
        Object states = Class.forName("net.minecraft.world.level.block.Block")
                .getField("BLOCK_STATE_REGISTRY").get(null);
        Method size = states.getClass().getMethod("size");
        Method byId = states.getClass().getMethod("byId", int.class);
        Class<?> base = Class.forName(
                "net.minecraft.world.level.block.state.BlockBehaviour$BlockStateBase");
        Class<?> getter = Class.forName("net.minecraft.world.level.BlockGetter");
        Class<?> blockPos = Class.forName("net.minecraft.core.BlockPos");
        Method mapColor = base.getMethod("getMapColor", getter, blockPos);
        Method getBlock = base.getMethod("getBlock");
        Class<?> holder = Class.forName("net.minecraft.world.level.block.state.StateHolder");
        Method getProperties = holder.getMethod("getProperties");
        Class<?> property = Class.forName(
                "net.minecraft.world.level.block.state.properties.Property");
        Method getValue = holder.getMethod("getValue", property);
        Method propName = property.getMethod("getName");
        Method valueName = property.getMethod("getName", Comparable.class);
        java.lang.reflect.Field colorId = Class.forName(
                "net.minecraft.world.level.material.MapColor").getField("id");
        Object empty = Class.forName("net.minecraft.world.level.EmptyBlockGetter")
                .getField("INSTANCE").get(null);
        Object zero = blockPos.getField("ZERO").get(null);
        Object registry = Class.forName("net.minecraft.core.registries.BuiltInRegistries")
                .getField("BLOCK").get(null);
        Method getKey = registry.getClass().getMethod("getKey", Object.class);

        // имя блока → (ключ состояния → цвет), в порядке реестра
        Map<String, Map<String, Integer>> byBlock = new LinkedHashMap<>();
        int n = (Integer) size.invoke(states);
        for (int id = 0; id < n; id++) {
            Object state = byId.invoke(states, id);
            int color = colorId.getInt(mapColor.invoke(state, empty, zero));
            String name = String.valueOf(getKey.invoke(registry, getBlock.invoke(state)));
            TreeMap<String, String> props = new TreeMap<>();
            for (Object p : (Collection<?>) getProperties.invoke(state)) {
                props.put((String) propName.invoke(p),
                        (String) valueName.invoke(p, getValue.invoke(state, p)));
            }
            List<String> parts = new ArrayList<>();
            for (Map.Entry<String, String> e : props.entrySet()) {
                parts.add(e.getKey() + "=" + e.getValue());
            }
            byBlock.computeIfAbsent(name, k -> new LinkedHashMap<>())
                    .put(name + "[" + String.join(",", parts) + "]", color);
        }

        StringBuilder out = new StringBuilder();
        for (Map.Entry<String, Map<String, Integer>> b : byBlock.entrySet()) {
            Collection<Integer> colors = b.getValue().values();
            int first = colors.iterator().next();
            if (colors.stream().allMatch(c -> c == first)) {
                out.append(b.getKey()).append(';').append(first).append('\n');
            } else {
                for (Map.Entry<String, Integer> s : b.getValue().entrySet()) {
                    out.append(s.getKey()).append(';').append(s.getValue()).append('\n');
                }
            }
        }
        Files.writeString(Path.of(args[0]), out);
        System.exit(0);
    }
}
