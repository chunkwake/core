package net.minecraft.world.level;

/**
 * NMS {@code ScheduledTickAccess}: доступ к планировщику тиков.
 *
 * <p>Нужен сигнатуре {@code BlockBehaviour.updateShape}; Chunkwake пока не
 * планирует тики блоков, поэтому интерфейс пустой.
 */
public interface ScheduledTickAccess {
}
