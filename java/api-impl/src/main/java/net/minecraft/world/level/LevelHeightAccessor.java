package net.minecraft.world.level;

import net.minecraft.core.BlockPos;

/** NMS {@code LevelHeightAccessor}: высоты мира (squaremap и др.). */
public interface LevelHeightAccessor {
    int getHeight();

    int getMinY();

    default int getMaxY() {
        return this.getMinY() + this.getHeight() - 1;
    }

    default int getSectionsCount() {
        return (this.getMaxY() >> 4) - (this.getMinY() >> 4) + 1;
    }

    default int getMinSectionY() {
        return this.getMinY() >> 4;
    }

    default int getMaxSectionY() {
        return this.getMaxY() >> 4;
    }

    default boolean isInsideBuildHeight(BlockPos pos) {
        return this.isInsideBuildHeight(pos.getY());
    }

    default boolean isInsideBuildHeight(int y) {
        return y >= this.getMinY() && y <= this.getMaxY();
    }

    default boolean isOutsideBuildHeight(BlockPos pos) {
        return !this.isInsideBuildHeight(pos);
    }

    default boolean isOutsideBuildHeight(int y) {
        return !this.isInsideBuildHeight(y);
    }

    default int getSectionIndex(int y) {
        return this.getSectionIndexFromSectionY(y >> 4);
    }

    default int getSectionIndexFromSectionY(int sectionY) {
        return sectionY - this.getMinSectionY();
    }

    default int getSectionYFromSectionIndex(int index) {
        return index + this.getMinSectionY();
    }

    static LevelHeightAccessor create(int minY, int height) {
        return new LevelHeightAccessor() {
            @Override
            public int getHeight() {
                return height;
            }

            @Override
            public int getMinY() {
                return minY;
            }
        };
    }
}
