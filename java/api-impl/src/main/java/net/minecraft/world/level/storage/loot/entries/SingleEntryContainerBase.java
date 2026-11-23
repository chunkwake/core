package net.minecraft.world.level.storage.loot.entries;

import com.mojang.datafixers.Products;
import com.mojang.serialization.Codec;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import java.util.Optional;

/**
 * NMS {@code SingleEntryContainerBase}: база loot-entry, которую наследует
 * CraftEngine's {@code CraftEngineItem}. Конструктор повторяет дескриптор
 * Paper `(IILjava/util/Optional;Ljava/util/Optional;)V`.
 */
public class SingleEntryContainerBase {
    public SingleEntryContainerBase(int weight, int quality,
            Optional<?> conditions, Optional<?> functions) {
    }

    /**
     * {@code UniformContainerBase.uniformFields}: CE's {@code CraftEngineItem}
     * строит через него свой CODEC. Поля нужны только для формы codec;
     * Chunkwake не сериализует loot обратно.
     */
    @SuppressWarnings({"rawtypes", "unchecked"})
    public static <T extends SingleEntryContainerBase> Products.P4 uniformFields(
            RecordCodecBuilder.Instance<T> instance) {
        RecordCodecBuilder weight =
                Codec.INT.optionalFieldOf("weight", 1).forGetter(value -> 1);
        RecordCodecBuilder quality =
                Codec.INT.optionalFieldOf("quality", 0).forGetter(value -> 0);
        RecordCodecBuilder conditions =
                Codec.INT.optionalFieldOf("conditions").forGetter(value -> Optional.empty());
        RecordCodecBuilder functions =
                Codec.INT.optionalFieldOf("functions").forGetter(value -> Optional.empty());
        return instance.group(weight, quality).and(conditions).and(functions);
    }
}
