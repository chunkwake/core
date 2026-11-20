package net.minecraft.world.level.block;

import net.minecraft.world.level.block.state.properties.IntegerProperty;

/** NMS {@code StemBlock}: squaremap читает AGE. */
public class StemBlock extends Block {
    public static final IntegerProperty AGE = IntegerProperty.create("age", 0, 7);

    public StemBlock() {
        super("stem");
    }
}
