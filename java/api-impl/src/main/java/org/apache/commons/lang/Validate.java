package org.apache.commons.lang;

import java.util.Collection;
import java.util.Map;

/**
 * Мини-совместимость с commons-lang 2.x: CraftBukkit традиционно нёс
 * {@code org.apache.commons.lang} в classpath, и плагины (ProtocolLib)
 * рассчитывают на него.
 */
public final class Validate {
    private Validate() {
    }

    public static void isTrue(boolean expression, String message) {
        if (!expression) {
            throw new IllegalArgumentException(message);
        }
    }

    public static void isTrue(boolean expression) {
        isTrue(expression, "[Assertion failed] - this expression must be true");
    }

    public static void notNull(Object object, String message) {
        if (object == null) {
            throw new IllegalArgumentException(message);
        }
    }

    public static void notNull(Object object) {
        notNull(object, "[Assertion failed] - this argument is required; it must not be null");
    }

    public static void notEmpty(Object[] array, String message) {
        if (array == null || array.length == 0) {
            throw new IllegalArgumentException(message);
        }
    }

    public static void notEmpty(Object[] array) {
        notEmpty(array, "[Assertion failed] - this array must not be empty: it must contain at least 1 element");
    }

    public static void notEmpty(String string, String message) {
        if (string == null || string.isEmpty()) {
            throw new IllegalArgumentException(message);
        }
    }

    public static void notEmpty(String string) {
        notEmpty(string, "[Assertion failed] - this string must not be empty");
    }

    public static void notEmpty(Collection<?> collection, String message) {
        if (collection == null || collection.isEmpty()) {
            throw new IllegalArgumentException(message);
        }
    }

    public static void notEmpty(Collection<?> collection) {
        notEmpty(collection, "[Assertion failed] - this collection must not be empty: it must contain at least 1 element");
    }

    public static void notEmpty(Map<?, ?> map, String message) {
        if (map == null || map.isEmpty()) {
            throw new IllegalArgumentException(message);
        }
    }

    public static void notEmpty(Map<?, ?> map) {
        notEmpty(map, "[Assertion failed] - this map must not be empty; it must contain at least one entry");
    }

    public static void noNullElements(Object[] array, String message) {
        notNull(array, message);
        for (Object element : array) {
            if (element == null) {
                throw new IllegalArgumentException(message);
            }
        }
    }

    public static void noNullElements(Object[] array) {
        noNullElements(array, "[Assertion failed] - this array must not contain any null elements");
    }
}
