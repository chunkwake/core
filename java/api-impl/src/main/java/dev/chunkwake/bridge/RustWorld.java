package dev.chunkwake.bridge;

/** Публичный доступ к Rust-миру для NMS-стабов (ServerLevel/LevelChunk). */
public final class RustWorld {
    private RustWorld() {
    }

    public static String request(String line) {
        return Bridge.world(line);
    }
}
