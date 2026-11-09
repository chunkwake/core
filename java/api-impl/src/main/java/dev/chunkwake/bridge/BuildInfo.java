package dev.chunkwake.bridge;

import io.papermc.paper.ServerBuildInfo;
import java.time.Instant;
import java.util.Optional;
import java.util.OptionalInt;
import net.kyori.adventure.key.Key;

/** {@link ServerBuildInfo} (ServiceLoader): нужен {@code Bukkit.setServer}. */
public final class BuildInfo implements ServerBuildInfo {
    private static final Key BRAND = Key.key("chunkwake", "chunkwake");
    private static final Instant BUILT = Instant.now();

    @Override public Key brandId() { return BRAND; }
    // Плагины, проверяющие «это Paper?», должны работать.
    @Override public boolean isBrandCompatible(Key k) { return k.equals(BRAND) || k.equals(BRAND_PAPER_ID); }
    @Override public String brandName() { return "Chunkwake"; }
    @Override public String minecraftVersionId() { return ChunkwakeServer.VERSION; }
    @Override public String minecraftVersionName() { return ChunkwakeServer.VERSION; }
    @Override public OptionalInt buildNumber() { return OptionalInt.empty(); }
    @Override public Instant buildTime() { return BUILT; }
    @Override public Optional<String> gitBranch() { return Optional.empty(); }
    @Override public Optional<String> gitCommit() { return Optional.empty(); }

    @Override
    public String asString(StringRepresentation r) {
        return r == StringRepresentation.VERSION_SIMPLE ? ChunkwakeServer.VERSION
                : ChunkwakeServer.VERSION + " (MC: " + ChunkwakeServer.VERSION + ")";
    }
}
