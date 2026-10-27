package dev.chunkwake.bridge;

import java.lang.reflect.InvocationHandler;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.Arrays;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Реализация огромных интерфейсов Bukkit делегатом: метод интерфейса ищется в
 * делегате по имени и типам параметров; иначе — default-метод интерфейса;
 * иначе {@link UnsupportedOperationException} с именем метода — сразу видно,
 * чего не хватает плагину.
 */
final class Stub {
    private static final Map<Method, Optional<Method>> CACHE = new ConcurrentHashMap<>();

    private Stub() {}

    static <T> T of(Class<T> iface, Object impl) {
        InvocationHandler h = (proxy, m, args) -> {
            Optional<Method> target = CACHE.computeIfAbsent(m, k -> find(impl.getClass(), k));
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
            throw new UnsupportedOperationException("chunkwake: " + iface.getSimpleName() + "."
                    + m.getName() + Arrays.toString(m.getParameterTypes()) + " is not implemented yet");
        };
        return iface.cast(Proxy.newProxyInstance(iface.getClassLoader(), new Class<?>[] {iface}, h));
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
}
