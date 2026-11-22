package org.bukkit.craftbukkit;

/** NMS-тип частицы с Bukkit-зеркалом для CraftEngine. */
public class CraftParticleType extends net.minecraft.core.particles.ParticleType {
    public final org.bukkit.Particle particle;

    public CraftParticleType(org.bukkit.Particle particle) {
        this.particle = particle;
    }

    @Override
    public String toString() {
        return this.particle.name();
    }
}
