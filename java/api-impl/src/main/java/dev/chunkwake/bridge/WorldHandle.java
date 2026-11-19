package dev.chunkwake.bridge;

/** Дополнительный интерфейс World-прокси: NMS-уровень (Orebfuscator). */
interface WorldHandle {
    net.minecraft.server.level.ServerLevel getHandle();
}
