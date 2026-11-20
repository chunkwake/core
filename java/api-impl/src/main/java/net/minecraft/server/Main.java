package net.minecraft.server;

/**
 * NMS {@code net.minecraft.server.Main}.
 *
 * <p>CraftEngine ищет любой из «unobfuscated» entrypoint-классов, чтобы
 * прочитать {@code /version.json} из его classloader'а; наличие класса —
 * часть контракта Paper-сервера.
 */
public final class Main {
    private Main() {
    }

    public static void main(String[] args) {
        throw new UnsupportedOperationException(
                "Chunkwake runs the server from Rust; net.minecraft.server.Main is a marker");
    }
}
