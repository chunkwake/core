package com.destroystokyo.paper;

/**
 * Заглушка старого маркера Paper: PaperLib 1.x проверяет его через
 * {@code Class.forName("com.destroystokyo.paper.PaperConfig")}. Как и
 * {@code io.papermc.paper.configuration.PaperConfigurations}, нужен только
 * для определения платформы.
 */
public final class PaperConfig {
    private PaperConfig() {
    }
}
