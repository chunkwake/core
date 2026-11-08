// Выгрузка типов клеток для поиска пути: `reports/path_types.csv`, строка на
// id состояния: `тип,land,air,solid`, где тип — имя `PathType` из
// `WalkNodeEvaluator.getPathTypeFromState` (сам блок, без соседей и без
// `getPathTypeStatic`), `land` — `isAir() || isPathfindable(LAND)`, `air` —
// `isAir()`, `solid` — `isSolid()` (флаги 0/1).
//
// `getPathTypeFromState` читает только состояние в своей клетке, поэтому мир
// подменяется прокси `BlockGetter`, который на любой адрес отдаёт это состояние.
//
// Только рефлексия и java.base, как у `ChunkwakeLight`:
//   javac --release 21 -d . ChunkwakePathTypes.java

import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.nio.file.Files;
import java.nio.file.Path;

public final class ChunkwakePathTypes {
    public static void main(String[] args) throws Exception {
        Class.forName("net.minecraft.SharedConstants").getMethod("tryDetectVersion").invoke(null);
        Class.forName("net.minecraft.server.Bootstrap").getMethod("bootStrap").invoke(null);
        Object registry = Class.forName("net.minecraft.world.level.block.Block")
                .getField("BLOCK_STATE_REGISTRY").get(null);
        Method size = registry.getClass().getMethod("size");
        Method byId = registry.getClass().getMethod("byId", int.class);
        Class<?> base = Class.forName(
                "net.minecraft.world.level.block.state.BlockBehaviour$BlockStateBase");
        Class<?> getter = Class.forName("net.minecraft.world.level.BlockGetter");
        Class<?> blockPos = Class.forName("net.minecraft.core.BlockPos");
        Class<?> computation = Class.forName("net.minecraft.world.level.pathfinder.PathComputationType");
        Object land = computation.getField("LAND").get(null);
        Method isAir = base.getMethod("isAir");
        Method isSolid = base.getMethod("isSolid");
        Method pathfindable = base.getMethod("isPathfindable", computation);
        Method fluid = base.getMethod("getFluidState");
        Method fromState = Class.forName("net.minecraft.world.level.pathfinder.WalkNodeEvaluator")
                .getDeclaredMethod("getPathTypeFromState", getter, blockPos);
        fromState.setAccessible(true);
        Object zero = blockPos.getField("ZERO").get(null);

        int n = (Integer) size.invoke(registry);
        StringBuilder out = new StringBuilder(n * 16);
        for (int id = 0; id < n; id++) {
            Object state = byId.invoke(registry, id);
            InvocationHandler h = (proxy, m, a) -> {
                switch (m.getName()) {
                    case "getBlockState":
                        return state;
                    case "getFluidState":
                        return fluid.invoke(state);
                    default:
                        break;
                }
                if (m.isDefault()) {
                    return InvocationHandler.invokeDefault(proxy, m, a);
                }
                Class<?> r = m.getReturnType();
                if (r == int.class) {
                    return 0;
                }
                if (r == boolean.class) {
                    return false;
                }
                return null;
            };
            Object world = Proxy.newProxyInstance(getter.getClassLoader(), new Class<?>[] {getter}, h);
            boolean air = (boolean) isAir.invoke(state);
            out.append(fromState.invoke(null, world, zero)).append(',')
                    .append(air || (boolean) pathfindable.invoke(state, land) ? 1 : 0).append(',')
                    .append(air ? 1 : 0).append(',')
                    .append((boolean) isSolid.invoke(state) ? 1 : 0).append('\n');
        }
        Files.writeString(Path.of(args[0]), out);
        System.exit(0);
    }
}
