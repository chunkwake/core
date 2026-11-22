package dev.chunkwake.bridge;

import java.util.Collection;
import java.util.List;
import org.bukkit.util.BoundingBox;
import org.bukkit.util.VoxelShape;

/** Bukkit {@link VoxelShape} над списком коробок. */
final class VoxelShapeImpl implements VoxelShape {
    private final List<BoundingBox> boxes;

    private VoxelShapeImpl(List<BoundingBox> boxes) {
        this.boxes = boxes;
    }

    static VoxelShape empty() {
        return new VoxelShapeImpl(List.of());
    }

    static VoxelShape fullCube() {
        return new VoxelShapeImpl(List.of(new BoundingBox(0, 0, 0, 1, 1, 1)));
    }

    static VoxelShape of(List<BoundingBox> boxes) {
        return new VoxelShapeImpl(List.copyOf(boxes));
    }

    @Override
    public Collection<BoundingBox> getBoundingBoxes() {
        return this.boxes;
    }

    @Override
    public boolean overlaps(BoundingBox other) {
        if (other == null) {
            return false;
        }
        for (BoundingBox box : this.boxes) {
            if (box.overlaps(other)) {
                return true;
            }
        }
        return false;
    }

    @Override
    public String toString() {
        return "VoxelShape" + this.boxes;
    }
}
