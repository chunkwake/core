package io.papermc.paper.plugin.entrypoint.classloader;

import java.net.URL;
import java.net.URLClassLoader;

/**
 * Paper-загрузчик плагина. CraftEngine ищет этот класс по имени, проверяет
 * {@code isInstance(pluginLoader)} и читает объявленное поле типа
 * {@link URLClassLoader}, чтобы добавлять библиотеки в classpath плагина
 * (`ClassPathAppender`).
 *
 * <p>Наш {@code PluginLoader} наследуется от этого класса, поэтому проверка
 * проходит, а {@code libraryLoader} указывает на него самого.
 */
public class PaperPluginClassLoader extends URLClassLoader {
    static {
        // Обязательно: PluginLoader регистрируется как parallel-capable,
        // а регистрация не срабатывает, если предок не зарегистрирован.
        ClassLoader.registerAsParallelCapable();
    }

    /** Поле, которое Paper-плагины находят рефлексией (тип важен). */
    private final URLClassLoader libraryLoader;

    public PaperPluginClassLoader(String name, URL[] urls, ClassLoader parent) {
        super(name, urls, parent);
        this.libraryLoader = this;
    }

    public PaperPluginClassLoader(URL[] urls, ClassLoader parent) {
        super(urls, parent);
        this.libraryLoader = this;
    }

    public URLClassLoader getLibraryLoader() {
        return this.libraryLoader;
    }
}
