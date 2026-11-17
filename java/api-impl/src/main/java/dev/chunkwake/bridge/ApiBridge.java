package dev.chunkwake.bridge;

import com.destroystokyo.paper.SkinParts;
import io.papermc.paper.InternalAPIBridge;
import io.papermc.paper.command.brigadier.CommandSourceStack;
import io.papermc.paper.datacomponent.item.ResolvableProfile;
import io.papermc.paper.entity.poi.PoiType;
import io.papermc.paper.persistence.PersistentDataKey;
import io.papermc.paper.plugin.lifecycle.event.LifecycleEventManager;
import io.papermc.paper.world.damagesource.CombatEntry;
import io.papermc.paper.world.damagesource.FallLocationType;
import java.util.Set;
import java.util.function.BooleanSupplier;
import java.util.function.Function;
import java.util.function.Predicate;
import net.kyori.adventure.key.Key;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.flattener.ComponentFlattener;
import org.bukkit.GameRule;
import org.bukkit.NamespacedKey;
import org.bukkit.Statistic;
import org.bukkit.attribute.Attributable;
import org.bukkit.block.Biome;
import org.bukkit.command.CommandSender;
import org.bukkit.damage.DamageEffect;
import org.bukkit.damage.DamageSource;
import org.bukkit.damage.DamageType;
import org.bukkit.enchantments.EnchantmentOffer;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;
import org.bukkit.entity.LivingEntity;
import org.bukkit.entity.Pose;
import org.bukkit.entity.SpawnCategory;
import org.bukkit.inventory.ItemStack;
import org.bukkit.persistence.PersistentDataType;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.java.JavaPlugin;

/**
 * Внутренний мост paper-api к серверу (находится через ServiceLoader).
 * На MVP реализовано только то, что нужно для создания плагина.
 */
public final class ApiBridge implements InternalAPIBridge {
    private static UnsupportedOperationException todo(String what) {
        return new UnsupportedOperationException("chunkwake: InternalAPIBridge." + what + " is not implemented yet");
    }

    @Override
    public LifecycleEventManager<Plugin> createPluginLifecycleEventManager(JavaPlugin plugin, BooleanSupplier allowed) {
        // Метаданные плагина появляются только после init(), то есть позже этого
        // вызова, — поэтому имя берём лениво.
        return Lifecycle.manager(() -> {
            var meta = plugin.getPluginMeta();
            return meta == null ? plugin.toString() : meta.getName();
        });
    }

    /** {@code Biome.CUSTOM}: как Paper {@code LegacyCustomBiomeImpl} — ключ {@code minecraft:custom}, имя CUSTOM. */
    @Override public Biome constructLegacyCustomBiome() {
        return Stub.of(Biome.class, new RegistryValues.KeyedValue(new org.bukkit.NamespacedKey("minecraft", "custom"), -1,
                "worldgen/biome") {
            @Override public String name() { return "CUSTOM"; }
            @Override public String toString() { return "CUSTOM"; }
        });
    }
    @Override public CombatEntry createCombatEntry(LivingEntity e, DamageSource s, float d) {
        return new CombatEntryImpl(s, d, null, e == null ? 0.0f : e.getFallDistance());
    }
    @Override public CombatEntry createCombatEntry(DamageSource s, float d, FallLocationType f, float dist) {
        return new CombatEntryImpl(s, d, f, dist);
    }

    private record CombatEntryImpl(DamageSource source, float damage, FallLocationType fall,
            float fallDistance) implements CombatEntry {
        @Override public DamageSource getDamageSource() { return this.source; }
        @Override public float getDamage() { return this.damage; }
        @Override public FallLocationType getFallLocationType() { return this.fall; }
        @Override public float getFallDistance() { return this.fallDistance; }
    }
    @Override public Predicate<CommandSourceStack> restricted(Predicate<CommandSourceStack> p, boolean b) { return p; }
    @Override public ResolvableProfile defaultMannequinProfile() {
        // Ваниль: `Mannequin.DEFAULT_PROFILE = ResolvableProfile.Static.EMPTY`.
        return ResolvableProfile.resolvableProfile().build();
    }
    @Override public SkinParts.Mutable allSkinParts() { return new SkinPartsImpl(); }
    @Override public Component defaultMannequinDescription() {
        // Ваниль: `Component.translatable("entity.minecraft.mannequin.label")`.
        return Component.translatable("entity.minecraft.mannequin.label");
    }
    @Override
    @SuppressWarnings("unchecked")
    public <M, L> GameRule<L> legacyGameRuleBridge(GameRule<M> r, Function<L, M> a, Function<M, L> b, Class<L> c) {
        return new RegistryValues.Legacy<>((RegistryValues.GameRuleImpl<M>) r, a, b, c);
    }
    @Override public Set<Pose> validMannequinPoses() {
        // Ваниль: `Mannequin.VALID_POSES` = STANDING, CROUCHING, SWIMMING,
        // FALL_FLYING, SLEEPING (Mojmap CROUCHING = Bukkit SNEAKING).
        return Set.of(
                Pose.STANDING, Pose.SNEAKING, Pose.SWIMMING, Pose.FALL_FLYING, Pose.SLEEPING);
    }
    @Override public DamageSource.Builder createDamageSourceBuilder(DamageType t) { return Events.sourceBuilder(t); }
    private static final java.util.Map<String, DamageEffect> DAMAGE_EFFECTS = new java.util.HashMap<>();
    private static final java.util.Map<String, PoiType.Occupancy> OCCUPANCIES = new java.util.HashMap<>();

    @Override public DamageEffect getDamageEffect(String s) {
        return DAMAGE_EFFECTS.computeIfAbsent(s, name -> new DamageEffectImpl(switch (name) {
            case "hurt" -> () -> org.bukkit.Sound.ENTITY_PLAYER_HURT;
            case "thorns" -> () -> org.bukkit.Sound.ENCHANT_THORNS_HIT;
            case "drowning" -> () -> org.bukkit.Sound.ENTITY_PLAYER_HURT_DROWN;
            case "burning" -> () -> org.bukkit.Sound.ENTITY_PLAYER_HURT_ON_FIRE;
            case "poking" -> () -> org.bukkit.Sound.ENTITY_PLAYER_HURT_SWEET_BERRY_BUSH;
            case "freezing" -> () -> org.bukkit.Sound.ENTITY_PLAYER_HURT_FREEZE;
            default -> throw new IllegalArgumentException("Unknown damage effect: " + name);
        }));
    }

    /** Звук резолвится лениво: константы {@code Sound}
     * требуют загруженного реестра. */
    private record DamageEffectImpl(java.util.function.Supplier<org.bukkit.Sound> sound)
            implements DamageEffect {
        @Override public org.bukkit.Sound getSound() {
            return this.sound.get();
        }
    }

    @Override public PoiType.Occupancy createOccupancy(String s) {
        return OCCUPANCIES.computeIfAbsent(s, name -> switch (name) {
            case "HAS_SPACE", "IS_OCCUPIED", "ANY" -> new OccupancyImpl(name);
            default -> throw new IllegalArgumentException("Unknown occupancy: " + name);
        });
    }

    private record OccupancyImpl(String name) implements PoiType.Occupancy {
        @Override public String toString() {
            return this.name;
        }
    }
    @Override public String getTranslationKey(EntityType t) {
        // Ваниль: `EntityType.getDescriptionId()` = "entity.<namespace>.<path>".
        NamespacedKey key = t.getKey();
        return "entity." + key.getNamespace() + "." + key.getKey();
    }
    @Override public SpawnCategory getSpawnCategory(EntityType t) {
        return EntityCategories.of(t.name());
    }
    @Override public ItemStack deserializeItem(byte[] bytes) {
        if (bytes == null || bytes.length == 0) {
            throw new IllegalArgumentException("cannot deserialize nothing");
        }
        // Формат `CraftItemStack.serializeAsBytes`: gzip-NBT `ItemStack.CODEC`.
        try (var in = new java.util.zip.GZIPInputStream(new java.io.ByteArrayInputStream(bytes))) {
            String line = Registries.world("itemfromnbt " + Hex.encode(in.readAllBytes()));
            if (line.isEmpty()) {
                throw new IllegalArgumentException("invalid item bytes");
            }
            return StackImpl.parse(line);
        } catch (java.io.IOException error) {
            throw new IllegalArgumentException("invalid item bytes", error);
        }
    }
    @Override public boolean hasDefaultEntityAttributes(NamespacedKey k) {
        String name = entityName(k);
        return name != null && DefaultAttributeValues.has(name);
    }

    @Override public Attributable getDefaultEntityAttributes(NamespacedKey k) {
        String name = entityName(k);
        return name == null ? null : DefaultAttributeValues.create(name);
    }

    private static String entityName(NamespacedKey key) {
        for (EntityType type : EntityType.values()) {
            if (type != EntityType.UNKNOWN && type.getKey().equals(key)) {
                return type.name();
            }
        }
        return null;
    }
    @Override public String getStatisticCriteriaKey(Statistic s) {
        // Ванильные custom-статистики: критерий `minecraft.custom:<stat>`,
        // `Statistic.getKey()` = `minecraft:<имя в нижнем регистре>`.
        // `Statistic.getKey()` = `minecraft:deaths`; критерий ванили —
        // `minecraft.custom:minecraft.deaths` (namespace и path через точку).
        NamespacedKey key = s.getKey();
        return "minecraft.custom:" + key.getNamespace() + "." + key.getKey();
    }
    @Override public ItemStack createEmptyStack() { return new StackImpl(org.bukkit.Material.AIR, 0); }
    @Override public Component resolveWithContext(Component c, CommandSender s, Entity e, boolean b) { return c; }
    @Override public ComponentFlattener componentFlattener() { return ComponentFlattener.basic(); }
    @Override public <C> PersistentDataKey<C> createPersistentDataKey(Key k, PersistentDataType<?, C> t) {
        return new PersistentDataKeyImpl<>(new NamespacedKey(k.namespace(), k.value()), t);
    }

    private record PersistentDataKeyImpl<C>(NamespacedKey key, PersistentDataType<?, C> dataType)
            implements PersistentDataKey<C> {
    }
    @Override public EnchantmentOffer[] rollEnchantmentOffers(ItemStack item, int seed, int bookshelves, int amount) {
        if (item == null || item.getType().isAir() || amount <= 0) {
            return new EnchantmentOffer[0];
        }
        String reply = Registries.world("enchant offers " + item.getType().getKey() + " " + seed + " "
                + bookshelves);
        EnchantmentOffer[] offers = new EnchantmentOffer[amount];
        String[] lines = reply.split("\n");
        for (int i = 0; i < amount && i < lines.length; i++) {
            String[] fields = lines[i].split(" ", 3);
            if (fields.length < 3) {
                continue;
            }
            int cost = Integer.parseInt(fields[0]);
            int level = Integer.parseInt(fields[1]);
            if (cost <= 0 || "-".equals(fields[2])) {
                continue;
            }
            org.bukkit.enchantments.Enchantment enchantment = org.bukkit.Registry.ENCHANTMENT
                    .get(NamespacedKey.fromString(fields[2]));
            if (enchantment != null) {
                offers[i] = new EnchantmentOffer(enchantment, level, cost);
            }
        }
        return offers;
    }

    /**
     * `SkinParts.Mutable`: сырые биты как у ванильного
     * `player.getModelPartCustomisation()` (установлен = слой скрыт).
     */
    private static final class SkinPartsImpl implements SkinParts.Mutable {
        private static final int CAPE = 1;
        private static final int JACKET = 2;
        private static final int LEFT_SLEEVE = 4;
        private static final int RIGHT_SLEEVE = 8;
        private static final int LEFT_PANTS = 16;
        private static final int RIGHT_PANTS = 32;
        private static final int HAT = 64;

        private int hidden;

        @Override public boolean hasCapeEnabled() { return (this.hidden & CAPE) == 0; }
        @Override public boolean hasJacketEnabled() { return (this.hidden & JACKET) == 0; }
        @Override public boolean hasLeftSleeveEnabled() { return (this.hidden & LEFT_SLEEVE) == 0; }
        @Override public boolean hasRightSleeveEnabled() { return (this.hidden & RIGHT_SLEEVE) == 0; }
        @Override public boolean hasLeftPantsEnabled() { return (this.hidden & LEFT_PANTS) == 0; }
        @Override public boolean hasRightPantsEnabled() { return (this.hidden & RIGHT_PANTS) == 0; }
        @Override public boolean hasHatsEnabled() { return (this.hidden & HAT) == 0; }

        @Override public void setCapeEnabled(boolean enabled) { this.hidden = set(CAPE, enabled); }
        @Override public void setJacketEnabled(boolean enabled) { this.hidden = set(JACKET, enabled); }
        @Override public void setLeftSleeveEnabled(boolean enabled) { this.hidden = set(LEFT_SLEEVE, enabled); }
        @Override public void setRightSleeveEnabled(boolean enabled) { this.hidden = set(RIGHT_SLEEVE, enabled); }
        @Override public void setLeftPantsEnabled(boolean enabled) { this.hidden = set(LEFT_PANTS, enabled); }
        @Override public void setRightPantsEnabled(boolean enabled) { this.hidden = set(RIGHT_PANTS, enabled); }
        @Override public void setHatsEnabled(boolean enabled) { this.hidden = set(HAT, enabled); }

        private int set(int mask, boolean enabled) {
            return enabled ? this.hidden & ~mask : this.hidden | mask;
        }

        @Override public int getRaw() { return this.hidden; }
        @Override public SkinParts.Mutable mutableCopy() {
            SkinPartsImpl copy = new SkinPartsImpl();
            copy.hidden = this.hidden;
            return copy;
        }
        @Override public SkinParts immutableCopy() { return mutableCopy(); }

        @Override public String toString() { return "SkinParts{raw=" + this.hidden + "}"; }
    }
}
