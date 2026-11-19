package net.minecraft.commands.synchronization;

import java.util.LinkedHashMap;
import java.util.Map;

/**
 * NMS {@code ArgumentTypeInfos}.
 *
 * <p>Incendo Cloud (GrimAC/SkinsRestorer) берёт
 * {@code getDeclaredFields()[0]} и читает из него карту «класс → info»,
 * поэтому {@code BY_CLASS} объявлено первым.
 */
public final class ArgumentTypeInfos {
    private static final Map<Class<?>, Object> BY_CLASS = new LinkedHashMap<>();

    private ArgumentTypeInfos() {
    }

    public static void register(Class<?> type, Object info) {
        BY_CLASS.put(type, info);
    }

    public static Map<Class<?>, Object> byClass() {
        return BY_CLASS;
    }
}
