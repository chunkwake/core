import java.io.*;
import java.lang.reflect.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.jar.*;

/**
 * Генератор реализаций ServiceLoader-провайдеров paper-api.
 *
 * <p>Paper находит часть своего API через {@link java.util.ServiceLoader}
 * ({@code FeatureFlagProvider}, {@code VanillaArgumentProvider}, …). Эти
 * интерфейсы объявлены package-private, поэтому реализовать их можно только
 * классом в том же пакете — руками это сотня методов, которые к тому же
 * ломаются при каждой смене версии paper-api.
 *
 * <p>Задача читает paper-api.jar и для каждого такого интерфейса пишет класс
 * {@code Chunkwake<Имя>} в тот же пакет плюс файл
 * {@code META-INF/services/<интерфейс>}. Тела методов возвращают пустые
 * значения того же типа, что и {@code Stub}: провайдер существует, плагин
 * грузится, а чего не хватает — видно по сообщениям {@code api: …}.
 *
 * <p>Интерфейсы, у которых есть осмысленная реализация, перечислены в
 * {@link #HANDWRITTEN} и пропускаются — их пишем сами.
 */
public final class GenProviders {

    /**
     * Интерфейсы, у которых есть ручная реализация: их классы лежат в
     * {@code src/main/java}, сервис-файлы — в {@code src/main/resources}.
     * Генератор их не трогает.
     */
    private static final Set<String> HANDWRITTEN = Set.of(
            "io.papermc.paper.plugin.lifecycle.event.types.LifecycleEventTypeProvider");

    public static void main(String[] args) throws Exception {
        if (args.length < 3) {
            System.err.println("usage: GenProviders <paper-api.jar> <java-out-dir> <resources-out-dir>");
            System.exit(2);
        }
        Path jar = Path.of(args[0]);
        Path javaOut = Path.of(args[1]);
        Path resOut = Path.of(args[2]);
        Files.createDirectories(javaOut);
        Files.createDirectories(resOut);

        List<String> classNames = new ArrayList<>();
        try (JarFile jf = new JarFile(jar.toFile())) {
            Enumeration<JarEntry> entries = jf.entries();
            while (entries.hasMoreElements()) {
                String name = entries.nextElement().getName();
                if (name.endsWith(".class") && !name.contains("$")) {
                    classNames.add(name.substring(0, name.length() - 6).replace('/', '.'));
                }
            }
        }
        Collections.sort(classNames);

        URLClassLoader loader = new URLClassLoader(
                new URL[] {jar.toUri().toURL()}, GenProviders.class.getClassLoader());

        int providers = 0;
        int methods = 0;
        for (String name : classNames) {
            Class<?> type = load(name, loader);
            if (type == null || !type.isInterface() || !hasSelfProvider(type)) {
                continue;
            }
            if (HANDWRITTEN.contains(name)) {
                continue;
            }
            String simple = type.getSimpleName();
            String pkg = type.getPackageName();
            String className = "Chunkwake" + simple;
            String fqn = pkg + "." + className;
            StringBuilder src = new StringBuilder();
            src.append("// Сгенерировано GenProviders из paper-api. Не править: изменения пропадут.\n");
            src.append("package ").append(pkg).append(";\n\n");
            src.append("/**\n * Заглушка {@link ").append(simple).append("} для Chunkwake: провайдер\n")
                    .append(" * зарегистрирован, методы возвращают пустые значения.\n */\n");
            src.append("public final class ").append(className)
                    .append(" implements ").append(simple).append(" {\n\n");
            src.append("    public ").append(className).append("() {}\n");
            int count = 0;
            for (Method m : abstractMethods(type)) {
                src.append("\n    @Override\n    public ").append(declaration(m)).append(" {\n");
                src.append("        dev.chunkwake.bridge.SoftValue.note(\"").append(simple).append('.')
                        .append(m.getName()).append("\");\n");
                String ret = defaultReturn(m);
                if (ret != null) {
                    src.append("        return ").append(ret).append(";\n");
                }
                src.append("    }\n");
                count++;
            }
            src.append("}\n");
            Path file = javaOut.resolve(pkg.replace('.', '/')).resolve(className + ".java");
            Files.createDirectories(file.getParent());
            Files.writeString(file, src.toString(), StandardCharsets.UTF_8);
            writeService(resOut, name, fqn);
            providers++;
            methods += count;
            System.out.printf("  %-72s %2d methods -> %s%n", name, count, fqn);
        }
        System.out.printf("generated %d provider(s), %d method stub(s)%n", providers, methods);
    }

    private static Class<?> load(String name, ClassLoader loader) {
        try {
            return Class.forName(name, false, loader);
        } catch (Throwable t) {
            return null;
        }
    }

    /** У интерфейса есть константа {@code Optional<Себя>} — значит, его ищут через ServiceLoader. */
    private static boolean hasSelfProvider(Class<?> type) {
        for (Field f : type.getDeclaredFields()) {
            if (Modifier.isStatic(f.getModifiers()) && f.getType() == Optional.class
                    && f.getGenericType().getTypeName().contains(type.getName())) {
                return true;
            }
        }
        return false;
    }

    private static List<Method> abstractMethods(Class<?> type) {
        List<Method> out = new ArrayList<>();
        for (Method m : type.getDeclaredMethods()) {
            if (Modifier.isAbstract(m.getModifiers())) {
                out.add(m);
            }
        }
        out.sort(Comparator.comparing(Method::getName).thenComparing(Method::getParameterCount));
        return out;
    }

    /** Сигнатура метода полностью в квалифицированных именах — импорты не нужны. */
    private static String declaration(Method m) {
        StringBuilder s = new StringBuilder();
        // Параметры типа идут перед возвращаемым типом: `<T> ArgumentType<T> resource(...)`.
        TypeVariable<Method>[] vars = m.getTypeParameters();
        if (vars.length > 0) {
            s.append('<');
            for (int i = 0; i < vars.length; i++) {
                if (i > 0) {
                    s.append(", ");
                }
                s.append(vars[i].getName());
                Type[] bounds = vars[i].getBounds();
                if (bounds.length > 0 && bounds[0] != Object.class) {
                    s.append(" extends ").append(typeName(bounds[0]));
                }
            }
            s.append("> ");
        }
        s.append(typeName(m.getGenericReturnType())).append(' ').append(m.getName());
        s.append('(');
        Parameter[] params = m.getParameters();
        for (int i = 0; i < params.length; i++) {
            if (i > 0) {
                s.append(", ");
            }
            s.append(typeName(params[i].getParameterizedType())).append(" p").append(i);
        }
        s.append(')');
        Type[] thrown = m.getGenericExceptionTypes();
        if (thrown.length > 0) {
            s.append(" throws ");
            for (int i = 0; i < thrown.length; i++) {
                if (i > 0) {
                    s.append(", ");
                }
                s.append(typeName(thrown[i]));
            }
        }
        return s.toString();
    }

    /** {@code $} из вложенных классов → {@code .}: так удобнее читать и это валидный Java. */
    private static String typeName(Type type) {
        return type.getTypeName().replace('$', '.');
    }

    /** Тело заглушки: пустое значение ровно того типа, что ждёт интерфейс. */
    private static String defaultReturn(Method m) {
        Class<?> type = m.getReturnType();
        if (type == void.class) {
            return null;
        }
        // Примитив должен быть своего типа: интерфейс может ждать `byte`, а
        // `return 0` дал бы `int` и не скомпилировался (или дал бы CCE в прокси).
        if (type == boolean.class) {
            return "false";
        }
        if (type == char.class) {
            return "'\\0'";
        }
        if (type == byte.class) {
            return "(byte) 0";
        }
        if (type == short.class) {
            return "(short) 0";
        }
        if (type == int.class) {
            return "0";
        }
        if (type == long.class) {
            return "0L";
        }
        if (type == float.class) {
            return "0f";
        }
        if (type == double.class) {
            return "0d";
        }
        if (type == Optional.class) {
            return "dev.chunkwake.bridge.SoftValue.optional()";
        }
        if (type == java.util.concurrent.CompletableFuture.class) {
            return "dev.chunkwake.bridge.SoftValue.future()";
        }
        if (type == List.class || type == java.util.Collection.class || type == Iterable.class) {
            return "dev.chunkwake.bridge.SoftValue.list()";
        }
        if (type == Set.class) {
            return "dev.chunkwake.bridge.SoftValue.set()";
        }
        if (type == Map.class) {
            return "dev.chunkwake.bridge.SoftValue.map()";
        }
        if (type == String.class) {
            return "dev.chunkwake.bridge.SoftValue.string()";
        }
        if (type.isArray()) {
            String component = typeName(type.getComponentType());
            return "dev.chunkwake.bridge.SoftValue.array(" + component + ".class)";
        }
        return "dev.chunkwake.bridge.SoftValue.object()";
    }

    private static void writeService(Path resOut, String iface, String impl) throws IOException {
        Path dir = resOut.resolve("META-INF/services");
        Files.createDirectories(dir);
        Files.writeString(dir.resolve(iface), impl + "\n", StandardCharsets.UTF_8);
    }
}
