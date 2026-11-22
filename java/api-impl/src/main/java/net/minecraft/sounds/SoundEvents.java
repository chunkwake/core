package net.minecraft.sounds;

/** NMS {@code SoundEvents}: используемые CE звуки. */
public final class SoundEvents {
    public static final SoundEvent EMPTY =
            new SoundEvent(net.minecraft.resources.Identifier.parse("minecraft:intentionally_empty"));
    public static final SoundEvent TOTEM_USE =
            new SoundEvent(net.minecraft.resources.Identifier.parse("minecraft:item.totem.use"));

    private SoundEvents() {
    }
}
