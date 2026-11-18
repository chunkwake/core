package net.minecraft.core;

/** NMS {@code SectionPos}. */
public class SectionPos extends Vec3i {
    public SectionPos(int x, int y, int z) {
        super(x, y, z);
    }

    public static SectionPos of(int x, int y, int z) {
        return new SectionPos(x, y, z);
    }

    public static SectionPos of(BlockPos pos) {
        return new SectionPos(pos.getX() >> 4, pos.getY() >> 4, pos.getZ() >> 4);
    }

    public static int blockToSectionCoord(int coordinate) {
        return coordinate >> 4;
    }

    public static int sectionRelative(int coordinate) {
        return coordinate & 15;
    }

    public int minBlockX() {
        return this.x << 4;
    }

    public int minBlockY() {
        return this.y << 4;
    }

    public int minBlockZ() {
        return this.z << 4;
    }

    public long asLong() {
        return BlockPos.of(0).asLong();
    }
}
