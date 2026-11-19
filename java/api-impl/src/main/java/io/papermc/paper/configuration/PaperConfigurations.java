package io.papermc.paper.configuration;

/**
 * Заглушка внутреннего класса Paper: плагины (NpcPlugin, PaperLib и др.)
 * определяют Paper через {@code Class.forName("io.papermc.paper.configuration.PaperConfigurations")}.
 * На настоящем Paper класс есть, у нас его не было — плагин считал сервер
 * Spigot и отключался. Методы сюда добавлять только по мере надобности:
 * для детекта достаточно самого класса.
 */
public final class PaperConfigurations {
    private PaperConfigurations() {
    }
}
