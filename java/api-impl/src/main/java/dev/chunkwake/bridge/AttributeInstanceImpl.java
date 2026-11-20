package dev.chunkwake.bridge;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.UUID;
import java.util.function.Supplier;
import net.kyori.adventure.key.Key;
import org.bukkit.NamespacedKey;
import org.bukkit.attribute.Attribute;
import org.bukkit.attribute.AttributeInstance;
import org.bukkit.attribute.AttributeModifier;

/**
 * {@link AttributeInstance} игрока или моба поверх Rust-сервера (запрос
 * {@code attr <uuid> <атрибут> …}): та же база и те же модификаторы, что у
 * {@code /attribute}, клиент получает {@code update_attributes}.
 *
 * <p>Видны все модификаторы, как у ванильного {@code AttributeInstance}:
 * эффекты, чары, команды и плагины. Обычные и «временные»
 * ({@code addTransientModifier}) хранятся одинаково — сохранения атрибутов
 * между входами на сервере пока нет.
 */
final class AttributeInstanceImpl implements AttributeInstance {
    private final Supplier<UUID> owner;
    private final Attribute attribute;
    /** База, заданная плагином, если Rust не подтвердил атрибут. */
    private Double fallbackBase;

    AttributeInstanceImpl(Supplier<UUID> owner, Attribute attribute) {
        this.owner = owner;
        this.attribute = attribute;
    }

    /**
     * Экземпляр атрибута сущности (как {@code LivingEntity.getAttribute}).
     *
     * <p>Ванильная семантика: для стандартных атрибутов возвращается
     * экземпляр, а не {@code null}. Раньше при пустом ответе Rust
     * возвращался {@code null}, и EliteMobs падал на
     * {@code getBaseValue()} у null. Теперь база по умолчанию берётся из
     * {@link AttributeImpl#getDefaultValue()}.
     */
    static AttributeInstance of(Supplier<UUID> owner, Attribute attribute) {
        if (attribute == null) {
            return null;
        }
        return new AttributeInstanceImpl(owner, attribute);
    }

    private String request(String action) {
        return Registries.world("attr " + owner.get() + " " + attribute.getKey() + " " + action);
    }

    /** Ответ {@code get}: первая строка — база, значение, умолчание; дальше модификаторы. */
    private List<String> query() {
        String reply = request("get");
        if (!reply.isEmpty()) {
            return List.of(reply.split("\n"));
        }
        double base = this.fallbackBase != null
                ? this.fallbackBase
                : (this.attribute instanceof AttributeImpl impl ? impl.getDefaultValue() : 0.0);
        return List.of(base + " " + base + " " + base);
    }

    private double number(int field) {
        List<String> q = query();
        return q.isEmpty() ? 0.0 : Double.parseDouble(q.get(0).split(" ")[field]);
    }

    @Override public Attribute getAttribute() { return attribute; }
    @Override public double getBaseValue() { return number(0); }
    @Override public double getValue() { return number(1); }
    @Override public double getDefaultValue() { return number(2); }

    @Override
    public void setBaseValue(double value) {
        if (!Double.isFinite(value)) {
            throw new IllegalArgumentException("Base value must be finite");
        }
        this.fallbackBase = value;
        request("base " + value);
    }

    @Override
    public Collection<AttributeModifier> getModifiers() {
        List<AttributeModifier> out = new ArrayList<>();
        List<String> q = query();
        for (String line : q.subList(Math.min(1, q.size()), q.size())) {
            String[] f = line.split(" ");
            NamespacedKey key = NamespacedKey.fromString(f[0]);
            int op = Integer.parseInt(f[2]);
            if (key != null && op >= 0 && op < AttributeModifier.Operation.values().length) {
                out.add(new AttributeModifier(key, Double.parseDouble(f[1]),
                        AttributeModifier.Operation.values()[op]));
            }
        }
        return out;
    }

    @Override
    public AttributeModifier getModifier(Key key) {
        for (AttributeModifier m : getModifiers()) {
            if (m.getKey().equals(key)) {
                return m;
            }
        }
        return null;
    }

    @Override
    public void removeModifier(Key key) {
        request("remove " + key.asString());
    }

    /** UUID старых модификаторов: Paper переводит его в ключ {@code minecraft:<uuid>}. */
    @Override
    @SuppressWarnings("deprecation")
    public AttributeModifier getModifier(UUID uuid) {
        return getModifier(NamespacedKey.minecraft(uuid.toString()));
    }

    @Override
    public void removeModifier(UUID uuid) {
        removeModifier(NamespacedKey.minecraft(uuid.toString()));
    }

    @Override
    public void addModifier(AttributeModifier modifier) {
        if (modifier == null) {
            throw new IllegalArgumentException("modifier");
        }
        if (!"1".equals(request("add " + modifier.getKey() + " " + modifier.getAmount() + " "
                + modifier.getOperation().ordinal()))) {
            throw new IllegalArgumentException("Modifier is already applied on this attribute!");
        }
    }

    @Override
    public void addTransientModifier(AttributeModifier modifier) {
        addModifier(modifier);
    }

    @Override
    public void removeModifier(AttributeModifier modifier) {
        if (modifier == null) {
            throw new IllegalArgumentException("modifier");
        }
        removeModifier(modifier.getKey());
    }

    @Override
    public String toString() {
        return "AttributeInstance{" + attribute.getKey() + "}";
    }
}
