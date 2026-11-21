package net.minecraft.world.entity.decoration;

import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.network.syncher.EntityDataSerializers;
import net.minecraft.world.entity.Entity;

/** NMS {@code ArmorStand} (метаданные; id 14 — Entity 0..7 + LivingEntity 8..13). */
public class ArmorStand extends Entity {
    /** Поля, которые ищет CraftEngine (позы по умолчанию). */
    private Pose defaultLeftArmPose = new Pose();
    private Pose defaultRightArmPose = new Pose();
    private Pose defaultBodyPose = new Pose();

    public static final EntityDataAccessor<Byte> DATA_CLIENT_FLAGS =
            new EntityDataAccessor<>(14, EntityDataSerializers.BYTE);
}
