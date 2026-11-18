package net.minecraft.world.level.chunk.status;

/** NMS {@code ChunkStatus}. */
public class ChunkStatus {
    public static final ChunkStatus FULL = new ChunkStatus("full");

    private final String name;

    public ChunkStatus(String name) {
        this.name = name;
    }

    public String getName() {
        return this.name;
    }
}
