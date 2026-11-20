package net.minecraft.world.entity.ai.attributes;

import java.util.Collections;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import net.minecraft.world.entity.EntityType;

/** NMS {@code DefaultAttributes}: базовые атрибуты типов сущностей. */
public final class DefaultAttributes {
    private static final Map<EntityType<?>, AttributeSupplier> SUPPLIERS = new ConcurrentHashMap<>();

    private DefaultAttributes() {
    }

    public static AttributeSupplier getSupplier(EntityType<?> type) {
        return type == null ? AttributeSupplier.EMPTY
                : SUPPLIERS.getOrDefault(type, AttributeSupplier.EMPTY);
    }

    public static boolean hasSupplier(EntityType<?> type) {
        return type != null && SUPPLIERS.containsKey(type);
    }

    public static void put(EntityType<?> type, AttributeSupplier supplier) {
        SUPPLIERS.put(type, supplier);
    }

    public static Map<EntityType<?>, AttributeSupplier> getAll() {
        return Collections.unmodifiableMap(SUPPLIERS);
    }
}
