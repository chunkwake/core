package net.minecraft.core.component;

/**
 * NMS {@code DataComponentType}.
 *
 * <p>Для Java-слоя достаточно идентичности: CraftEngine получает объект из
 * {@code BuiltInRegistries.DATA_COMPONENT_TYPE} и передаёт его в
 * {@code ItemStack.set/get}. Реальная сериализация компонентов идёт через
 * Rust-ядро, но реестр должен содержать все ключи, которые плагин ищет при
 * инициализации.
 */
public class DataComponentType<T> {
    private final String id;

    public DataComponentType() {
        this(null);
    }

    public DataComponentType(String id) {
        this.id = id;
    }

    public String id() {
        return this.id;
    }

    /**
     * {@code DataComponentType.codecOrThrow()} (CraftEngine).
     *
     * <p>Реальные компоненты сериализуются в Rust; Java-слою достаточно
     * codec-заглушки, которая принимает любое значение. Иначе CE падает
     * на {@code item_model} при сборке custom-предмета из loot.
     */
    @SuppressWarnings("unchecked")
    public com.mojang.serialization.Codec<T> codecOrThrow() {
        return (com.mojang.serialization.Codec<T>) (Object)
                com.mojang.serialization.Codec.PASSTHROUGH;
    }

    @Override
    public String toString() {
        return this.id == null ? "DataComponentType" : this.id;
    }
}
