package net.minecraft.world.entity;

/**
 * NMS {@code InsideBlockEffectApplier} (1.21.5+): эффекты при нахождении
 * сущности внутри блока. Нужен сигнатуре {@code BlockBehaviour.entityInside}.
 */
public final class InsideBlockEffectApplier {
    public static final InsideBlockEffectApplier NOOP = new InsideBlockEffectApplier();

    private InsideBlockEffectApplier() {
    }
}
