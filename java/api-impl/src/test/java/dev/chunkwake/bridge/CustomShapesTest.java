package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.util.List;
import java.util.Map;
import org.bukkit.util.BoundingBox;
import org.junit.jupiter.api.Test;

/** Collision-формы custom-заборов должны совпадать с `FenceBlock`. */
class CustomShapesTest {
    @Test
    void fenceWithoutConnectionsIsOnlyThePost() {
        List<BoundingBox> boxes = CustomShapes.fence(Map.of());
        assertEquals(1, boxes.size());
        BoundingBox post = boxes.getFirst();
        assertEquals(0.375, post.getMinX(), 1e-9);
        assertEquals(0.375, post.getMinZ(), 1e-9);
        assertEquals(0.625, post.getMaxX(), 1e-9);
        assertEquals(1.0, post.getMaxY(), 1e-9);
        assertEquals(0.625, post.getMaxZ(), 1e-9);
    }

    @Test
    void fenceAddsOnlyConnectedArms() {
        List<BoundingBox> boxes = CustomShapes.fence(Map.of(
                "north", "true",
                "east", "false",
                "south", "true",
                "west", "false"));
        assertEquals(3, boxes.size());

        BoundingBox north = boxes.get(1);
        assertEquals(0.4375, north.getMinX(), 1e-9);
        assertEquals(0.0, north.getMinZ(), 1e-9);
        assertEquals(0.5625, north.getMaxX(), 1e-9);
        assertEquals(0.5, north.getMaxZ(), 1e-9);
        assertEquals(0.375, north.getMinY(), 1e-9);
        assertEquals(0.9375, north.getMaxY(), 1e-9);

        BoundingBox south = boxes.get(2);
        assertEquals(0.5, south.getMinZ(), 1e-9);
        assertEquals(1.0, south.getMaxZ(), 1e-9);
    }

    @Test
    void wallWithoutConnectionsIsOnlyTheTallPost() {
        List<BoundingBox> boxes = CustomShapes.wall(Map.of());
        assertEquals(1, boxes.size());
        BoundingBox post = boxes.getFirst();
        assertEquals(0.25, post.getMinX(), 1e-9);
        assertEquals(0.25, post.getMinZ(), 1e-9);
        assertEquals(0.75, post.getMaxX(), 1e-9);
        assertEquals(1.5, post.getMaxY(), 1e-9);
        assertEquals(0.75, post.getMaxZ(), 1e-9);
    }

    @Test
    void wallAddsArmsForLowAndTallSides() {
        List<BoundingBox> boxes = CustomShapes.wall(Map.of(
                "north", "tall",
                "east", "low",
                "south", "none",
                "west", "false"));
        assertEquals(3, boxes.size());

        BoundingBox north = boxes.get(1);
        assertEquals(0.3125, north.getMinX(), 1e-9);
        assertEquals(0.0, north.getMinZ(), 1e-9);
        assertEquals(0.6875, north.getMaxX(), 1e-9);
        assertEquals(0.6875, north.getMaxZ(), 1e-9);
        assertEquals(1.5, north.getMaxY(), 1e-9);

        BoundingBox east = boxes.get(2);
        assertEquals(0.3125, east.getMinX(), 1e-9);
        assertEquals(1.0, east.getMaxX(), 1e-9);
        assertEquals(0.3125, east.getMinZ(), 1e-9);
        assertEquals(0.6875, east.getMaxZ(), 1e-9);
    }

    @Test
    void wallMountedButtonUsesThinPlate() {
        List<BoundingBox> boxes = CustomShapes.button(Map.of(
                "face", "wall",
                "facing", "north"));
        assertEquals(1, boxes.size());
        BoundingBox plate = boxes.getFirst();
        assertEquals(0.3125, plate.getMinX(), 1e-9);
        assertEquals(0.3125, plate.getMinY(), 1e-9);
        assertEquals(0.0, plate.getMinZ(), 1e-9);
        assertEquals(0.6875, plate.getMaxX(), 1e-9);
        assertEquals(0.6875, plate.getMaxY(), 1e-9);
        assertEquals(0.125, plate.getMaxZ(), 1e-9);
    }

    @Test
    void floorAndCeilingButtonsUseHorizontalPlates() {
        BoundingBox floor = CustomShapes.button(Map.of("face", "floor")).getFirst();
        assertEquals(0.0, floor.getMinY(), 1e-9);
        assertEquals(0.125, floor.getMaxY(), 1e-9);

        BoundingBox ceiling = CustomShapes.button(Map.of("face", "ceiling")).getFirst();
        assertEquals(0.875, ceiling.getMinY(), 1e-9);
        assertEquals(1.0, ceiling.getMaxY(), 1e-9);
    }

    @Test
    void fenceEastAndWestArmsUseTheRightAxis() {
        List<BoundingBox> boxes = CustomShapes.fence(Map.of(
                "north", "false",
                "east", "true",
                "south", "false",
                "west", "true"));
        assertEquals(3, boxes.size());

        BoundingBox west = boxes.get(1);
        assertEquals(0.0, west.getMinX(), 1e-9);
        assertEquals(0.5, west.getMaxX(), 1e-9);
        assertEquals(0.4375, west.getMinZ(), 1e-9);
        assertEquals(0.5625, west.getMaxZ(), 1e-9);

        BoundingBox east = boxes.get(2);
        assertEquals(0.5, east.getMinX(), 1e-9);
        assertEquals(1.0, east.getMaxX(), 1e-9);
    }
}
