package net.minecraft.world.level;

/** NMS {@code DataPackConfig} (CommandAPI). */
public final class DataPackConfig {
    private final java.util.List<String> disabled = java.util.List.of();

    public DataPackConfig() {
    }

    public java.util.List<String> getDisabled() {
        return this.disabled;
    }

    public java.util.List<String> getEnabled() {
        return java.util.List.of();
    }
}
