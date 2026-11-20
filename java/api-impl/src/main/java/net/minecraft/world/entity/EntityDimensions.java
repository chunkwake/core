package net.minecraft.world.entity;

/** NMS {@code EntityDimensions}: размеры хитбокса сущности. */
public class EntityDimensions {
    public final float width;
    public final float height;
    public final float eyeHeight;
    public final EntityAttachments attachments;
    public final boolean fixed;

    public EntityDimensions(float width, float height, float eyeHeight,
            EntityAttachments attachments, boolean fixed) {
        this.width = width;
        this.height = height;
        this.eyeHeight = eyeHeight;
        this.attachments = attachments;
        this.fixed = fixed;
    }

    public static EntityDimensions scalable(float width, float height) {
        return new EntityDimensions(width, height, height * 0.85F,
                EntityAttachments.createDefault(width, height), false);
    }

    public static EntityDimensions fixed(float width, float height) {
        return new EntityDimensions(width, height, height * 0.85F,
                EntityAttachments.createDefault(width, height), true);
    }

    public float width() {
        return this.width;
    }

    public float height() {
        return this.height;
    }

    public float eyeHeight() {
        return this.eyeHeight;
    }
}
