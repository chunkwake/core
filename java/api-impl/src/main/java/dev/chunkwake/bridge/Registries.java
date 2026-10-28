package dev.chunkwake.bridge;

import io.papermc.paper.registry.RegistryAccess;
import io.papermc.paper.registry.RegistryKey;
import io.papermc.paper.registry.tag.Tag;
import io.papermc.paper.registry.tag.TagKey;
import java.util.Collection;
import java.util.Iterator;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.function.Function;
import java.util.stream.Stream;
import org.bukkit.Keyed;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.Registry;
import org.bukkit.damage.DamageEffect;
import org.bukkit.damage.DamageScaling;
import org.bukkit.damage.DamageType;
import org.bukkit.damage.DeathMessageType;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.ItemType;

/**
 * Реестры paper-api (находится через ServiceLoader). {@code Material.asItemType},
 * {@code ItemStack.of} и {@code DamageType.*} идут сюда; предметы — из данных
 * Rust-сервера, остальные реестры пока пустые.
 */
public final class Registries implements RegistryAccess {
    /** Предметы сервера: ключ → размер стопки. Заполняется до загрузки плагинов. */
    private static final Map<NamespacedKey, Integer> ITEM_STACKS = new ConcurrentHashMap<>();

    /*
     * Реестры создаются лениво в экземпляре: первый KeyedRegistry инициализирует
     * интерфейс Registry, а его <clinit> сам вызывает getRegistry — статические
     * поля в этот момент ещё были бы null.
     */
    private KeyedRegistry<ItemType> items;
    private KeyedRegistry<DamageType> damageTypes;

    private synchronized KeyedRegistry<ItemType> items() {
        if (items == null) {
            items = new KeyedRegistry<>(
                    k -> ITEM_STACKS.containsKey(k) ? Stub.of(ItemType.class, new ItemTypeImpl(k)) : null,
                    () -> ITEM_STACKS.keySet().stream());
        }
        return items;
    }

    private synchronized KeyedRegistry<DamageType> damageTypes() {
        if (damageTypes == null) {
            damageTypes = new KeyedRegistry<>(k -> Stub.of(DamageType.class, new DamageTypeImpl(k)), Stream::empty);
        }
        return damageTypes;
    }

    /** Строки {@code minecraft:stone 64}. */
    static void loadItems(String list) {
        for (String line : list.split("\n")) {
            String[] f = line.trim().split(" ");
            NamespacedKey key = f.length == 2 ? NamespacedKey.fromString(f[0]) : null;
            if (key != null) {
                ITEM_STACKS.put(key, Integer.parseInt(f[1]));
            }
        }
    }

    static int maxStack(Material m) {
        return ITEM_STACKS.getOrDefault(m.getKey(), 64);
    }

    @Override
    @SuppressWarnings("unchecked")
    public <T extends Keyed> Registry<T> getRegistry(Class<T> type) {
        if (type == ItemType.class) return (Registry<T>) items();
        if (type == DamageType.class) return (Registry<T>) damageTypes();
        return new KeyedRegistry<>(k -> null, Stream::empty);
    }

    @Override
    @SuppressWarnings("unchecked")
    public <T extends Keyed> Registry<T> getRegistry(RegistryKey<T> key) {
        if (key == RegistryKey.ITEM) return (Registry<T>) items();
        if (key == RegistryKey.DAMAGE_TYPE) return (Registry<T>) damageTypes();
        return new KeyedRegistry<>(k -> null, Stream::empty);
    }

    /** Реестр с ленивым созданием значений по ключу. */
    static final class KeyedRegistry<T extends Keyed> implements Registry<T> {
        private final Function<NamespacedKey, T> factory;
        private final java.util.function.Supplier<Stream<NamespacedKey>> keys;
        private final Map<NamespacedKey, T> cache = new ConcurrentHashMap<>();

        KeyedRegistry(Function<NamespacedKey, T> factory, java.util.function.Supplier<Stream<NamespacedKey>> keys) {
            this.factory = factory;
            this.keys = keys;
        }

        @Override
        public T get(NamespacedKey key) {
            T v = cache.get(key);
            if (v == null) {
                v = factory.apply(key);
                if (v != null) {
                    T prev = cache.putIfAbsent(key, v);
                    v = prev != null ? prev : v;
                }
            }
            return v;
        }

        @Override public NamespacedKey getKey(T value) { return value.getKey(); }
        @Override public boolean hasTag(TagKey<T> key) { return false; }
        @Override public Tag<T> getTag(TagKey<T> key) { throw new java.util.NoSuchElementException(key.toString()); }
        @Override public Collection<Tag<T>> getTags() { return List.of(); }
        @Override public Stream<T> stream() { return keys.get().map(this::get); }
        @Override public Stream<NamespacedKey> keyStream() { return keys.get(); }
        @Override public int size() { return (int) keys.get().count(); }
        @Override public Iterator<T> iterator() { return stream().iterator(); }
    }

    /** {@link ItemType}: ключ, материал, стопка, создание {@link ItemStack}. */
    static final class ItemTypeImpl {
        private final NamespacedKey key;

        ItemTypeImpl(NamespacedKey key) {
            this.key = key;
        }

        public NamespacedKey getKey() { return key; }
        public net.kyori.adventure.key.Key key() { return key; }
        public Material asMaterial() { return Material.matchMaterial(key.toString()); }
        public int getMaxStackSize() { return ITEM_STACKS.getOrDefault(key, 64); }
        public short getMaxDurability() { return 0; }
        public boolean hasBlockType() { return false; }
        public boolean isEdible() { return false; }
        public String getTranslationKey() { return "item." + key.getNamespace() + "." + key.getKey(); }
        public String translationKey() { return getTranslationKey(); }
        public ItemStack createItemStack() { return createItemStack(1); }
        public ItemStack createItemStack(int amount) { return new StackImpl(asMaterial(), amount); }

        @Override
        public String toString() {
            return key.toString();
        }
    }

    /** {@link DamageType}: ключ и ванильные свойства по умолчанию. */
    static final class DamageTypeImpl {
        private final NamespacedKey key;

        DamageTypeImpl(NamespacedKey key) {
            this.key = key;
        }

        public NamespacedKey getKey() { return key; }
        public net.kyori.adventure.key.Key key() { return key; }
        public String getTranslationKey() {
            return switch (key.getKey()) {
                case "out_of_world" -> "outOfWorld";
                default -> key.getKey().toLowerCase(Locale.ROOT);
            };
        }
        public DamageScaling getDamageScaling() { return DamageScaling.WHEN_CAUSED_BY_LIVING_NON_PLAYER; }
        public DamageEffect getDamageEffect() { return null; }
        public DeathMessageType getDeathMessageType() {
            return key.getKey().equals("fall") ? DeathMessageType.FALL_VARIANTS : DeathMessageType.DEFAULT;
        }
        public float getExhaustion() {
            return switch (key.getKey()) {
                case "fall", "out_of_world", "starve", "generic" -> 0.0f;
                default -> 0.1f;
            };
        }

        @Override
        public String toString() {
            return key.toString();
        }
    }
}
