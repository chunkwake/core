package net.minecraft.util.datafix.fixes;

import com.mojang.datafixers.DSL;

/** NMS {@code DataConverterTypes}: ссылки типов DFU (nightcore). */
public final class DataConverterTypes {
    public static final DSL.TypeReference ITEM_STACK = () -> "item_stack";
    public static final DSL.TypeReference BLOCK_NAME = () -> "block_name";
    public static final DSL.TypeReference ITEM_NAME = () -> "item_name";
    public static final DSL.TypeReference ENTITY_NAME = () -> "entity_name";

    private DataConverterTypes() {
    }
}
