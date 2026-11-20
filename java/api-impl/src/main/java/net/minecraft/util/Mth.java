package net.minecraft.util;

/** Заглушка NMS {@code Mth}: математические помощники (squaremap и др.). */
public final class Mth {
    private Mth() {
    }

    public static int ceillog2(int value) {
        return value <= 1 ? 0 : 32 - Integer.numberOfLeadingZeros(value - 1);
    }

    public static int clamp(int value, int min, int max) {
        return Math.max(min, Math.min(max, value));
    }

    public static long clamp(long value, long min, long max) {
        return Math.max(min, Math.min(max, value));
    }

    public static float clamp(float value, float min, float max) {
        return Math.max(min, Math.min(max, value));
    }

    public static double clamp(double value, double min, double max) {
        return Math.max(min, Math.min(max, value));
    }

    public static int floor(float value) {
        int i = (int) value;
        return value < i ? i - 1 : i;
    }

    public static int floor(double value) {
        int i = (int) value;
        return value < i ? i - 1 : i;
    }

    public static long lfloor(double value) {
        long l = (long) value;
        return value < l ? l - 1 : l;
    }

    public static int ceil(float value) {
        int i = (int) value;
        return value > i ? i + 1 : i;
    }

    public static int ceil(double value) {
        int i = (int) value;
        return value > i ? i + 1 : i;
    }

    public static int abs(int value) {
        return Math.abs(value);
    }

    public static float abs(float value) {
        return Math.abs(value);
    }

    public static double abs(double value) {
        return Math.abs(value);
    }

    public static int square(int value) {
        return value * value;
    }

    public static float square(float value) {
        return value * value;
    }

    public static double square(double value) {
        return value * value;
    }

    public static double cube(double value) {
        return value * value * value;
    }

    public static float lerp(float delta, float start, float end) {
        return start + delta * (end - start);
    }

    public static double lerp(double delta, double start, double end) {
        return start + delta * (end - start);
    }

    public static double lerp2(double deltaX, double deltaY, double x0y0, double x1y0,
            double x0y1, double x1y1) {
        return lerp(deltaY, lerp(deltaX, x0y0, x1y0), lerp(deltaX, x0y1, x1y1));
    }

    public static double lerp3(double deltaX, double deltaY, double deltaZ, double x0y0z0,
            double x1y0z0, double x0y1z0, double x1y1z0, double x0y0z1, double x1y0z1,
            double x0y1z1, double x1y1z1) {
        return lerp(deltaZ, lerp2(deltaX, deltaY, x0y0z0, x1y0z0, x0y1z0, x1y1z0),
                lerp2(deltaX, deltaY, x0y0z1, x1y0z1, x0y1z1, x1y1z1));
    }

    public static float sqrt(float value) {
        return (float) Math.sqrt(value);
    }

    public static float sin(float value) {
        return (float) Math.sin(value);
    }

    public static float cos(float value) {
        return (float) Math.cos(value);
    }

    public static int floorDiv(int x, int y) {
        return Math.floorDiv(x, y);
    }

    public static int positiveModulo(int x, int y) {
        return Math.floorMod(x, y);
    }

    public static float positiveModulo(float x, float y) {
        return x - y * (float) Math.floor(x / y);
    }

    public static double positiveModulo(double x, double y) {
        return x - y * Math.floor(x / y);
    }

    public static int wrap(int value, int min, int max) {
        int range = max - min;
        if (range <= 0) {
            return min;
        }
        int mod = positiveModulo(value - min, range);
        return mod + min;
    }

    public static float wrapDegrees(float value) {
        value = value % 360.0F;
        if (value >= 180.0F) {
            value -= 360.0F;
        }
        if (value < -180.0F) {
            value += 360.0F;
        }
        return value;
    }

    public static double wrapDegrees(double value) {
        value = value % 360.0D;
        if (value >= 180.0D) {
            value -= 360.0D;
        }
        if (value < -180.0D) {
            value += 360.0D;
        }
        return value;
    }

    public static int smallestEncompassingPowerOfTwo(int value) {
        int i = value - 1;
        i |= i >> 1;
        i |= i >> 2;
        i |= i >> 4;
        i |= i >> 8;
        i |= i >> 16;
        return i + 1;
    }

    public static boolean isPowerOfTwo(int value) {
        return value != 0 && (value & value - 1) == 0;
    }

    public static int fastFloor(double value) {
        return (int) (value + 1024.0D) - 1024;
    }

    public static boolean equal(float a, float b) {
        return Math.abs(b - a) < 1.0E-5F;
    }

    public static boolean equal(double a, double b) {
        return Math.abs(b - a) < 1.0E-5D;
    }

    public static int hsvToRgb(float hue, float saturation, float value) {
        int i = (int) (hue * 6.0F) % 6;
        float f = hue * 6.0F - (int) (hue * 6.0F);
        float p = value * (1.0F - saturation);
        float q = value * (1.0F - f * saturation);
        float t = value * (1.0F - (1.0F - f) * saturation);
        float r;
        float g;
        float b;
        switch (i) {
            case 0 -> {
                r = value;
                g = t;
                b = p;
            }
            case 1 -> {
                r = q;
                g = value;
                b = p;
            }
            case 2 -> {
                r = p;
                g = value;
                b = t;
            }
            case 3 -> {
                r = p;
                g = q;
                b = value;
            }
            case 4 -> {
                r = t;
                g = p;
                b = value;
            }
            default -> {
                r = value;
                g = p;
                b = q;
            }
        }
        int red = (int) (r * 255.0F);
        int green = (int) (g * 255.0F);
        int blue = (int) (b * 255.0F);
        return (red << 16) | (green << 8) | blue;
    }

    public static int murmurHash3Mixer(int value) {
        value ^= value >>> 16;
        value *= 0x85EBCA6B;
        value ^= value >>> 13;
        value *= 0xC2B2AE35;
        value ^= value >>> 16;
        return value;
    }
}
