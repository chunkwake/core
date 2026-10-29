// Выгрузка свойств состояний блоков для фич декорации: `reports/worldgen_blocks.csv`,
// строка на id состояния: `жидкость,isSolid(0/1),canBeReplaced(0/1),класс`
// (жидкость: 0 — нет, 1 — вода, 2 — лава, в том числе у залитых водой блоков;
// класс — простое имя класса блока, по нему переносится `canSurvive`).
//
// Только рефлексия и java.base, как у ChunkwakeLight:
//   javac --release 21 -d . ChunkwakeWorldgen.java

import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;

public final class ChunkwakeWorldgen {
    public static void main(String[] args) throws Exception {
        Class.forName("net.minecraft.SharedConstants").getMethod("tryDetectVersion").invoke(null);
        Class.forName("net.minecraft.server.Bootstrap").getMethod("bootStrap").invoke(null);
        Object registry = Class.forName("net.minecraft.world.level.block.Block")
                .getField("BLOCK_STATE_REGISTRY").get(null);
        Method size = registry.getClass().getMethod("size");
        Method byId = registry.getClass().getMethod("byId", int.class);
        Class<?> base = Class.forName(
                "net.minecraft.world.level.block.state.BlockBehaviour$BlockStateBase");
        Method fluidState = base.getMethod("getFluidState");
        Method solid = base.getMethod("isSolid");
        Method replaceable = base.getMethod("canBeReplaced");
        Method getBlock = base.getMethod("getBlock");
        Class<?> fluidClass = Class.forName("net.minecraft.world.level.material.FluidState");
        Method isEmpty = fluidClass.getMethod("isEmpty");
        Method getType = fluidClass.getMethod("getType");
        Class<?> fluids = Class.forName("net.minecraft.world.level.material.Fluids");
        Object water = fluids.getField("WATER").get(null);
        Object flowingWater = fluids.getField("FLOWING_WATER").get(null);

        int n = (Integer) size.invoke(registry);
        StringBuilder out = new StringBuilder(n * 16);
        for (int id = 0; id < n; id++) {
            Object state = byId.invoke(registry, id);
            Object fluid = fluidState.invoke(state);
            int f = 0;
            if (!(Boolean) isEmpty.invoke(fluid)) {
                Object type = getType.invoke(fluid);
                f = type == water || type == flowingWater ? 1 : 2;
            }
            out.append(f).append(',')
                    .append((Boolean) solid.invoke(state) ? 1 : 0).append(',')
                    .append((Boolean) replaceable.invoke(state) ? 1 : 0).append(',')
                    .append(getBlock.invoke(state).getClass().getSimpleName()).append('\n');
        }
        Files.writeString(Path.of(args[0]), out);
        System.exit(0);
    }
}
