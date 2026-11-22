package net.minecraft.server.dedicated;

/** NMS {@code DedicatedServerProperties}. */
public class DedicatedServerProperties {
    public final int networkCompressionThreshold = 256;
    public boolean enforceSecureProfile;

    public DedicatedServerProperties() {
    }

    public int getNetworkCompressionThreshold() {
        return this.networkCompressionThreshold;
    }
}
