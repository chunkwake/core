package net.minecraft.world.level.chunk;

/** Минимальный NMS {@code LevelChunk} (AxiomPaper). */
public class LevelChunk {
    private final int x;
    private final int z;

    public LevelChunk() {
        this(0, 0);
    }

    public LevelChunk(int x, int z) {
        this.x = x;
        this.z = z;
    }

    public int getPosX() { return this.x; }
    public int getPosZ() { return this.z; }

    public void updateBlockEntityTicker(
            net.minecraft.world.level.block.entity.BlockEntity blockEntity) {
    }
}
