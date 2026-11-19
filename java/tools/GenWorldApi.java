import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.lang.reflect.Type;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Генерирует {@code dev.chunkwake.bridge.WorldApi}: настоящий Java-класс,
 * который наследует {@code org.bukkit.craftbukkit.CraftWorld} и реализует
 * {@code org.bukkit.World}, делегируя вызовы в {@code WorldImpl} через
 * {@code Stub.call}. Нужен плагинам, которые кастуют мир к CraftWorld
 * (squaremap): динамический прокси не может быть наследником класса.
 */
public final class GenWorldApi {
    public static void main(String[] args) throws Exception {
        Class<?> world = Class.forName("org.bukkit.World");
        Path out = Path.of(args[0]).resolve("dev/chunkwake/bridge/WorldApi.java");
        StringBuilder sb = new StringBuilder(1 << 16);
        sb.append("package dev.chunkwake.bridge;\n\n");
        sb.append("/** Сгенерировано GenWorldApi: настоящий класс мира поверх WorldImpl. */\n");
        sb.append("@SuppressWarnings({\"unchecked\", \"rawtypes\", \"deprecation\"})\n");
        sb.append("public final class WorldApi extends org.bukkit.craftbukkit.CraftWorld\n");
        sb.append("        implements org.bukkit.World, WorldHandle {\n");
        sb.append("    final Object impl;\n\n");
        sb.append("    public WorldApi(Object impl) {\n        this.impl = impl;\n    }\n\n");
        sb.append("    @Override\n");
        sb.append("    public net.minecraft.server.level.ServerLevel getHandle() {\n");
        sb.append("        return ((WorldImpl) impl).getHandle();\n    }\n\n");
        sb.append("    @Override\n");
        sb.append("    public boolean equals(Object o) {\n");
        sb.append("        return o == this || (o instanceof WorldApi w && w.impl == impl);\n    }\n\n");
        sb.append("    @Override\n    public int hashCode() {\n        return impl.hashCode();\n    }\n\n");
        sb.append("    @Override\n    public String toString() {\n        return impl.toString();\n    }\n\n");

        Map<String, Method> methods = new LinkedHashMap<>();
        for (Method m : world.getMethods()) {
            if (Modifier.isStatic(m.getModifiers()) || !m.getDeclaringClass().isInterface()) {
                continue;
            }
            String name = m.getName();
            if (name.equals("equals") || name.equals("hashCode") || name.equals("toString")) {
                continue;
            }
            String key = name + java.util.Arrays.toString(m.getParameterTypes());
            methods.putIfAbsent(key, m);
        }
        for (Method m : methods.values()) {
            sb.append("    @Override\n");
            sb.append("    public ").append(typeName(m.getReturnType())).append(' ').append(m.getName()).append('(');
            Class<?>[] params = m.getParameterTypes();
            for (int i = 0; i < params.length; i++) {
                if (i > 0) {
                    sb.append(", ");
                }
                sb.append(typeName(params[i])).append(" p").append(i);
            }
            sb.append(')');
            Class<?>[] thrown = m.getExceptionTypes();
            if (thrown.length > 0) {
                sb.append(" throws ");
                for (int i = 0; i < thrown.length; i++) {
                    if (i > 0) {
                        sb.append(", ");
                    }
                    sb.append(typeName(thrown[i]));
                }
            }
            sb.append(" {\n");
            sb.append("        ");
            if (m.getReturnType() != void.class) {
                sb.append("return (").append(typeName(m.getReturnType())).append(") ");
            }
            sb.append("Stub.call(impl, this, org.bukkit.World.class, \"").append(m.getName())
                    .append("\", new Class<?>[] {");
            for (int i = 0; i < params.length; i++) {
                if (i > 0) {
                    sb.append(", ");
                }
                sb.append(typeName(params[i])).append(".class");
            }
            sb.append("}, new Object[] {");
            for (int i = 0; i < params.length; i++) {
                if (i > 0) {
                    sb.append(", ");
                }
                sb.append("p").append(i);
            }
            sb.append("});\n");
            sb.append("    }\n\n");
        }
        sb.append("}\n");
        Files.createDirectories(out.getParent());
        Files.writeString(out, sb.toString());
        System.out.println("generated " + out + " (" + methods.size() + " methods)");
    }

    private static String typeName(Class<?> type) {
        if (type.isArray()) {
            return typeName(type.getComponentType()) + "[]";
        }
        String name = type.getCanonicalName();
        return name == null ? type.getName() : name;
    }

    private GenWorldApi() {
    }
}
