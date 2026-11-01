// Выгрузка климатических параметров оверворлда: `reports/biomes.csv`,
// строка на точку `Climate.ParameterPoint`:
//   имя,темп_мин,темп_макс,влаж_мин,влаж_макс,контин_мин,контин_макс,
//   эроз_мин,эроз_макс,глуб_мин,глуб_макс,стран_мин,стран_макс,смещение
// Оси идут в порядке `Climate.TargetPoint`: temperature, humidity, continentalness,
// erosion, depth, weirdness. Числа — `Climate.quantizeCoord` (float * 10000),
// поэтому сравнение целочисленное и без потерь. Имя биома — как в реестре
// (`minecraft:plains`), его ID наш `RegistryData` присваивает сам.
//
// Список в JSON не лежит: `multi_noise_biome_source_parameter_list/overworld.json`
// — это `{"preset": "minecraft:overworld"}`, а сама таблица строится кодом
// (`MultiNoiseBiomeSourceParameterList.Preset.OVERWORLD`).
//
// Второй режим сверяет выбор биома с ванильным:
//   ChunkwakeBiomes decide <строки.txt> <итог.txt>
// где строка — `темп влаж контин эроз глуб стран` как hex-биты f32
// (`Float.floatToRawIntBits`); в итог пишется имя биома на строку.
//
// Только рефлексия и java.base: классы сервера новее компилятора, поэтому
// компилируется без них и запускается на Java сервера:
//   javac --release 21 -d . ChunkwakeBiomes.java
// Готовый ChunkwakeBiomes.class лежит рядом и встраивается в chunkwake-data.

import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.function.Function;

public final class ChunkwakeBiomes {
    private static final String[] AXES = {"temperature", "humidity", "continentalness", "erosion", "depth",
            "weirdness"};

    public static void main(String[] args) throws Exception {
        Class.forName("net.minecraft.SharedConstants").getMethod("tryDetectVersion").invoke(null);
        Class.forName("net.minecraft.server.Bootstrap").getMethod("bootStrap").invoke(null);
        Object list = overworld();
        if (args.length >= 3 && args[0].equals("decide")) {
            decide(list, Path.of(args[1]), Path.of(args[2]));
        } else {
            write(list, Path.of(args[0]));
        }
        System.exit(0);
    }

    /// `Preset.OVERWORLD.provider().apply(key -> key)`.
    private static Object overworld() throws Exception {
        Class<?> presetClass =
                Class.forName("net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterList$Preset");
        Object preset = presetClass.getField("OVERWORLD").get(null);
        Object provider = presetClass.getMethod("provider").invoke(preset);
        Method apply = null;
        for (Method m : provider.getClass().getMethods()) {
            if (m.getName().equals("apply") && m.getParameterCount() == 1) {
                apply = m;
            }
        }
        if (apply == null) {
            throw new IllegalStateException("Preset provider has no apply(Function)");
        }
        apply.setAccessible(true);
        return apply.invoke(provider, (Function<Object, Object>) key -> key);
    }

    private static List<?> values(Object list) throws Exception {
        return (List<?>) list.getClass().getMethod("values").invoke(list);
    }

    private static void write(Object list, Path out) throws Exception {
        Class<?> pointClass = Class.forName("net.minecraft.world.level.biome.Climate$ParameterPoint");
        Class<?> parameterClass = Class.forName("net.minecraft.world.level.biome.Climate$Parameter");
        Class<?> keyClass = Class.forName("net.minecraft.resources.ResourceKey");
        Method first = null;
        Method second = null;
        for (Method m : Class.forName("com.mojang.datafixers.util.Pair").getMethods()) {
            if (m.getName().equals("getFirst")) {
                first = m;
            } else if (m.getName().equals("getSecond")) {
                second = m;
            }
        }
        Method min = parameterClass.getMethod("min");
        Method max = parameterClass.getMethod("max");
        Method identifier = keyClass.getMethod("identifier");
        Method offset = pointClass.getMethod("offset");
        Method[] axis = new Method[AXES.length];
        for (int i = 0; i < AXES.length; i++) {
            axis[i] = pointClass.getMethod(AXES[i]);
        }

        List<?> pairs = values(list);
        StringBuilder text = new StringBuilder(pairs.size() * 96);
        for (Object pair : pairs) {
            Object point = first.invoke(pair);
            Object key = second.invoke(pair);
            Object id = identifier.invoke(key);
            text.append(id.toString());
            for (Method m : axis) {
                Object p = m.invoke(point);
                text.append(',').append(min.invoke(p)).append(',').append(max.invoke(p));
            }
            text.append(',').append(offset.invoke(point)).append('\n');
        }
        Files.writeString(out, text);
    }

    /// `ParameterList.findValue` на готовых числах климата.
    private static void decide(Object list, Path in, Path out) throws Exception {
        Class<?> targetClass = Class.forName("net.minecraft.world.level.biome.Climate$TargetPoint");
        java.lang.reflect.Constructor<?> target = targetClass.getConstructor(
                long.class, long.class, long.class, long.class, long.class, long.class);
        Method find = list.getClass().getMethod("findValue", targetClass);
        Class<?> keyClass = Class.forName("net.minecraft.resources.ResourceKey");
        Method identifier = keyClass.getMethod("identifier");

        StringBuilder text = new StringBuilder();
        for (String line : Files.readAllLines(in)) {
            String[] f = line.trim().split("\\s+");
            if (f.length < 6) {
                continue;
            }
            long[] q = new long[6];
            for (int i = 0; i < 6; i++) {
                float v = Float.intBitsToFloat((int) Long.parseLong(f[i].trim(), 16));
                q[i] = (long) (v * 10000.0f);
            }
            Object point = target.newInstance(q[0], q[1], q[2], q[3], q[4], q[5]);
            Object key = find.invoke(list, point);
            text.append(identifier.invoke(key)).append('\n');
        }
        Files.writeString(out, text);
    }
}
