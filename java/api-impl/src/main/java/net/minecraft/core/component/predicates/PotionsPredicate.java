package net.minecraft.core.component.predicates;

/** NMS {@code PotionsPredicate} (CraftEngine brewing). */
public final class PotionsPredicate {
    private final java.util.Optional<?> potions;
    private final java.util.Optional<?> effects;

    public PotionsPredicate(java.util.Optional<?> potions, java.util.Optional<?> effects) {
        this.potions = potions;
        this.effects = effects;
    }

    public java.util.Optional<?> potions() {
        return this.potions;
    }

    public java.util.Optional<?> effects() {
        return this.effects;
    }
}
