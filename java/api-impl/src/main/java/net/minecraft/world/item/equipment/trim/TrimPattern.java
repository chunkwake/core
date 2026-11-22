package net.minecraft.world.item.equipment.trim;

/** NMS {@code TrimPattern}. */
public class TrimPattern {
    public static final Object DIRECT_CODEC = null;
    public static final Object DIRECT_STREAM_CODEC = null;
    public static final Object CODEC = null;
    public static final Object STREAM_CODEC = null;

    private final net.minecraft.resources.Identifier assetId;
    private final net.minecraft.network.chat.Component description;
    private final boolean decal;

    public TrimPattern(net.minecraft.resources.Identifier assetId,
            net.minecraft.network.chat.Component description, boolean decal) {
        this.assetId = assetId;
        this.description = description;
        this.decal = decal;
    }

    public TrimPattern(Object assetId, Object description, boolean decal) {
        this(assetId instanceof net.minecraft.resources.Identifier id ? id : null, null, decal);
    }

    public TrimPattern(Object assetId, Object description, Object template, boolean decal) {
        this(assetId, description, decal);
    }

    public TrimPattern(Object assetId, Object description, Object template) {
        this(assetId, description, false);
    }

    public net.minecraft.resources.Identifier assetId() {
        return this.assetId;
    }

    public net.minecraft.network.chat.Component description() {
        return this.description;
    }

    public boolean decal() {
        return this.decal;
    }
}
