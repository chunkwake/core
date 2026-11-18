package dev.chunkwake.bridge;

import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Делегирование методов настоящего класса игрока
 * (`org.bukkit.craftbukkit.entity.CraftHumanEntity`) в {@link PlayerImpl}.
 *
 * <p>LuckPerms и другие плагины требуют, чтобы объект игрока был обычным
 * классом (с полем {@code perm}), а не динамическим прокси. Поэтому игрок
 * реализован классом, который зовёт методы {@code PlayerImpl} через этот
 * помощник; отсутствующие методы получают значение по умолчанию.
 */
public final class PlayerCalls {
    private static final Map<Method, Optional<Method>> CACHE = new ConcurrentHashMap<>();

    private PlayerCalls() {
    }

    public static Object call(Object impl, String name, Class<?>[] types, Object[] args,
            Class<?> returnType) {
        Method target = find(impl.getClass(), name, types);
        if (target != null) {
            try {
                return target.invoke(impl, args);
            } catch (InvocationTargetException error) {
                throw sneaky(error.getCause());
            } catch (ReflectiveOperationException error) {
                throw new IllegalStateException(error);
            }
        }
        return defaultValue(returnType);
    }

    private static <T extends Throwable> RuntimeException sneaky(Throwable error) throws T {
        throw (T) error;
    }

    private static Method find(Class<?> type, String name, Class<?>[] types) {
        Method[] methods = type.getMethods();
        for (Method m : methods) {
            if (!m.getName().equals(name) || m.getParameterCount() != types.length) {
                continue;
            }
            Class<?>[] params = m.getParameterTypes();
            boolean ok = true;
            for (int i = 0; i < params.length; i++) {
                if (!params[i].isAssignableFrom(types[i]) && !types[i].isAssignableFrom(params[i])) {
                    ok = false;
                    break;
                }
            }
            if (ok) {
                return m;
            }
        }
        return null;
    }

    private static Object defaultValue(Class<?> type) {
        if (type == void.class) {
            return null;
        }
        if (type == boolean.class || type == Boolean.class) {
            return Boolean.FALSE;
        }
        if (type == int.class || type == Integer.class) {
            return 0;
        }
        if (type == long.class || type == Long.class) {
            return 0L;
        }
        if (type == short.class || type == Short.class) {
            return (short) 0;
        }
        if (type == byte.class || type == Byte.class) {
            return (byte) 0;
        }
        if (type == float.class || type == Float.class) {
            return 0.0f;
        }
        if (type == double.class || type == Double.class) {
            return 0.0;
        }
        if (type == char.class || type == Character.class) {
            return '\0';
        }
        if (type == Optional.class) {
            return Optional.empty();
        }
        if (type == List.class) {
            return Collections.emptyList();
        }
        if (type == Set.class) {
            return Collections.emptySet();
        }
        if (type == Map.class) {
            return Collections.emptyMap();
        }
        if (type.isArray()) {
            return java.lang.reflect.Array.newInstance(type.getComponentType(), 0);
        }
        if (type == String.class) {
            return "";
        }
        return null;
    }

    static {
        // ссылка на Arrays, чтобы не потерять импорт при рефакторинге
        Arrays.asList();
    }
}
