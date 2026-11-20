package com.mojang.logging;

import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/** NMS/логирование Mojang: {@code LogUtils.getLogger()}. */
public final class LogUtils {
    private LogUtils() {
    }

    public static Logger getLogger() {
        return LoggerFactory.getLogger("Minecraft");
    }

    public static Logger getLogger(String name) {
        return LoggerFactory.getLogger(name);
    }
}
