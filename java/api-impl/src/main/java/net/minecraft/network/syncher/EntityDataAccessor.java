package net.minecraft.network.syncher;

/** NMS {@code EntityDataAccessor} (ImageFrame/ProtocolLib). */
public class EntityDataAccessor<T> {
    private final int id;
    private final EntityDataSerializer<T> serializer;

    public EntityDataAccessor(int id, EntityDataSerializer<T> serializer) {
        this.id = id;
        this.serializer = serializer;
    }

    public int getId() {
        return this.id;
    }

    public EntityDataSerializer<T> getSerializer() {
        return this.serializer;
    }

    @Override
    public String toString() {
        return "EntityDataAccessor[" + this.id + "]";
    }
}
