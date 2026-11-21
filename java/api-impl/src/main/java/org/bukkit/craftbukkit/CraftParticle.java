package org.bukkit.craftbukkit;

/** CraftBukkit-заглушка {@code CraftParticle}: CommandAPI проверяет наличие класса. */
public class CraftParticle {
    public CraftParticle() {
    }

    /** {@code CraftParticle.minecraftToBukkit(ParticleType)} (CraftEngine). */
    public static org.bukkit.Particle minecraftToBukkit(
            net.minecraft.core.particles.ParticleType type) {
        return null;
    }
}
