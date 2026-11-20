package net.minecraft.world.level.block.state;

/**
 * NMS {@code BlockBehaviour}: общие поля блока, которые читают
 * Paper-плагины (CraftEngine генерирует прокси по именам полей).
 */
public class BlockBehaviour {
    protected boolean hasCollision = true;
    protected float explosionResistance;
    protected boolean isRandomlyTicking;
    protected Object soundType;
    protected float friction = 0.6F;
    protected float speedFactor = 1.0F;
    protected float jumpFactor = 1.0F;
    protected float bounceRestitution;
    protected float fallDistanceReduction = 1.0F;
    protected boolean dynamicShape;
    protected Object requiredFeatures;
    protected Object properties;
    protected java.util.Optional<Object> drops = java.util.Optional.empty();
    protected String descriptionId = "";

    public BlockBehaviour() {
    }
}
