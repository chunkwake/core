package dev.chunkwake.bridge;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import io.papermc.paper.datapack.Datapack;
import io.papermc.paper.datapack.DatapackRegistrar;
import io.papermc.paper.datapack.DatapackSource;
import io.papermc.paper.datapack.DiscoveredDatapack;
import io.papermc.paper.plugin.configuration.PluginMeta;
import io.papermc.paper.plugin.lifecycle.event.registrar.ReloadableRegistrarEvent;
import java.io.IOException;
import java.io.Reader;
import java.net.URI;
import java.nio.file.FileSystemAlreadyExistsException;
import java.nio.file.FileSystems;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.function.Consumer;
import java.util.stream.Stream;
import net.kyori.adventure.text.Component;
import org.bukkit.FeatureFlag;
import org.bukkit.Keyed;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.Tag;

/**
 * Теги блоков и предметов для {@code Server.getTag}.
 *
 * Ванильные теги держит Rust (уже раскрытые, запрос {@code tag block|item имя}).
 * Поверх них — теги из датапаков, которые плагины объявили через
 * {@code LifecycleEvents.DATAPACK_DISCOVERY}: как в ванили, файл без
 * {@code "replace": true} дополняет одноимённый тег, а не заменяет его.
 * Датапак целиком (рецепты, функции) не применяется — только теги, их
 * плагины читают из своих настроек ({@code #c:ores} у Veinminer).
 */
final class Tags {

    private Tags() {}

    /** Записи тегов из датапаков: {@code "block"|"item"} → имя → записи как в файле. */
    private static final Map<String, Map<String, PackTag>> PACKS = new ConcurrentHashMap<>();
    private static final Map<String, DiscoveredDatapack> DISCOVERED = new ConcurrentHashMap<>();
    private static final Map<String, Set<String>> RESOLVED = new ConcurrentHashMap<>();

    private record PackTag(boolean replace, List<String> values) {}

    /** {@code Server.getTag}: только реестры блоков и предметов, значения — {@link Material}. */
    @SuppressWarnings("unchecked")
    static <T extends Keyed> Tag<T> get(String registry, NamespacedKey key, Class<T> clazz) {
        String kind = switch (registry) {
            case Tag.REGISTRY_BLOCKS -> "block";
            case Tag.REGISTRY_ITEMS -> "item";
            default -> null;
        };
        if (kind == null || clazz != Material.class) {
            return null;
        }
        Set<String> ids = resolve(kind, key.toString(), new HashSet<>());
        if (ids == null) {
            return null;
        }
        Set<Material> values = new LinkedHashSet<>();
        for (String id : ids) {
            Material m = Material.matchMaterial(id);
            if (m != null) {
                values.add(m);
            }
        }
        return (Tag<T>) new MaterialTag(key, Collections.unmodifiableSet(values));
    }

    /** Состав тега с раскрытыми вложенными; {@code null} — тега нет нигде. */
    private static Set<String> resolve(String kind, String name, Set<String> visiting) {
        String cacheKey = kind + " " + name;
        Set<String> cached = RESOLVED.get(cacheKey);
        if (cached != null) {
            return cached;
        }
        if (!visiting.add(cacheKey)) {
            return Set.of();
        }
        PackTag pack = PACKS.getOrDefault(kind, Map.of()).get(name);
        Set<String> out = new LinkedHashSet<>();
        boolean known = false;
        if (pack == null || !pack.replace()) {
            String vanilla = Bridge.world("tag " + kind + " " + name);
            if (!vanilla.equals("!")) {
                known = true;
                if (!vanilla.isEmpty()) {
                    out.addAll(List.of(vanilla.split(" ")));
                }
            }
        }
        if (pack != null) {
            known = true;
            for (String value : pack.values()) {
                if (value.startsWith("#")) {
                    Set<String> nested = resolve(kind, value.substring(1), visiting);
                    if (nested != null) {
                        out.addAll(nested);
                    }
                } else {
                    out.add(value.contains(":") ? value : "minecraft:" + value);
                }
            }
        }
        visiting.remove(cacheKey);
        if (!known) {
            return null;
        }
        Set<String> result = Collections.unmodifiableSet(out);
        RESOLVED.put(cacheKey, result);
        return result;
    }

    /** Теги из {@code data/<ns>/tags/block|item/**.json} пака по пути (каталог или корень jar). */
    static int load(Path root) throws IOException {
        Path data = root.resolve("data");
        if (!Files.isDirectory(data)) {
            return 0;
        }
        int count = 0;
        List<Path> namespaces;
        try (Stream<Path> s = Files.list(data)) {
            namespaces = s.filter(Files::isDirectory).toList();
        }
        for (Path ns : namespaces) {
            String namespace = ns.getFileName().toString().replace("/", "");
            for (String kind : List.of("block", "item")) {
                Path dir = ns.resolve("tags").resolve(kind);
                if (!Files.isDirectory(dir)) {
                    continue;
                }
                List<Path> files;
                try (Stream<Path> s = Files.walk(dir)) {
                    files = s.filter(p -> p.toString().endsWith(".json")).toList();
                }
                for (Path file : files) {
                    String rel = dir.relativize(file).toString().replace('\\', '/');
                    String name = namespace + ":" + rel.substring(0, rel.length() - ".json".length());
                    PACKS.computeIfAbsent(kind, k -> new ConcurrentHashMap<>())
                            .merge(name, parse(file), Tags::merge);
                    count++;
                }
            }
        }
        RESOLVED.clear();
        return count;
    }

    private static PackTag merge(PackTag old, PackTag next) {
        if (next.replace()) {
            return next;
        }
        List<String> values = new ArrayList<>(old.values());
        values.addAll(next.values());
        return new PackTag(old.replace(), values);
    }

    private static PackTag parse(Path file) throws IOException {
        JsonObject json;
        try (Reader r = Files.newBufferedReader(file)) {
            json = JsonParser.parseReader(r).getAsJsonObject();
        }
        boolean replace = json.has("replace") && json.get("replace").getAsBoolean();
        List<String> values = new ArrayList<>();
        if (json.has("values")) {
            for (JsonElement e : json.getAsJsonArray("values")) {
                // `{ "id": ..., "required": false }` — необязательная запись.
                values.add(e.isJsonObject() ? e.getAsJsonObject().get("id").getAsString() : e.getAsString());
            }
        }
        return new PackTag(replace, values);
    }

    /** Корень пака по URI: каталог или путь внутри jar ({@code jar:file:...!/pack}). */
    private static Path open(URI uri) throws IOException {
        if (uri.getScheme().equals("jar")) {
            try {
                FileSystems.newFileSystem(uri, Map.of());
            } catch (FileSystemAlreadyExistsException ignored) {
                // Уже открыта: Path.of(uri) найдёт её сам.
            }
        }
        return Path.of(uri);
    }

    record Pack(String name) implements DiscoveredDatapack {
        @Override public String getName() { return name; }
        @Override public Component getTitle() { return Component.text(name); }
        @Override public Component getDescription() { return Component.empty(); }
        @Override public boolean isRequired() { return false; }
        @Override public Datapack.Compatibility getCompatibility() { return Datapack.Compatibility.COMPATIBLE; }
        @Override public Set<FeatureFlag> getRequiredFeatures() { return Set.of(); }
        @Override public DatapackSource getSource() { return DatapackSource.PLUGIN; }
    }

    /** Регистратор {@code DATAPACK_DISCOVERY}: из найденного пака берутся только теги. */
    static final class Registrar implements DatapackRegistrar {
        private final String owner;

        Registrar(String owner) {
            this.owner = owner;
        }

        @Override public boolean hasPackDiscovered(String name) { return DISCOVERED.containsKey(name); }
        @Override public DiscoveredDatapack getDiscoveredPack(String name) { return DISCOVERED.get(name); }
        @Override public boolean removeDiscoveredPack(String name) { return DISCOVERED.remove(name) != null; }
        @Override public Map<String, DiscoveredDatapack> getDiscoveredPacks() {
            return Collections.unmodifiableMap(new LinkedHashMap<>(DISCOVERED));
        }

        @Override
        public DiscoveredDatapack discoverPack(URI uri, String id, Consumer<Configurer> configurer) throws IOException {
            return discoverPack(open(uri), id, configurer);
        }

        @Override
        public DiscoveredDatapack discoverPack(Path path, String id, Consumer<Configurer> configurer)
                throws IOException {
            // Имя пака в Paper — `плагин/id`.
            String name = owner.toLowerCase(java.util.Locale.ROOT) + "/" + id;
            int tags = load(path);
            Bridge.log(3, "datapack " + name + ": " + tags + " tag file(s)");
            Pack pack = new Pack(name);
            DISCOVERED.put(name, pack);
            return pack;
        }

        @Override
        public DiscoveredDatapack discoverPack(PluginMeta meta, URI uri, String id, Consumer<Configurer> configurer)
                throws IOException {
            return discoverPack(uri, id, configurer);
        }

        @Override
        public DiscoveredDatapack discoverPack(PluginMeta meta, Path path, String id, Consumer<Configurer> configurer)
                throws IOException {
            return discoverPack(path, id, configurer);
        }
    }

    /** Событие {@code DATAPACK_DISCOVERY}. */
    record DiscoveryEvent(DatapackRegistrar registrar) implements ReloadableRegistrarEvent<DatapackRegistrar> {
        @Override public Cause cause() { return Cause.INITIAL; }
    }

    private record MaterialTag(NamespacedKey key, Set<Material> values) implements Tag<Material> {
        @Override public NamespacedKey getKey() { return key; }
        @Override public boolean isTagged(Material item) { return values.contains(item); }
        @Override public Set<Material> getValues() { return values; }
    }
}
