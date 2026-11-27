package dev.chunkwake.bridge;

import java.nio.charset.StandardCharsets;
import java.util.UUID;

/**
 * Провод наборов ресурсов: {@code player <uuid> rpackpush …} /
 * {@code rpackpop …} / {@code rpackpopall}. Rust собирает
 * {@code resource_pack_push}/{@code resource_pack_pop} этому игроку.
 */
final class ResourcePackWire {
    private ResourcePackWire() {
    }

    static String push(UUID pack, String url, byte[] hash, String promptJson, boolean required) {
        return "rpackpush " + pack
                + " " + Hex.encode(url.getBytes(StandardCharsets.UTF_8))
                + " " + (hash == null || hash.length == 0 ? "-" : Hex.encode(hash))
                + " " + (required ? "1" : "0")
                + " " + (promptJson == null
                        ? "-"
                        : Hex.encode(promptJson.getBytes(StandardCharsets.UTF_8)));
    }

    static String pop(UUID pack) {
        return "rpackpop " + pack;
    }

    static String popAll() {
        return "rpackpopall";
    }
}
