// Выгрузка свойств типов сущностей: `reports/entities.csv`.
//
// Строка на тип, в порядке реестра `entity_type`:
//   имя;clientTrackingRange;updateInterval;ширина;высота;категория;fireImmune
// `clientTrackingRange` — в чанках (дальность слежения `ChunkMap.TrackedEntity`
// = значение × 16 блоков), `updateInterval` — тики между рассылками
// (`ServerEntity`), категория — `MobCategory.getName()`.
//
// Только рефлексия и java.base, как у ChunkwakeLight:
//   javac --release 21 -d . ChunkwakeEntities.java

import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;

public final class ChunkwakeEntities {
    public static void main(String[] args) throws Exception {
        Class.forName("net.minecraft.SharedConstants").getMethod("tryDetectVersion").invoke(null);
        Class.forName("net.minecraft.server.Bootstrap").getMethod("bootStrap").invoke(null);
        Object registry = Class.forName("net.minecraft.core.registries.BuiltInRegistries")
                .getField("ENTITY_TYPE").get(null);
        Class<?> type = Class.forName("net.minecraft.world.entity.EntityType");
        Method getKey = registry.getClass().getMethod("getKey", Object.class);
        Method range = type.getMethod("clientTrackingRange");
        Method interval = type.getMethod("updateInterval");
        Method width = type.getMethod("getWidth");
        Method height = type.getMethod("getHeight");
        Method category = type.getMethod("getCategory");
        Method fireImmune = type.getMethod("fireImmune");
        Method categoryName = Class.forName("net.minecraft.world.entity.MobCategory")
                .getMethod("getName");

        StringBuilder out = new StringBuilder();
        for (Object t : (Iterable<?>) registry) {
            out.append(getKey.invoke(registry, t)).append(';')
                    .append(range.invoke(t)).append(';')
                    .append(interval.invoke(t)).append(';')
                    .append(width.invoke(t)).append(';')
                    .append(height.invoke(t)).append(';')
                    .append(categoryName.invoke(category.invoke(t))).append(';')
                    .append(fireImmune.invoke(t)).append('\n');
        }
        Files.writeString(Path.of(args[0]), out);
        System.exit(0);
    }
}
