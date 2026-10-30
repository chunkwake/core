// Выгрузка физики состояний блоков: `reports/shapes.csv`, строка на id
// состояния: `трение,скорость,прыжок,коробки`, где коробки — `getCollisionShape`
// пустого контекста через `|`, каждая `x0 y0 z0 x1 y1 z1` (пусто — без коллизии);
// трение, скорость и прыжок — `getFriction`, `getSpeedFactor`, `getJumpFactor` блока.
//
// Только рефлексия и java.base, как у `ChunkwakeLight`:
//   javac --release 21 -d . ChunkwakeShapes.java

import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

public final class ChunkwakeShapes {
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
        Method shape = base.getMethod("getCollisionShape", getter, blockPos);
        Method getBlock = base.getMethod("getBlock");
        Class<?> block = Class.forName("net.minecraft.world.level.block.Block");
        Method friction = block.getMethod("getFriction");
        Method speed = block.getMethod("getSpeedFactor");
        Method jump = block.getMethod("getJumpFactor");
        Method toAabbs = Class.forName("net.minecraft.world.phys.shapes.VoxelShape").getMethod("toAabbs");
        Class<?> aabb = Class.forName("net.minecraft.world.phys.AABB");
        String[] fields = {"minX", "minY", "minZ", "maxX", "maxY", "maxZ"};
        java.lang.reflect.Field[] f = new java.lang.reflect.Field[6];
        for (int i = 0; i < 6; i++) {
            f[i] = aabb.getField(fields[i]);
        }
        Object empty = Class.forName("net.minecraft.world.level.EmptyBlockGetter").getField("INSTANCE").get(null);
        Object zero = blockPos.getField("ZERO").get(null);

        int n = (Integer) size.invoke(registry);
        StringBuilder out = new StringBuilder(n * 24);
        for (int id = 0; id < n; id++) {
            Object state = byId.invoke(registry, id);
            Object b = getBlock.invoke(state);
            out.append(friction.invoke(b)).append(',')
                    .append(speed.invoke(b)).append(',')
                    .append(jump.invoke(b)).append(',');
            List<?> boxes = (List<?>) toAabbs.invoke(shape.invoke(state, empty, zero));
            for (int i = 0; i < boxes.size(); i++) {
                if (i > 0) {
                    out.append('|');
                }
                for (int k = 0; k < 6; k++) {
                    if (k > 0) {
                        out.append(' ');
                    }
                    out.append(f[k].getDouble(boxes.get(i)));
                }
            }
            out.append('\n');
        }
        Files.writeString(Path.of(args[0]), out);
        System.exit(0);
    }
}
