package net.minecraft.world.entity.decoration;

import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.network.syncher.EntityDataSerializers;
import net.minecraft.world.entity.Entity;

/** NMS {@code ArmorStand} (метаданные; id 14 — Entity 0..7 + LivingEntity 8..13). */
public class ArmorStand extends Entity {
    /** Поля, которые ищет CraftEngine (позы по умолчанию). */
    private net.minecraft.world.entity.Pose defaultLeftArmPose =
            net.minecraft.world.entity.Pose.STANDING;
    private net.minecraft.world.entity.Pose defaultRightArmPose =
            net.minecraft.world.entity.Pose.STANDING;
    private net.minecraft.world.entity.Pose defaultBodyPose =
            net.minecraft.world.entity.Pose.STANDING;
    private net.minecraft.world.entity.Pose defaultHeadPose =
            net.minecraft.world.entity.Pose.STANDING;

    /** Статические поля, которые ищет CraftEngine (DEFAULT_*_POSE). */
    public static final net.minecraft.core.Rotations DEFAULT_HEAD_POSE =
            new net.minecraft.core.Rotations();
    public static final net.minecraft.core.Rotations DEFAULT_BODY_POSE =
            new net.minecraft.core.Rotations();
    public static final net.minecraft.core.Rotations DEFAULT_LEFT_ARM_POSE =
            new net.minecraft.core.Rotations();
    public static final net.minecraft.core.Rotations DEFAULT_RIGHT_ARM_POSE =
            new net.minecraft.core.Rotations();
    public static final net.minecraft.core.Rotations DEFAULT_LEFT_LEG_POSE =
            new net.minecraft.core.Rotations();
    public static final net.minecraft.core.Rotations DEFAULT_RIGHT_LEG_POSE =
            new net.minecraft.core.Rotations();

    public static final EntityDataAccessor<Byte> DATA_CLIENT_FLAGS =
            new EntityDataAccessor<>(14, EntityDataSerializers.BYTE);
}
