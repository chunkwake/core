package net.minecraft.world.level.chunk.status;

/** NMS {@code ChunkStatus}. */
public class ChunkStatus {
    public static final ChunkStatus EMPTY = new ChunkStatus("empty");
    public static final ChunkStatus SPAWN = new ChunkStatus("spawn");
    public static final ChunkStatus FULL = new ChunkStatus("full");

    public static final com.mojang.serialization.Codec<ChunkStatus> CODEC =
            com.mojang.serialization.Codec.STRING.xmap(name -> FULL, ChunkStatus::getName);

    private final String name;

    public ChunkStatus(String name) {
        this.name = name;
    }

    public boolean isOrAfter(ChunkStatus other) {
        return true;
    }

    public String getName() {
        return this.name;
    }
}
