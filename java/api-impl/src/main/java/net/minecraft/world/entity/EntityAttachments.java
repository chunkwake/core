package net.minecraft.world.entity;

/** NMS {@code EntityAttachments}: точки привязки модели к хитбоксу. */
public final class EntityAttachments {
    private final float width;
    private final float height;

    private EntityAttachments(float width, float height) {
        this.width = width;
        this.height = height;
    }

    public static EntityAttachments createDefault(float width, float height) {
        return new EntityAttachments(width, height);
    }

    public float width() {
        return this.width;
    }

    public float height() {
        return this.height;
    }
}
