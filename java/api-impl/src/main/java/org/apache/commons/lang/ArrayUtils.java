package org.apache.commons.lang;

import java.lang.reflect.Array;

/**
 * Минимальный {@code commons-lang 2.x ArrayUtils}: Spigot/Paper держит
 * commons-lang в libraries, и плагины (DeluxeHub) его используют.
 */
public final class ArrayUtils {
    public static final Object[] EMPTY_OBJECT_ARRAY = new Object[0];

    private ArrayUtils() {
    }

    public static <T> T[] addAll(T[] array1, T... array2) {
        if (array1 == null) {
            return array2 == null ? null : array2.clone();
        }
        if (array2 == null) {
            return array1.clone();
        }
        @SuppressWarnings("unchecked")
        T[] joined = (T[]) Array.newInstance(
                array1.getClass().getComponentType(), array1.length + array2.length);
        System.arraycopy(array1, 0, joined, 0, array1.length);
        System.arraycopy(array2, 0, joined, array1.length, array2.length);
        return joined;
    }

    public static <T> T[] add(T[] array, T element) {
        if (array == null) {
            @SuppressWarnings("unchecked")
            T[] created = (T[]) Array.newInstance(element.getClass(), 1);
            created[0] = element;
            return created;
        }
        @SuppressWarnings("unchecked")
        T[] joined = (T[]) Array.newInstance(
                array.getClass().getComponentType(), array.length + 1);
        System.arraycopy(array, 0, joined, 0, array.length);
        joined[array.length] = element;
        return joined;
    }

    public static boolean contains(Object[] array, Object value) {
        if (array == null) {
            return false;
        }
        for (Object element : array) {
            if (value == null ? element == null : value.equals(element)) {
                return true;
            }
        }
        return false;
    }

    public static int indexOf(Object[] array, Object value) {
        if (array == null) {
            return -1;
        }
        for (int i = 0; i < array.length; i++) {
            if (value == null ? array[i] == null : value.equals(array[i])) {
                return i;
            }
        }
        return -1;
    }

    public static <T> T[] remove(T[] array, int index) {
        if (array == null || index < 0 || index >= array.length) {
            return array;
        }
        @SuppressWarnings("unchecked")
        T[] shorter = (T[]) Array.newInstance(
                array.getClass().getComponentType(), array.length - 1);
        System.arraycopy(array, 0, shorter, 0, index);
        System.arraycopy(array, index + 1, shorter, index, array.length - index - 1);
        return shorter;
    }

    public static boolean isEmpty(Object[] array) {
        return array == null || array.length == 0;
    }
}
