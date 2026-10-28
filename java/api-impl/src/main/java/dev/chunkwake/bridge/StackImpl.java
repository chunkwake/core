package dev.chunkwake.bridge;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.serializer.gson.GsonComponentSerializer;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.Registry;
import org.bukkit.enchantments.Enchantment;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.Damageable;
import org.bukkit.inventory.meta.ItemMeta;

/**
 * {@link ItemStack} с компонентами, которые понимает Rust-сервер: износ, чары, имя и
 * описание (плюс непонятые сервером компоненты как есть). В Paper это CraftItemStack;
 * здесь — тот же «делегат», что возвращают {@code ItemStack.of} и {@code ItemType.createItemStack}.
 */
final class StackImpl extends ItemStack {
    private Material type;
    private int amount;
    private Data data;

    StackImpl(Material type, int amount) {
        this(type, amount, new Data());
    }

    private StackImpl(Material type, int amount, Data data) {
        this.type = type == null ? Material.AIR : type;
        this.amount = amount;
        this.data = data;
    }

    /** Стопка по имени предмета Rust-сервера ({@code minecraft:stone}); неизвестное — пусто. */
    static ItemStack of(String name, int count) {
        Material m = Material.matchMaterial(name);
        return new StackImpl(m == null ? Material.AIR : m, m == null ? 0 : count);
    }

    /** Строка Rust-сервера {@code item count [json]}; {@code -} или неизвестное — пусто. */
    static ItemStack parse(String line) {
        String[] f = line.trim().split(" ", 3);
        if (f.length < 2) {
            return new StackImpl(Material.AIR, 0);
        }
        Material m = Material.matchMaterial(f[0]);
        if (m == null) {
            return new StackImpl(Material.AIR, 0);
        }
        return new StackImpl(m, Integer.parseInt(f[1]), f.length == 3 ? Data.parse(f[2]) : new Data());
    }

    /** Имя для Rust: {@code minecraft:stone}; пустая стопка — {@code null}. */
    static String name(ItemStack s) {
        if (s == null || s.getType() == Material.AIR || s.getAmount() <= 0) {
            return null;
        }
        return s.getType().getKey().toString();
    }

    /** Строка для Rust (см. {@link #parse}); пустая стопка — {@code null}. */
    static String line(ItemStack s) {
        String name = name(s);
        if (name == null) {
            return null;
        }
        // `new ItemStack(...)` в Paper — обёртка над нашим делегатом: компоненты берём из меты.
        String json = data(s).json();
        return name + " " + s.getAmount() + (json == null ? "" : " " + json);
    }

    @Override public Material getType() { return type; }
    @Override public void setType(Material type) { this.type = Objects.requireNonNull(type, "type"); }
    @Override public int getAmount() { return amount; }
    @Override public void setAmount(int amount) { this.amount = amount; }
    @Override public int getMaxStackSize() { return Registries.maxStack(type); }
    @Override public boolean isEmpty() { return type == Material.AIR || amount <= 0; }
    @Override public boolean hasItemMeta() { return !isEmpty() && !data.isEmpty(); }
    @Override public ItemMeta getItemMeta() { return isEmpty() ? null : new MetaImpl(data.copy()).api; }
    @Override public short getDurability() { return (short) data.damage; }
    @Override public void setDurability(short d) { data.damage = Math.max(0, d); }

    @Override
    public boolean setItemMeta(ItemMeta meta) {
        if (meta == null) {
            data = new Data();
            return true;
        }
        if (Stub.impl(meta) instanceof MetaImpl m) {
            data = m.data.copy();
            return true;
        }
        return false;
    }

    @Override public Map<Enchantment, Integer> getEnchantments() { return Map.copyOf(data.enchants); }
    @Override public boolean containsEnchantment(Enchantment e) { return data.enchants.containsKey(e); }
    @Override public int getEnchantmentLevel(Enchantment e) { return data.enchants.getOrDefault(e, 0); }
    @Override public void addUnsafeEnchantment(Enchantment e, int level) { data.enchants.put(e, level); }
    @Override public void addUnsafeEnchantments(Map<Enchantment, Integer> all) { data.enchants.putAll(all); }
    @Override public void addEnchantments(Map<Enchantment, Integer> all) { all.forEach(this::addEnchantment); }
    @Override public void removeEnchantments() { data.enchants.clear(); }

    @Override
    public void addEnchantment(Enchantment e, int level) {
        Objects.requireNonNull(e, "Enchantment cannot be null");
        if (level < e.getStartLevel() || level > e.getMaxLevel()) {
            throw new IllegalArgumentException("Enchantment level is either too low or too high (given " + level
                    + ", bounds are " + e.getStartLevel() + " to " + e.getMaxLevel() + ")");
        }
        data.enchants.put(e, level);
    }

    @Override
    public int removeEnchantment(Enchantment e) {
        Integer old = data.enchants.remove(e);
        return old == null ? 0 : old;
    }

    @Override
    public boolean isSimilar(ItemStack other) {
        if (other == this) return true;
        if (other == null || other.getType() != type) return false;
        return data(other).equals(data);
    }

    /** Компоненты любой стопки: наша, обёртка Paper над нашей или чужая без меты. */
    private static Data data(ItemStack s) {
        if (s instanceof StackImpl i) return i.data;
        return s.hasItemMeta() && Stub.impl(s.getItemMeta()) instanceof MetaImpl m ? m.data : new Data();
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof ItemStack s && isSimilar(s) && s.getAmount() == amount;
    }

    @Override
    public int hashCode() {
        return Objects.hash(type, amount, data);
    }

    @Override
    public StackImpl clone() {
        return new StackImpl(type, amount, data.copy());
    }

    @Override
    public String toString() {
        return "ItemStack{" + type + " x " + amount + (data.isEmpty() ? "" : ", " + data) + "}";
    }

    /** Компоненты стопки. */
    static final class Data {
        int damage;
        final Map<Enchantment, Integer> enchants = new LinkedHashMap<>();
        Component name;
        List<Component> lore = new ArrayList<>();
        /** Непонятые сервером компоненты (hex), возвращаются как есть. */
        String raw;

        boolean isEmpty() {
            return damage == 0 && enchants.isEmpty() && name == null && lore.isEmpty() && raw == null;
        }

        Data copy() {
            Data d = new Data();
            d.damage = damage;
            d.enchants.putAll(enchants);
            d.name = name;
            d.lore = new ArrayList<>(lore);
            d.raw = raw;
            return d;
        }

        static Data parse(String json) {
            Data d = new Data();
            JsonObject j = JsonParser.parseString(json).getAsJsonObject();
            GsonComponentSerializer gson = GsonComponentSerializer.gson();
            if (j.has("damage")) d.damage = j.get("damage").getAsInt();
            if (j.has("enchantments")) {
                for (Map.Entry<String, JsonElement> e : j.getAsJsonObject("enchantments").entrySet()) {
                    NamespacedKey k = NamespacedKey.fromString(e.getKey());
                    Enchantment ench = k == null ? null : Registry.ENCHANTMENT.get(k);
                    if (ench != null) d.enchants.put(ench, e.getValue().getAsInt());
                }
            }
            if (j.has("name")) d.name = gson.deserializeFromTree(j.get("name"));
            if (j.has("lore")) {
                for (JsonElement l : j.getAsJsonArray("lore")) d.lore.add(gson.deserializeFromTree(l));
            }
            if (j.has("raw")) d.raw = j.get("raw").getAsString();
            return d;
        }

        /** JSON для Rust; без компонентов — {@code null}. */
        String json() {
            if (isEmpty()) return null;
            JsonObject j = new JsonObject();
            GsonComponentSerializer gson = GsonComponentSerializer.gson();
            if (damage > 0) j.addProperty("damage", damage);
            if (!enchants.isEmpty()) {
                JsonObject e = new JsonObject();
                enchants.forEach((k, v) -> e.addProperty(k.getKey().toString(), v));
                j.add("enchantments", e);
            }
            if (name != null) j.add("name", gson.serializeToTree(name));
            if (!lore.isEmpty()) {
                JsonArray a = new JsonArray();
                lore.forEach(l -> a.add(gson.serializeToTree(l)));
                j.add("lore", a);
            }
            if (raw != null) j.addProperty("raw", raw);
            return j.toString();
        }

        @Override
        public boolean equals(Object o) {
            return o instanceof Data d && d.damage == damage && d.enchants.equals(enchants)
                    && Objects.equals(d.name, name) && d.lore.equals(lore) && Objects.equals(d.raw, raw);
        }

        @Override
        public int hashCode() {
            return Objects.hash(damage, enchants, name, lore, raw);
        }

        @Override
        public String toString() {
            String j = json();
            return j == null ? "{}" : j;
        }
    }

    /**
     * {@link ItemMeta} и {@link Damageable} над копией компонентов: изменения попадают в
     * стопку через {@link ItemStack#setItemMeta}, как у CraftMetaItem.
     */
    static final class MetaImpl {
        final Damageable api = Stub.of(Damageable.class, this);
        final Data data;

        MetaImpl(Data data) {
            this.data = data;
        }

        private static String legacy(Component c) {
            return net.kyori.adventure.text.serializer.legacy.LegacyComponentSerializer.legacySection().serialize(c);
        }

        private static Component fromLegacy(String s) {
            return net.kyori.adventure.text.serializer.legacy.LegacyComponentSerializer.legacySection().deserialize(s);
        }

        public boolean hasDisplayName() { return data.name != null; }
        public Component displayName() { return data.name; }
        public void displayName(Component name) { data.name = name; }
        public String getDisplayName() { return data.name == null ? "" : legacy(data.name); }
        public void setDisplayName(String name) { data.name = name == null ? null : fromLegacy(name); }
        public boolean hasCustomName() { return hasDisplayName(); }
        public Component customName() { return data.name; }
        public void customName(Component name) { data.name = name; }

        public boolean hasLore() { return !data.lore.isEmpty(); }
        public List<Component> lore() { return data.lore.isEmpty() ? null : List.copyOf(data.lore); }
        public void lore(List<? extends Component> lore) { data.lore = lore == null ? new ArrayList<>() : new ArrayList<>(lore); }
        public List<String> getLore() {
            return data.lore.isEmpty() ? null : new ArrayList<>(data.lore.stream().map(MetaImpl::legacy).toList());
        }
        public void setLore(List<String> lore) {
            data.lore = lore == null ? new ArrayList<>() : new ArrayList<>(lore.stream().map(MetaImpl::fromLegacy).toList());
        }

        public boolean hasEnchants() { return !data.enchants.isEmpty(); }
        public boolean hasEnchant(Enchantment e) { return data.enchants.containsKey(e); }
        public int getEnchantLevel(Enchantment e) { return data.enchants.getOrDefault(e, 0); }
        public Map<Enchantment, Integer> getEnchants() { return Map.copyOf(data.enchants); }
        public boolean removeEnchantments() {
            boolean had = !data.enchants.isEmpty();
            data.enchants.clear();
            return had;
        }
        public boolean removeEnchant(Enchantment e) { return data.enchants.remove(e) != null; }
        public boolean hasConflictingEnchant(Enchantment e) {
            return data.enchants.keySet().stream().anyMatch(o -> !o.equals(e) && o.conflictsWith(e));
        }

        public boolean addEnchant(Enchantment e, int level, boolean ignoreLevelRestriction) {
            if (!ignoreLevelRestriction && (level < e.getStartLevel() || level > e.getMaxLevel())) {
                return false;
            }
            Integer old = data.enchants.put(e, level);
            return old == null || old != level;
        }

        public boolean hasDamage() { return data.damage > 0; }
        public int getDamage() { return data.damage; }
        public void setDamage(int damage) { data.damage = Math.max(0, damage); }
        public boolean hasDamageValue() { return hasDamage(); }
        public void resetDamage() { data.damage = 0; }

        public boolean isUnbreakable() { return false; }
        public java.util.Set<org.bukkit.inventory.ItemFlag> getItemFlags() { return java.util.Set.of(); }
        public boolean hasItemFlag(org.bukkit.inventory.ItemFlag flag) { return false; }

        public Damageable clone() { return new MetaImpl(data.copy()).api; }

        @Override
        public boolean equals(Object o) {
            return o instanceof ItemMeta m && Stub.impl(m) instanceof MetaImpl other && other.data.equals(data);
        }

        @Override
        public int hashCode() {
            return data.hashCode();
        }

        @Override
        public String toString() {
            return "ItemMeta" + data;
        }
    }
}
