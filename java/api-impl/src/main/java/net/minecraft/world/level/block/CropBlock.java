package net.minecraft.world.level.block;

import net.minecraft.world.level.block.state.properties.IntegerProperty;

/** NMS {@code CropBlock}: squaremap читает AGE. */
public class CropBlock extends Block {
    public static final IntegerProperty AGE = IntegerProperty.create("age", 0, 7);

    public CropBlock() {
        super("crop");
    }
}
