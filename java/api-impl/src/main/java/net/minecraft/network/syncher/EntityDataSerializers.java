package net.minecraft.network.syncher;

/**
 * NMS {@code EntityDataSerializers}.
 *
 * <p>Id взяты из реального реестра 26.3 (порядок регистрации в
 * {@code EntityDataSerializers.<clinit>}): BYTE=0, INT=1, LONG=2, FLOAT=3,
 * STRING=4, COMPONENT=5, OPTIONAL_COMPONENT=6, ITEM_STACK=7, BLOCK_STATE=8,
 * BOOLEAN=10, VECTOR3=39, QUATERNION=40.
 */
public final class EntityDataSerializers {    public static final EntityDataSerializer<Object> ARMADILLO_STATE = ser();
    public static final EntityDataSerializer<Object> HUMANOID_ARM = ser();
    public static final EntityDataSerializer<Object> OPTIONAL_UUID = ser();
    public static final EntityDataSerializer<Object> PARTICLES = ser();
    public static final EntityDataSerializer<Object> WOLF_VARIANT = ser();

    private static EntityDataSerializer<Object> ser() {
        return new EntityDataSerializer<Object>() {
            @Override
            public int id() {
                return 0;
            }
        };
    }

    /** Поле, которое ищет CraftEngine (getPaintingVariant). */
    public static final EntityDataSerializer<Object> particle =
            new EntityDataSerializer<Object>() {
                @Override
                public int id() {
                    return 0;
                }
            };
    public static final EntityDataSerializer<Object> paintingVariant =
            new EntityDataSerializer<Object>() {
                @Override
                public int id() {
                    return 0;
                }
            };

    public static final EntityDataSerializer<Object> PAINTING_VARIANT = paintingVariant;

    // Остальные поля, которые CraftEngine ищет по именам get*.
    public static final EntityDataSerializer<Object> PARTICLE = particle;
    public static final EntityDataSerializer<Object> OPTIONAL_LIVING_ENTITY_REFERENCE =
            serializer(0);
    public static final EntityDataSerializer<Object> ROTATIONS = serializer(0);
    public static final EntityDataSerializer<Object> BLOCK_POS = serializer(0);
    public static final EntityDataSerializer<Object> OPTIONAL_BLOCK_POS = serializer(0);
    public static final EntityDataSerializer<Object> OPTIONAL_BLOCK_STATE = serializer(0);
    public static final EntityDataSerializer<Object> DIRECTION = serializer(0);
    public static final EntityDataSerializer<Object> OPTIONAL_GLOBAL_POS = serializer(0);
    public static final EntityDataSerializer<Object> VILLAGER_DATA = serializer(0);
    public static final EntityDataSerializer<Object> OPTIONAL_UNSIGNED_INT = serializer(0);
    public static final EntityDataSerializer<Object> CAT_VARIANT = serializer(0);
    public static final EntityDataSerializer<Object> FROG_VARIANT = serializer(0);
    public static final EntityDataSerializer<Object> SNIFFER_STATE = serializer(0);
    public static final EntityDataSerializer<Object> COMPOUND_TAG = serializer(0);
    public static final EntityDataSerializer<Object> RESOLVABLE_PROFILE = serializer(0);

    public static final EntityDataSerializer<Byte> BYTE = serializer(0);
    public static final EntityDataSerializer<Integer> INT = serializer(1);
    public static final EntityDataSerializer<Long> LONG = serializer(2);
    public static final EntityDataSerializer<Float> FLOAT = serializer(3);
    public static final EntityDataSerializer<String> STRING = serializer(4);
    public static final EntityDataSerializer<net.minecraft.network.chat.Component> COMPONENT =
            serializer(5);
    public static final EntityDataSerializer<java.util.Optional<net.minecraft.network.chat.Component>>
            OPTIONAL_COMPONENT = serializer(6);
    public static final EntityDataSerializer<net.minecraft.world.item.ItemStack> ITEM_STACK =
            serializer(7);
    public static final EntityDataSerializer<Object> BLOCK_STATE = serializer(8);
    public static final EntityDataSerializer<Boolean> BOOLEAN = serializer(10);
    public static final EntityDataSerializer<Object> POSE = serializer(21);
    public static final EntityDataSerializer<Object> VECTOR3 = serializer(39);
    public static final EntityDataSerializer<Object> QUATERNION = serializer(40);

    private EntityDataSerializers() {
    }

    private static <T> EntityDataSerializer<T> serializer(int id) {
        return () -> id;
    }
}
