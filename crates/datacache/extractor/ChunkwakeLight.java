// Выгрузка свойств состояний блоков: `reports/light.csv`, строка на id
// состояния: `излучение,затухание,небо_вниз(0/1),твёрдость,нужен_инструмент(0/1),прочные_грани,твёрдый(0/1)`
// (прочные грани — биты `isFaceSturdy` по `Direction`: низ, верх, север, юг, запад, восток)
// (твёрдость -1 — неразрушимый; инструмент — `requiresCorrectToolForDrops`;
// твёрдый — `BlockState.isSolid`, опора рамок и картин).
//
// Только рефлексия и java.base: классы сервера новее компилятора, поэтому
// компилируется без них и запускается на Java сервера:
//   javac --release 21 -d . ChunkwakeLight.java
// Готовый ChunkwakeLight.class лежит рядом и встраивается в chunkwake-data.

import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;

public final class ChunkwakeLight {
    public static void main(String[] args) throws Exception {
        Class.forName("net.minecraft.SharedConstants").getMethod("tryDetectVersion").invoke(null);
        Class.forName("net.minecraft.server.Bootstrap").getMethod("bootStrap").invoke(null);
        Object registry = Class.forName("net.minecraft.world.level.block.Block")
                .getField("BLOCK_STATE_REGISTRY").get(null);
        Method size = registry.getClass().getMethod("size");
        Method byId = registry.getClass().getMethod("byId", int.class);
        Class<?> base = Class.forName(
                "net.minecraft.world.level.block.state.BlockBehaviour$BlockStateBase");
        Method emission = base.getMethod("getLightEmission");
        Method dampening = base.getMethod("getLightDampening");
        Method skyDown = base.getMethod("propagatesSkylightDown");
        Method requiresTool = base.getMethod("requiresCorrectToolForDrops");
        Method solid = base.getMethod("isSolid");
        java.lang.reflect.Field hardness = base.getDeclaredField("destroySpeed");
        hardness.setAccessible(true);
        Class<?> getter = Class.forName("net.minecraft.world.level.BlockGetter");
        Class<?> blockPos = Class.forName("net.minecraft.core.BlockPos");
        Class<?> direction = Class.forName("net.minecraft.core.Direction");
        Method sturdy = base.getMethod("isFaceSturdy", getter, blockPos, direction);
        Object empty = Class.forName("net.minecraft.world.level.EmptyBlockGetter").getField("INSTANCE").get(null);
        Object zero = blockPos.getField("ZERO").get(null);
        Object[] dirs = direction.getEnumConstants();

        int n = (Integer) size.invoke(registry);
        StringBuilder out = new StringBuilder(n * 8);
        for (int id = 0; id < n; id++) {
            Object state = byId.invoke(registry, id);
            out.append(emission.invoke(state)).append(',')
                    .append(dampening.invoke(state)).append(',')
                    .append((Boolean) skyDown.invoke(state) ? 1 : 0).append(',')
                    .append(hardness.getFloat(state)).append(',')
                    .append((Boolean) requiresTool.invoke(state) ? 1 : 0).append(',');
            int mask = 0;
            for (int d = 0; d < dirs.length; d++) {
                if ((Boolean) sturdy.invoke(state, empty, zero, dirs[d])) {
                    mask |= 1 << d;
                }
            }
            out.append(mask).append(',').append((Boolean) solid.invoke(state) ? 1 : 0).append('\n');
        }
        Files.writeString(Path.of(args[0]), out);
        System.exit(0);
    }
}
