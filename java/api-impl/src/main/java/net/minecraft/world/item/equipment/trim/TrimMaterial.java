package net.minecraft.world.item.equipment.trim;

/** NMS {@code TrimMaterial}. */
public class TrimMaterial {
    public static final Object DIRECT_CODEC = null;
    public static final Object DIRECT_STREAM_CODEC = null;
    public static final Object CODEC = null;
    public static final Object STREAM_CODEC = null;

    private final net.minecraft.resources.Identifier paletteId;
    private final net.minecraft.network.chat.Component description;

    public TrimMaterial(net.minecraft.resources.Identifier paletteId,
            net.minecraft.network.chat.Component description) {
        this.paletteId = paletteId;
        this.description = description;
    }

    public TrimMaterial(String assetName, Object description, java.util.Map<?, ?> overrides,
            Object unknown) {
        this(net.minecraft.resources.Identifier.parse(assetName), null);
    }

    public TrimMaterial(String assetName, Object description, float itemModelIndex,
            java.util.Map<?, ?> overrides, Object unknown) {
        this(net.minecraft.resources.Identifier.parse(assetName), null);
    }

    public net.minecraft.resources.Identifier paletteId() {
        return this.paletteId;
    }

    public net.minecraft.network.chat.Component description() {
        return this.description;
    }
}
