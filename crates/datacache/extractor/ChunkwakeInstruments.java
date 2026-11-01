// Выгрузка инструмента нотного блока: `reports/instruments.csv`.
//
// Строка на блок: `<имя блока>,<инструмент>`. Инструмент задаётся в
// `BlockBehaviour.Properties` блока (`instrument(NoteBlockInstrument…)`), а не
// состоянием, поэтому и строка одна на блок — состояния одного блока не
// различаются. Блоки с `harp` (по умолчанию) не пишутся: их большинство.
//
// Имя блока берётся из реестра (`BuiltInRegistries.BLOCK.getKey`), а не id:
// порядок id в реестре состояний не совпадает с порядком в `blocks.json`,
// а имя совпадает всегда.
//
// Только рефлексия и java.base, как у ChunkwakeLight:
//   javac --release 21 -d . ChunkwakeInstruments.java

import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.Map;

public final class ChunkwakeInstruments {
    public static void main(String[] args) throws Exception {
        Class.forName("net.minecraft.SharedConstants").getMethod("tryDetectVersion").invoke(null);
        Class.forName("net.minecraft.server.Bootstrap").getMethod("bootStrap").invoke(null);
        Object states = Class.forName("net.minecraft.world.level.block.Block")
                .getField("BLOCK_STATE_REGISTRY").get(null);
        Method size = states.getClass().getMethod("size");
        Method byId = states.getClass().getMethod("byId", int.class);
        Class<?> base = Class.forName(
                "net.minecraft.world.level.block.state.BlockBehaviour$BlockStateBase");
        Method instrument = base.getMethod("instrument");
        Method getBlock = base.getMethod("getBlock");

        // Имя блока: `BuiltInRegistries.BLOCK.getKey(block)`.
        Object registry = Class.forName("net.minecraft.core.registries.BuiltInRegistries")
                .getField("BLOCK").get(null);
        Method getKey = registry.getClass().getMethod("getKey", Object.class);

        Map<String, String> byBlock = new LinkedHashMap<>();
        int n = (Integer) size.invoke(states);
        for (int id = 0; id < n; id++) {
            Object state = byId.invoke(states, id);
            Object value = instrument.invoke(state);
            String kind = (String) value.getClass().getMethod("getSerializedName").invoke(value);
            Object block = getBlock.invoke(state);
            String name = String.valueOf(getKey.invoke(registry, block));
            String previous = byBlock.putIfAbsent(name, kind);
            if (previous != null && !previous.equals(kind)) {
                throw new IllegalStateException("у блока " + name + " разные инструменты");
            }
        }

        StringBuilder out = new StringBuilder();
        for (Map.Entry<String, String> e : byBlock.entrySet()) {
            if ("harp".equals(e.getValue())) {
                continue;
            }
            out.append(e.getKey()).append(',').append(e.getValue()).append('\n');
        }
        Files.writeString(Path.of(args[0]), out);
        System.exit(0);
    }
}
