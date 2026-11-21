package net.minecraft.core.particles;

/** NMS-заглушка {@code ParticleOptions} (CommandAPI проверяет наличие). */
public class ParticleOptions {
    public ParticleOptions() {
    }

    /** {@code ParticleOptions.writeToNetwork(FriendlyByteBuf)} (CraftEngine). */
    public void writeToNetwork(net.minecraft.network.FriendlyByteBuf buffer) {
    }

    /** {@code ParticleOptions.getType()} (CraftEngine). */
    public ParticleType getType() {
        return null;
    }
}
