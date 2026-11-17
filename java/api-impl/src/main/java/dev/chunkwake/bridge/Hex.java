package dev.chunkwake.bridge;

/** hex ↔ bytes для передачи NBT через мост `world`. */
final class Hex {
    private static final char[] DIGITS = "0123456789abcdef".toCharArray();

    private Hex() {
    }

    static String encode(byte[] bytes) {
        StringBuilder out = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) {
            out.append(DIGITS[(b >> 4) & 15]);
            out.append(DIGITS[b & 15]);
        }
        return out.toString();
    }

    static byte[] decode(String text) {
        if (text.length() % 2 != 0) {
            throw new IllegalArgumentException("odd hex length");
        }
        byte[] out = new byte[text.length() / 2];
        for (int i = 0; i < out.length; i++) {
            out[i] = (byte) ((digit(text.charAt(i * 2)) << 4) | digit(text.charAt(i * 2 + 1)));
        }
        return out;
    }

    private static int digit(char c) {
        if (c >= '0' && c <= '9') {
            return c - '0';
        }
        if (c >= 'a' && c <= 'f') {
            return c - 'a' + 10;
        }
        if (c >= 'A' && c <= 'F') {
            return c - 'A' + 10;
        }
        throw new IllegalArgumentException("bad hex digit: " + c);
    }
}
