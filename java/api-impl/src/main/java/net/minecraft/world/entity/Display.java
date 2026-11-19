package net.minecraft.world.entity;

import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.network.syncher.EntityDataSerializers;

/**
 * NMS {@code Display} и его варианты.
 *
 * <p>Id метаданных — из реального 26.3: Entity 0..7, Display 8..22,
 * TextDisplay 23..27 (ItemDisplay/BlockDisplay начинаются с 23).
 */
public class Display extends Entity {
    public static final int NO_BRIGHTNESS_OVERRIDE = -1;

    public static final EntityDataAccessor<Integer>
            DATA_TRANSFORMATION_INTERPOLATION_START_DELTA_TICKS_ID = accessor(8,
                    EntityDataSerializers.INT);
    public static final EntityDataAccessor<Integer>
            DATA_TRANSFORMATION_INTERPOLATION_DURATION_ID = accessor(9,
                    EntityDataSerializers.INT);
    public static final EntityDataAccessor<Integer>
            DATA_POS_ROT_INTERPOLATION_DURATION_ID = accessor(10,
                    EntityDataSerializers.INT);
    public static final EntityDataAccessor<Object> DATA_TRANSLATION_ID = accessor(11,
            EntityDataSerializers.VECTOR3);
    public static final EntityDataAccessor<Object> DATA_SCALE_ID = accessor(12,
            EntityDataSerializers.VECTOR3);
    public static final EntityDataAccessor<Object> DATA_LEFT_ROTATION_ID = accessor(13,
            EntityDataSerializers.QUATERNION);
    public static final EntityDataAccessor<Object> DATA_RIGHT_ROTATION_ID = accessor(14,
            EntityDataSerializers.QUATERNION);
    public static final EntityDataAccessor<Byte> DATA_BILLBOARD_RENDER_CONSTRAINTS_ID =
            accessor(15, EntityDataSerializers.BYTE);
    public static final EntityDataAccessor<Integer> DATA_BRIGHTNESS_OVERRIDE_ID = accessor(16,
            EntityDataSerializers.INT);
    public static final EntityDataAccessor<Float> DATA_VIEW_RANGE_ID = accessor(17,
            EntityDataSerializers.FLOAT);
    public static final EntityDataAccessor<Float> DATA_SHADOW_RADIUS_ID = accessor(18,
            EntityDataSerializers.FLOAT);
    public static final EntityDataAccessor<Float> DATA_SHADOW_STRENGTH_ID = accessor(19,
            EntityDataSerializers.FLOAT);
    public static final EntityDataAccessor<Float> DATA_WIDTH_ID = accessor(20,
            EntityDataSerializers.FLOAT);
    public static final EntityDataAccessor<Float> DATA_HEIGHT_ID = accessor(21,
            EntityDataSerializers.FLOAT);
    public static final EntityDataAccessor<Integer> DATA_GLOW_COLOR_OVERRIDE_ID = accessor(22,
            EntityDataSerializers.INT);

    private static <T> EntityDataAccessor<T> accessor(int id,
            net.minecraft.network.syncher.EntityDataSerializer<T> serializer) {
        return new EntityDataAccessor<>(id, serializer);
    }

    /** NMS {@code Display.TextDisplay}. */
    public static class TextDisplay extends Display {
        public static final EntityDataAccessor<net.minecraft.network.chat.Component>
                DATA_TEXT_ID = new EntityDataAccessor<>(23, EntityDataSerializers.COMPONENT);
        public static final EntityDataAccessor<Integer> DATA_LINE_WIDTH_ID =
                new EntityDataAccessor<>(24, EntityDataSerializers.INT);
        public static final EntityDataAccessor<Integer> DATA_BACKGROUND_COLOR_ID =
                new EntityDataAccessor<>(25, EntityDataSerializers.INT);
        public static final EntityDataAccessor<Byte> DATA_TEXT_OPACITY_ID =
                new EntityDataAccessor<>(26, EntityDataSerializers.BYTE);
        public static final EntityDataAccessor<Byte> DATA_STYLE_FLAGS_ID =
                new EntityDataAccessor<>(27, EntityDataSerializers.BYTE);
    }

    /** NMS {@code Display.ItemDisplay}. */
    public static class ItemDisplay extends Display {
        public static final EntityDataAccessor<net.minecraft.world.item.ItemStack>
                DATA_ITEM_STACK_ID = new EntityDataAccessor<>(23,
                        EntityDataSerializers.ITEM_STACK);
        public static final EntityDataAccessor<Byte> DATA_ITEM_DISPLAY_ID =
                new EntityDataAccessor<>(24, EntityDataSerializers.BYTE);
    }

    /** NMS {@code Display.BlockDisplay}. */
    public static class BlockDisplay extends Display {
        public static final EntityDataAccessor<Object> DATA_BLOCK_STATE_ID =
                new EntityDataAccessor<>(23, EntityDataSerializers.BLOCK_STATE);
    }
}
