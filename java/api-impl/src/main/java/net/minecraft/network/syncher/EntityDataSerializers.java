package net.minecraft.network.syncher;

/** NMS {@code EntityDataSerializers} (минимальный набор). */
public final class EntityDataSerializers {
    public static final EntityDataSerializer<Byte> BYTE = new EntityDataSerializer<Byte>() {
    };
    public static final EntityDataSerializer<Integer> INT = new EntityDataSerializer<Integer>() {
    };
    public static final EntityDataSerializer<Float> FLOAT = new EntityDataSerializer<Float>() {
    };
    public static final EntityDataSerializer<Boolean> BOOLEAN = new EntityDataSerializer<Boolean>() {
    };
    public static final EntityDataSerializer<String> STRING = new EntityDataSerializer<String>() {
    };
    public static final EntityDataSerializer<net.minecraft.network.chat.Component> COMPONENT =
            new EntityDataSerializer<net.minecraft.network.chat.Component>() {
            };
    public static final EntityDataSerializer<java.util.Optional<net.minecraft.network.chat.Component>>
            OPTIONAL_COMPONENT = new EntityDataSerializer<
                    java.util.Optional<net.minecraft.network.chat.Component>>() {
            };

    private EntityDataSerializers() {
    }
}
