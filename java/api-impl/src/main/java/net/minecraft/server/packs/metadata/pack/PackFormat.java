package net.minecraft.server.packs.metadata.pack;

/** NMS {@code PackFormat}: major/minor версия формата ресурсов/data pack. */
public final class PackFormat implements Comparable<PackFormat> {
    public static final com.mojang.serialization.Codec<PackFormat> BOTTOM_CODEC = null;
    public static final com.mojang.serialization.Codec<PackFormat> TOP_CODEC = null;

    private final int major;
    private final int minor;

    public PackFormat(int major, int minor) {
        this.major = major;
        this.minor = minor;
    }

    public static PackFormat of(int major) {
        return new PackFormat(major, 0);
    }

    public static PackFormat of(int major, int minor) {
        return new PackFormat(major, minor);
    }

    public int major() {
        return this.major;
    }

    public int minor() {
        return this.minor;
    }

    @Override
    public int compareTo(PackFormat other) {
        int byMajor = Integer.compare(this.major, other.major);
        return byMajor != 0 ? byMajor : Integer.compare(this.minor, other.minor);
    }

    @Override
    public String toString() {
        return this.minor == 0 ? Integer.toString(this.major)
                : this.major + "." + this.minor;
    }
}
