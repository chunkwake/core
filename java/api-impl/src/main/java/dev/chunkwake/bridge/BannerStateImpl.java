package dev.chunkwake.bridge;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.DyeColor;
import org.bukkit.Material;
import org.bukkit.block.Banner;
import org.bukkit.block.BlockState;
import org.bukkit.block.banner.Pattern;

/** Рабочий {@link Banner} для {@code BlockStateMeta} (InvSee++). */
final class BannerStateImpl {
    private DyeColor baseColor = DyeColor.WHITE;
    private final List<Pattern> patterns = new ArrayList<>();

    public Material getType() { return Material.WHITE_BANNER; }
    public DyeColor getBaseColor() { return this.baseColor; }
    public void setBaseColor(DyeColor color) {
        this.baseColor = color == null ? DyeColor.WHITE : color;
    }
    public List<Pattern> getPatterns() { return List.copyOf(this.patterns); }
    public void setPatterns(List<Pattern> patterns) {
        this.patterns.clear();
        if (patterns != null) {
            this.patterns.addAll(patterns);
        }
    }
    public boolean update() { return true; }
    public boolean update(boolean force, boolean applyPhysics) { return true; }
    public boolean isPlaced() { return true; }

    static Banner api() {
        return Stub.of(Banner.class, new BannerStateImpl());
    }

    static BlockState generic(Material type) {
        return Stub.of(BlockState.class, new GenericState(type));
    }

    private record GenericState(Material type) {
        public Material getType() { return this.type; }
        public boolean update() { return true; }
        public boolean isPlaced() { return true; }
    }
}
