package org.apache.commons.lang;

/** Совместимость: старый commons-lang 2.x (EconomyShopGUI). */
public final class StringUtils {
    private StringUtils() {
    }

    public static boolean isEmpty(String value) {
        return value == null || value.isEmpty();
    }

    public static boolean isNotEmpty(String value) {
        return !isEmpty(value);
    }

    public static boolean isBlank(String value) {
        if (value == null) {
            return true;
        }
        for (int i = 0; i < value.length(); i++) {
            if (!Character.isWhitespace(value.charAt(i))) {
                return false;
            }
        }
        return true;
    }

    public static boolean isNotBlank(String value) {
        return !isBlank(value);
    }

    public static boolean equals(String a, String b) {
        return a == null ? b == null : a.equals(b);
    }

    public static boolean equalsIgnoreCase(String a, String b) {
        return a == null ? b == null : a.equalsIgnoreCase(b);
    }

    public static boolean contains(String value, String search) {
        return value != null && search != null && value.contains(search);
    }

    public static boolean containsIgnoreCase(String value, String search) {
        return value != null && search != null
                && value.toLowerCase(java.util.Locale.ROOT)
                        .contains(search.toLowerCase(java.util.Locale.ROOT));
    }

    public static boolean startsWith(String value, String prefix) {
        return value != null && prefix != null && value.startsWith(prefix);
    }

    public static boolean startsWithIgnoreCase(String value, String prefix) {
        return value != null && prefix != null
                && value.toLowerCase(java.util.Locale.ROOT)
                        .startsWith(prefix.toLowerCase(java.util.Locale.ROOT));
    }

    public static boolean endsWith(String value, String suffix) {
        return value != null && suffix != null && value.endsWith(suffix);
    }

    public static String[] split(String value, String separator) {
        return value == null ? null : value.split(java.util.regex.Pattern.quote(separator));
    }

    public static String[] split(String value, char separator) {
        return value == null ? null : split(value, String.valueOf(separator));
    }

    public static String join(Object[] parts, String separator) {
        if (parts == null) {
            return null;
        }
        StringBuilder out = new StringBuilder();
        for (int i = 0; i < parts.length; i++) {
            if (i > 0) {
                out.append(separator);
            }
            out.append(parts[i]);
        }
        return out.toString();
    }

    public static String join(java.util.Collection<?> parts, String separator) {
        return parts == null ? null : join(parts.toArray(), separator);
    }

    public static String capitalize(String value) {
        if (isEmpty(value)) {
            return value;
        }
        return Character.toUpperCase(value.charAt(0)) + value.substring(1);
    }

    public static String uncapitalize(String value) {
        if (isEmpty(value)) {
            return value;
        }
        return Character.toLowerCase(value.charAt(0)) + value.substring(1);
    }

    public static String lowerCase(String value) {
        return value == null ? null : value.toLowerCase(java.util.Locale.ROOT);
    }

    public static String upperCase(String value) {
        return value == null ? null : value.toUpperCase(java.util.Locale.ROOT);
    }

    public static String trim(String value) {
        return value == null ? null : value.trim();
    }

    public static String trimToNull(String value) {
        String trimmed = trim(value);
        return isEmpty(trimmed) ? null : trimmed;
    }

    public static String defaultString(String value) {
        return value == null ? "" : value;
    }

    public static String defaultString(String value, String fallback) {
        return value == null ? fallback : value;
    }

    public static String substringBefore(String value, String separator) {
        if (isEmpty(value) || separator == null) {
            return value;
        }
        int index = value.indexOf(separator);
        return index < 0 ? value : value.substring(0, index);
    }

    public static String substringAfter(String value, String separator) {
        if (isEmpty(value)) {
            return value;
        }
        if (separator == null) {
            return "";
        }
        int index = value.indexOf(separator);
        return index < 0 ? "" : value.substring(index + separator.length());
    }

    public static String removeStart(String value, String prefix) {
        if (isEmpty(value) || isEmpty(prefix)) {
            return value;
        }
        return value.startsWith(prefix) ? value.substring(prefix.length()) : value;
    }

    public static String removeEnd(String value, String suffix) {
        if (isEmpty(value) || isEmpty(suffix)) {
            return value;
        }
        return value.endsWith(suffix) ? value.substring(0, value.length() - suffix.length()) : value;
    }

    public static String remove(String value, char remove) {
        if (isEmpty(value)) {
            return value;
        }
        StringBuilder out = new StringBuilder(value.length());
        for (int i = 0; i < value.length(); i++) {
            char c = value.charAt(i);
            if (c != remove) {
                out.append(c);
            }
        }
        return out.toString();
    }

    public static String remove(String value, String remove) {
        if (isEmpty(value) || isEmpty(remove)) {
            return value;
        }
        return value.replace(remove, "");
    }

    public static String deleteWhitespace(String value) {
        if (isEmpty(value)) {
            return value;
        }
        StringBuilder out = new StringBuilder(value.length());
        for (int i = 0; i < value.length(); i++) {
            char c = value.charAt(i);
            if (!Character.isWhitespace(c)) {
                out.append(c);
            }
        }
        return out.toString();
    }

    public static String strip(String value) {
        return trim(value);
    }

    public static String strip(String value, String stripChars) {
        if (isEmpty(value)) {
            return value;
        }
        String chars = stripChars == null ? " " : stripChars;
        int start = 0;
        int end = value.length();
        while (start < end && chars.indexOf(value.charAt(start)) >= 0) {
            start++;
        }
        while (end > start && chars.indexOf(value.charAt(end - 1)) >= 0) {
            end--;
        }
        return value.substring(start, end);
    }

    public static String chomp(String value) {
        if (isEmpty(value)) {
            return value;
        }
        if (value.endsWith("\r\n")) {
            return value.substring(0, value.length() - 2);
        }
        char last = value.charAt(value.length() - 1);
        return last == '\n' || last == '\r' ? value.substring(0, value.length() - 1) : value;
    }

    public static String repeat(String value, int repeat) {
        if (value == null || repeat <= 0) {
            return "";
        }
        StringBuilder out = new StringBuilder(value.length() * repeat);
        for (int i = 0; i < repeat; i++) {
            out.append(value);
        }
        return out.toString();
    }

    public static String reverse(String value) {
        return value == null ? null : new StringBuilder(value).reverse().toString();
    }

    public static String swapCase(String value) {
        if (isEmpty(value)) {
            return value;
        }
        StringBuilder out = new StringBuilder(value.length());
        for (int i = 0; i < value.length(); i++) {
            char c = value.charAt(i);
            out.append(Character.isUpperCase(c) ? Character.toLowerCase(c)
                    : Character.toUpperCase(c));
        }
        return out.toString();
    }

    public static String left(String value, int len) {
        if (value == null) {
            return null;
        }
        return len < 0 ? "" : value.substring(0, Math.min(len, value.length()));
    }

    public static String right(String value, int len) {
        if (value == null) {
            return null;
        }
        return len < 0 ? "" : value.substring(Math.max(0, value.length() - len));
    }

    public static String mid(String value, int pos, int len) {
        if (value == null) {
            return null;
        }
        if (len < 0 || pos > value.length()) {
            return "";
        }
        int start = Math.max(0, pos);
        int end = Math.min(start + len, value.length());
        return value.substring(start, end);
    }

    public static int indexOf(String value, char search) {
        return value == null ? -1 : value.indexOf(search);
    }

    public static int indexOf(String value, String search) {
        return value == null || search == null ? -1 : value.indexOf(search);
    }

    public static int lastIndexOf(String value, char search) {
        return value == null ? -1 : value.lastIndexOf(search);
    }

    public static int lastIndexOf(String value, String search) {
        return value == null || search == null ? -1 : value.lastIndexOf(search);
    }

    public static boolean containsAny(String value, char[] searchChars) {
        if (isEmpty(value) || searchChars == null) {
            return false;
        }
        for (int i = 0; i < value.length(); i++) {
            char c = value.charAt(i);
            for (char search : searchChars) {
                if (c == search) {
                    return true;
                }
            }
        }
        return false;
    }

    public static boolean equalsAny(String value, String[] candidates) {
        if (candidates == null) {
            return false;
        }
        for (String candidate : candidates) {
            if (equals(value, candidate)) {
                return true;
            }
        }
        return false;
    }

    public static boolean equalsAnyIgnoreCase(String value, String[] candidates) {
        if (candidates == null) {
            return false;
        }
        for (String candidate : candidates) {
            if (equalsIgnoreCase(value, candidate)) {
                return true;
            }
        }
        return false;
    }

    public static String replace(String value, String search, String replacement) {
        return value == null ? null : value.replace(search == null ? "" : search,
                replacement == null ? "" : replacement);
    }

    public static int countMatches(String value, String search) {
        if (isEmpty(value) || isEmpty(search)) {
            return 0;
        }
        int count = 0;
        int index = 0;
        while ((index = value.indexOf(search, index)) >= 0) {
            count++;
            index += search.length();
        }
        return count;
    }
}
