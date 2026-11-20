package net.minecraft.core;

/** NMS {@code QuartPos}: перевод блоков <-> кварты/секции. */
public final class QuartPos {
    public static final int BITS = 2;
    public static final int SIZE = 4;
    public static final int MASK = 3;

    private static final int SECTION_TO_QUARTS_BITS = 2;

    private QuartPos() {
    }

    public static int fromBlock(int block) {
        return block >> 2;
    }

    public static int quartLocal(int quart) {
        return quart & MASK;
    }

    public static int toBlock(int quart) {
        return quart << 2;
    }

    public static int fromSection(int section) {
        return section << SECTION_TO_QUARTS_BITS;
    }

    public static int toSection(int quart) {
        return quart >> SECTION_TO_QUARTS_BITS;
    }
}
