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
    @Override public ItemMeta getItemMeta() { return isEmpty() ? null : new MetaImpl(data.copy(), type).api; }
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
        /** Чары зачарованной книги — как пришли от сервера. */
        JsonObject stored;
        /** {@code custom_data}: compound NBT, в нём {@code PublicBukkitValues}. */
        Map<String, Object> custom = new LinkedHashMap<>();
        /** {@code writable_book_content} / {@code written_book_content}; нет книги — {@code null}. */
        BookData book;

        /** {@code custom_data} без пустого {@code PublicBukkitValues} (пустой PDC не сохраняется). */
        Map<String, Object> customOut() {
            if (custom.get(PDC) instanceof Map<?, ?> m && m.isEmpty()) {
                Map<String, Object> out = new LinkedHashMap<>(custom);
                out.remove(PDC);
                return out;
            }
            return custom;
        }

        boolean isEmpty() {
            return damage == 0 && enchants.isEmpty() && name == null && lore.isEmpty() && raw == null
                    && stored == null && book == null && customOut().isEmpty();
        }

        Data copy() {
            Data d = new Data();
            d.damage = damage;
            d.enchants.putAll(enchants);
            d.name = name;
            d.lore = new ArrayList<>(lore);
            d.raw = raw;
            d.stored = stored == null ? null : stored.deepCopy();
            d.book = book == null ? null : book.copy();
            @SuppressWarnings("unchecked")
            Map<String, Object> c = (Map<String, Object>) PdcImpl.copy(custom);
            d.custom = c;
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
            if (j.has("stored")) d.stored = j.getAsJsonObject("stored");
            if (j.has("book")) d.book = BookData.parse(j.getAsJsonObject("book"));
            if (j.has("custom")) d.custom = PdcImpl.Nbt.decode(j.getAsJsonObject("custom"));
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
            if (stored != null) j.add("stored", stored);
            if (book != null) j.add("book", book.json());
            if (!customOut().isEmpty()) j.add("custom", PdcImpl.Nbt.encode(customOut()));
            return j.toString();
        }

        @Override
        public boolean equals(Object o) {
            return o instanceof Data d && d.damage == damage && d.enchants.equals(enchants)
                    && Objects.equals(d.name, name) && d.lore.equals(lore) && Objects.equals(d.raw, raw)
                    && Objects.equals(d.stored, stored)
                    && Objects.equals(d.book, book)
                    && PdcImpl.Nbt.encode(d.customOut()).equals(PdcImpl.Nbt.encode(customOut()));
        }

        @Override
        public int hashCode() {
            return Objects.hash(damage, enchants, name, lore, raw, stored, book, PdcImpl.Nbt.encode(customOut()));
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
    /** Ключ данных плагинов в {@code custom_data} (как у CraftBukkit). */
    static final String PDC = "PublicBukkitValues";

    static final class MetaImpl {
        final ItemMeta api;
        final Data data;
        /** Тип стопки: у книг мета ещё и {@link org.bukkit.inventory.meta.BookMeta}, как CraftMetaBook. */
        final Material type;

        MetaImpl(Data data, Material type) {
            this.data = data;
            this.type = type;
            this.api = type == Material.WRITABLE_BOOK || type == Material.WRITTEN_BOOK
                    ? Stub.of(org.bukkit.inventory.meta.BookMeta.class, this)
                    : Stub.of(Damageable.class, this);
        }

        private static String legacy(Component c) {
            return net.kyori.adventure.text.serializer.legacy.LegacyComponentSerializer.legacySection().serialize(c);
        }

        private static Component fromLegacy(String s) {
            return net.kyori.adventure.text.serializer.legacy.LegacyComponentSerializer.legacySection().deserialize(s);
        }

        public org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
            if (!(data.custom.get(PDC) instanceof Map<?, ?>)) {
                data.custom.put(PDC, new LinkedHashMap<String, Object>());
            }
            @SuppressWarnings("unchecked")
            Map<String, Object> tags = (Map<String, Object>) data.custom.get(PDC);
            return new PdcImpl(tags).api;
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

        public ItemMeta clone() { return new MetaImpl(data.copy(), type).api; }

        // BookMeta / WritableBookMeta: страницы с 1, как у CraftMetaBook(Signed).
        private boolean written() {
            return data.book != null ? data.book.written : type == Material.WRITTEN_BOOK;
        }

        private BookData book() {
            if (data.book == null) {
                data.book = new BookData(type == Material.WRITTEN_BOOK);
            }
            return data.book;
        }

        private List<String> rawPages() {
            return data.book == null ? List.of() : data.book.pages;
        }

        private void checkPage(int page) {
            if (page < 1 || page > rawPages().size()) {
                throw new IllegalArgumentException("Invalid page number " + page + "/" + rawPages().size());
            }
        }

        private static Component component(String json) {
            return GsonComponentSerializer.gson().deserialize(json);
        }

        /** Страница в хранимый вид: у подписанной — JSON, у черновой — текст до 1024. */
        private String encode(Component c) {
            if (c == null) c = Component.empty();
            if (written()) return GsonComponentSerializer.gson().serialize(c);
            return truncate(legacy(c));
        }

        private String encode(String s) {
            if (s == null) s = "";
            return written() ? GsonComponentSerializer.gson().serialize(fromLegacy(s)) : truncate(s);
        }

        private static String truncate(String s) {
            return s.length() > BookData.WRITABLE_PAGE_LENGTH ? s.substring(0, BookData.WRITABLE_PAGE_LENGTH) : s;
        }

        public boolean hasPages() { return !rawPages().isEmpty(); }
        public int getPageCount() { return rawPages().size(); }

        public String getPage(int page) {
            checkPage(page);
            String p = rawPages().get(page - 1);
            return written() ? legacy(component(p)) : p;
        }

        public void setPage(int page, String text) {
            checkPage(page);
            book().pages.set(page - 1, encode(text));
        }

        public List<String> getPages() {
            List<String> out = new ArrayList<>();
            for (int i = 1; i <= rawPages().size(); i++) out.add(getPage(i));
            return out;
        }

        public void setPages(List<String> pages) {
            book().pages.clear();
            addPage(pages.toArray(String[]::new));
        }

        public void setPages(String... pages) { setPages(java.util.Arrays.asList(pages)); }

        public void addPage(String... pages) {
            for (String p : pages) {
                if (book().pages.size() >= BookData.MAX_PAGES) return;
                book().pages.add(encode(p));
            }
        }

        public Component page(int page) {
            checkPage(page);
            String p = rawPages().get(page - 1);
            return written() ? component(p) : fromLegacy(p);
        }

        public void page(int page, Component c) {
            checkPage(page);
            book().pages.set(page - 1, encode(c));
        }

        public List<Component> pages() {
            List<Component> out = new ArrayList<>();
            for (int i = 1; i <= rawPages().size(); i++) out.add(page(i));
            return out;
        }

        public void addPages(Component... pages) {
            for (Component c : pages) {
                if (book().pages.size() >= BookData.MAX_PAGES) return;
                book().pages.add(encode(c));
            }
        }

        /** {@code Book}-методы Adventure меняют копию, как CraftMetaBook. */
        public org.bukkit.inventory.meta.BookMeta pages(List<Component> pages) {
            MetaImpl m = new MetaImpl(data.copy(), type);
            m.book().pages.clear();
            m.addPages(pages.toArray(Component[]::new));
            return (org.bukkit.inventory.meta.BookMeta) m.api;
        }

        public boolean hasTitle() { return written() && data.book != null && data.book.title != null; }
        public String getTitle() { return hasTitle() ? data.book.title : null; }

        public boolean setTitle(String title) {
            if (!written()) return false;
            if (title != null && title.length() > BookData.TITLE_LENGTH) return false;
            book().title = title;
            return true;
        }

        public Component title() { return hasTitle() ? fromLegacy(getTitle()) : null; }

        public org.bukkit.inventory.meta.BookMeta title(Component title) {
            MetaImpl m = new MetaImpl(data.copy(), type);
            m.setTitle(title == null ? null : legacy(title));
            return (org.bukkit.inventory.meta.BookMeta) m.api;
        }

        public boolean hasAuthor() { return written() && data.book != null && data.book.author != null; }
        public String getAuthor() { return hasAuthor() ? data.book.author : null; }

        public void setAuthor(String author) {
            if (written()) book().author = author;
        }

        public Component author() { return hasAuthor() ? fromLegacy(getAuthor()) : null; }

        public org.bukkit.inventory.meta.BookMeta author(Component author) {
            MetaImpl m = new MetaImpl(data.copy(), type);
            m.setAuthor(author == null ? null : legacy(author));
            return (org.bukkit.inventory.meta.BookMeta) m.api;
        }

        public boolean hasGeneration() { return written() && data.book != null; }

        public org.bukkit.inventory.meta.BookMeta.Generation getGeneration() {
            var all = org.bukkit.inventory.meta.BookMeta.Generation.values();
            return data.book == null ? all[0] : all[Math.max(0, Math.min(all.length - 1, data.book.generation))];
        }

        public void setGeneration(org.bukkit.inventory.meta.BookMeta.Generation g) {
            if (written()) book().generation = g == null ? 0 : g.ordinal();
        }

        public org.bukkit.inventory.meta.BookMeta.Spigot spigot() { return new org.bukkit.inventory.meta.BookMeta.Spigot(); }

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
