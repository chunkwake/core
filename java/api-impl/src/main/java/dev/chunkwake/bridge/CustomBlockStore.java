package dev.chunkwake.bridge;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import net.minecraft.core.BlockPos;

/**
 * Persistence for CraftEngine custom block placements.
 *
 * <p>CraftEngine пишет custom-состояния в Java-оверлей NMS-мира; Rust-ядро
 * о них не знает. Чтобы блоки переживали рестарт (как на Paper), храним
 * координаты и id block definition в простом файле в каталоге мира и
 * восстанавливаем состояние через {@code CraftEngineBlocks.place} при первом
 * обращении.
 */
final class CustomBlockStore {
    private static final Map<String, Map<BlockPos, String>> BLOCKS = new ConcurrentHashMap<>();
    private static final Map<String, Path> FILES = new ConcurrentHashMap<>();
    private static final java.util.Set<Path> LOADED = ConcurrentHashMap.newKeySet();
    private static final ThreadLocal<Boolean> RESTORING = ThreadLocal.withInitial(() -> false);

    private CustomBlockStore() {
    }

    static synchronized void bind(String dimension, Path file) {
        FILES.put(dimension, file);
        if (LOADED.add(file)) {
            load(file);
        }
    }

    static String get(String dimension, BlockPos pos) {
        Map<BlockPos, String> map = BLOCKS.get(dimension);
        return map == null ? null : map.get(pos);
    }

    static boolean restoring() {
        return RESTORING.get();
    }

    static void restoring(boolean value) {
        RESTORING.set(value);
    }

    static synchronized void record(String dimension, BlockPos pos, String id) {
        if (dimension == null || pos == null || id == null || id.isEmpty()) {
            return;
        }
        Map<BlockPos, String> map = BLOCKS.computeIfAbsent(dimension,
                ignored -> new ConcurrentHashMap<>());
        String previous = map.put(pos, id);
        if (!id.equals(previous)) {
            save(dimension);
        }
    }

    static synchronized void remove(String dimension, BlockPos pos) {
        Map<BlockPos, String> map = BLOCKS.get(dimension);
        if (map != null && map.remove(pos) != null) {
            save(dimension);
        }
    }

    private static void load(Path file) {
        if (file == null || !Files.isRegularFile(file)) {
            return;
        }
        try {
            for (String line : Files.readAllLines(file, StandardCharsets.UTF_8)) {
                String[] parts = line.trim().split(" ");
                if (parts.length != 5) {
                    continue;
                }
                try {
                    BLOCKS.computeIfAbsent(parts[0], ignored -> new ConcurrentHashMap<>())
                            .put(new BlockPos(Integer.parseInt(parts[1]),
                                    Integer.parseInt(parts[2]), Integer.parseInt(parts[3])), parts[4]);
                    FILES.putIfAbsent(parts[0], file);
                } catch (NumberFormatException ignored) {
                    // битая строка
                }
            }
        } catch (IOException ignored) {
            // файла ещё нет
        }
    }

    private static void save(String dimension) {
        Path file = FILES.get(dimension);
        if (file == null) {
            return;
        }
        Map<BlockPos, String> map = BLOCKS.get(dimension);
        StringBuilder out = new StringBuilder();
        if (map != null) {
            map.forEach((pos, id) -> out.append(dimension).append(' ')
                    .append(pos.getX()).append(' ').append(pos.getY()).append(' ')
                    .append(pos.getZ()).append(' ').append(id).append('\n'));
        }
        try {
            Path parent = file.getParent();
            if (parent != null) {
                Files.createDirectories(parent);
            }
            Files.writeString(file, out.toString(), StandardCharsets.UTF_8);
        } catch (IOException ignored) {
            // не роняем сервер из-за диска
        }
    }
}
