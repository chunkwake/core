package net.minecraft.server.level;

import java.util.concurrent.atomic.AtomicInteger;

/**
 * Минимальный NMS {@code ServerLevel}: DecentHolograms берёт из него
 * статический {@code ENTITY_COUNTER} для выдачи entity id.
 */
public class ServerLevel extends net.minecraft.world.level.Level {
    public static final AtomicInteger ENTITY_COUNTER = new AtomicInteger();
}
