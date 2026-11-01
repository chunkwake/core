package dev.chunkwake.bridge;

import java.lang.reflect.Array;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Пустые значения для нереализованных методов API.
 *
 * <p>Нужны двум местам: прокси {@link Stub} (когда метода нет в делегате) и
 * сгенерированным провайдерам paper-api (см. {@code GenProviders}), которые
 * лежат в чужих пакетах и потому зовут этот класс напрямую.
 *
 * <p>Каждый пропущенный метод один раз попадает в журнал: список того, что
 * стоит реализовать, виден без отладки.
 */
public final class SoftValue {

    /** Сколько раз звали незаполненные методы за всё время работы. */
    private static final AtomicLong MISSES = new AtomicLong();

    /** Уже предупреждали про этот метод: в журнал — по одному разу. */
    private static final Set<String> WARNED = ConcurrentHashMap.newKeySet();

    private SoftValue() {}

    public static long misses() {
        return MISSES.get();
    }

    /** Отметить пропуск; возвращает {@code true}, если про этот метод ещё не писали. */
    public static boolean note(String key) {
        MISSES.incrementAndGet();
        if (!WARNED.add(key)) {
            return false;
        }
        Bridge.log(1, "api: " + key + " is not implemented, returning an empty value");
        return true;
    }

    public static <T> Optional<T> optional() {
        return Optional.empty();
    }

    public static <T> List<T> list() {
        return Collections.emptyList();
    }

    public static <T> Set<T> set() {
        return Collections.emptySet();
    }

    public static <K, V> Map<K, V> map() {
        return Collections.emptyMap();
    }

    public static <T> CompletableFuture<T> future() {
        return CompletableFuture.completedFuture(null);
    }

    public static String string() {
        return "";
    }

    /** Любое объектное значение: {@code null} — единственное, что подходит всем типам. */
    public static <T> T object() {
        return null;
    }

    /** Пустой массив нужного типа. */
    public static <T> T[] array(Class<T> component) {
        @SuppressWarnings("unchecked")
        T[] empty = (T[]) Array.newInstance(component, 0);
        return empty;
    }
}
