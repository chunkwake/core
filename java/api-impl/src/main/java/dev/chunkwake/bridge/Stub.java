package dev.chunkwake.bridge;

import java.lang.reflect.Array;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.Arrays;
import java.util.Collection;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Реализация огромных интерфейсов Bukkit делегатом: метод интерфейса ищется в
 * делегате по имени и типам параметров; иначе — default-метод интерфейса;
 * иначе — «мягкий» дефолт ({@link Soft}).
 *
 * <p>Мягкий дефолт вместо исключения: плагин, который зовёт нереализованный
 * метод, не падает целиком в {@code onEnable}, а получает пустое значение —
 * {@code false}, {@code 0}, пустой список, заглушку интерфейса. Каждый такой
 * вызов один раз пишется в журнал: список того, что стоит реализовать, виден
 * сразу и без отладки.
 *
 * <p>Строгий режим (исключение, как было раньше) включается переменной
 * окружения {@code CHUNKWAKE_API_STRICT=1} или системным свойством
 * {@code -Dchunkwake.api.strict=true} — он нужен, чтобы падение было видно
 * тестам и CI, где молчаливая заглушка только спрятала бы регрессию.
 */
final class Stub {
    /** Метод делегата по методу интерфейса — отдельно для каждого класса делегата. */
    private static final ClassValue<Map<Method, Optional<Method>>> CACHE = new ClassValue<>() {
        @Override
        protected Map<Method, Optional<Method>> computeValue(Class<?> type) {
            return new ConcurrentHashMap<>();
        }
    };

    /** Строгий режим: нереализованный метод — исключение, а не дефолт. */
    static final boolean STRICT = strict();

    private static boolean strict() {
        String env = System.getenv("CHUNKWAKE_API_STRICT");
        if (env != null) {
            return !env.isEmpty() && !env.equals("0");
        }
        return Boolean.getBoolean("chunkwake.api.strict");
    }

    private Stub() {}

    /** Делегат прокси, созданного {@link #of}; иначе {@code null}. */
    static Object impl(Object proxy) {
        return proxy != null && Proxy.isProxyClass(proxy.getClass())
                && Proxy.getInvocationHandler(proxy) instanceof Handler h ? h.impl : null;
    }

    private record Handler(Object impl, InvocationHandler call) implements InvocationHandler {
        @Override
        public Object invoke(Object proxy, Method m, Object[] args) throws Throwable {
            return call.invoke(proxy, m, args);
        }
    }

    static <T> T of(Class<T> iface, Object impl) {
        return iface.cast(ofAll(new Class<?>[] {iface}, impl));
    }

    /** Прокси сразу нескольких интерфейсов (например {@code BlockData} + {@code Directional}). */
    static Object ofAll(Class<?>[] ifaces, Object impl) {
        Class<?> iface = ifaces[ifaces.length - 1];
        InvocationHandler h = (proxy, m, args) -> {
            Optional<Method> target = CACHE.get(impl.getClass()).computeIfAbsent(m, k -> find(impl.getClass(), k));
            if (target.isPresent()) {
                try {
                    return target.get().invoke(impl, args);
                } catch (InvocationTargetException e) {
                    throw e.getCause();
                }
            }
            switch (m.getName()) {
                case "equals":
                    if (m.getParameterCount() == 1) {
                        return proxy == args[0];
                    }
                    break;
                case "hashCode":
                    if (m.getParameterCount() == 0) {
                        return System.identityHashCode(proxy);
                    }
                    break;
                case "toString":
                    if (m.getParameterCount() == 0) {
                        return impl.toString();
                    }
                    break;
                default:
                    break;
            }
            if (m.isDefault()) {
                return InvocationHandler.invokeDefault(proxy, m, args);
            }
            if (STRICT) {
                throw new UnsupportedOperationException("chunkwake: " + iface.getSimpleName() + "."
                        + m.getName() + Arrays.toString(m.getParameterTypes()) + " is not implemented yet");
            }
            return Soft.value(iface, m);
        };
        return Proxy.newProxyInstance(iface.getClassLoader(), ifaces, new Handler(impl, h));
    }

    private static Optional<Method> find(Class<?> impl, Method m) {
        try {
            Method found = impl.getMethod(m.getName(), m.getParameterTypes());
            found.setAccessible(true);
            return Optional.of(found);
        } catch (NoSuchMethodException e) {
            return Optional.empty();
        }
    }

    /** Значения-заглушки для нереализованных методов. */
    static final class Soft {
        /** Классы, чьи заглушки безопасно возвращать: API для плагинов. */
        private static final List<String> SOFT_PACKAGES = List.of("org.bukkit.", "io.papermc.paper.",
                "com.destroystokyo.paper.", "net.kyori.adventure.", "org.spigotmc.");

        private Soft() {}

        static long misses() {
            return SoftValue.misses();
        }

        /** Значение по типу возврата метода; для чужих типов — {@code null}. */
        static Object value(Class<?> iface, Method m) {
            return value(iface, m, 2);
        }

        /**
         * @param depth сколько ещё уровней заглушек допустимо: интерфейс API,
         *     возвращающий другой интерфейс API, не должен раскручиваться
         *     бесконечно
         */
        static Object value(Class<?> iface, Method m, int depth) {
            SoftValue.note(iface.getSimpleName() + "." + m.getName());
            Class<?> type = m.getReturnType();
            if (type == void.class) {
                return null;
            }
            // Примитивы — строго своего типа: прокси приводит результат к
            // объявленному типу, и `Integer` вместо `Byte` даёт ClassCastException.
            if (type == boolean.class) {
                return false;
            }
            if (type == char.class) {
                return (char) 0;
            }
            if (type == byte.class) {
                return (byte) 0;
            }
            if (type == short.class) {
                return (short) 0;
            }
            if (type == int.class) {
                return 0;
            }
            if (type == long.class) {
                return 0L;
            }
            if (type == float.class) {
                return 0f;
            }
            if (type == double.class) {
                return 0d;
            }
            if (type == Optional.class) {
                return SoftValue.optional();
            }
            if (type == CompletableFuture.class) {
                return SoftValue.future();
            }
            if (type.isArray()) {
                return Array.newInstance(type.getComponentType(), 0);
            }
            if (type == List.class || type == Collection.class || type == Iterable.class) {
                return SoftValue.list();
            }
            if (type == Set.class) {
                return SoftValue.set();
            }
            if (type == Map.class) {
                return SoftValue.map();
            }
            if (type == String.class) {
                return SoftValue.string();
            }
            return softProxy(type, depth);
        }

        /**
         * Заглушка интерфейса вместо {@code null}: плагин, который вызывает
         * {@code world.getGameRules().length}, получит пустой массив, а не NPE.
         * Только для типов API — на своих классах заглушка спрятала бы баг.
         */
        private static Object softProxy(Class<?> type, int depth) {
            if (depth <= 0 || !type.isInterface() || type.getName().equals("java.lang.Runnable")) {
                return null;
            }
            boolean api = SOFT_PACKAGES.stream().anyMatch(p -> type.getName().startsWith(p));
            if (!api) {
                return null;
            }
            return Proxy.newProxyInstance(type.getClassLoader(), new Class<?>[] {type},
                    (proxy, m, args) -> {
                        switch (m.getName()) {
                            case "equals":
                                return m.getParameterCount() == 1 && proxy == args[0];
                            case "hashCode":
                                return m.getParameterCount() == 0 ? System.identityHashCode(proxy) : null;
                            case "toString":
                                return m.getParameterCount() == 0 ? "empty " + type.getSimpleName() : null;
                            default:
                                break;
                        }
                        if (m.isDefault()) {
                            try {
                                return InvocationHandler.invokeDefault(proxy, m, args);
                            } catch (Throwable t) {
                                return value(type, m, depth - 1);
                            }
                        }
                        return value(type, m, depth - 1);
                    });
        }
    }
}
