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
    @Override public CombatEntry createCombatEntry(LivingEntity e, DamageSource s, float d) { throw todo("createCombatEntry"); }
    @Override public CombatEntry createCombatEntry(DamageSource s, float d, FallLocationType f, float dist) { throw todo("createCombatEntry"); }
    @Override public Predicate<CommandSourceStack> restricted(Predicate<CommandSourceStack> p, boolean b) { return p; }
    @Override public ResolvableProfile defaultMannequinProfile() { throw todo("defaultMannequinProfile"); }
    @Override public SkinParts.Mutable allSkinParts() { throw todo("allSkinParts"); }
    @Override public Component defaultMannequinDescription() { throw todo("defaultMannequinDescription"); }
    @Override
    @SuppressWarnings("unchecked")
    public <M, L> GameRule<L> legacyGameRuleBridge(GameRule<M> r, Function<L, M> a, Function<M, L> b, Class<L> c) {
        return new RegistryValues.Legacy<>((RegistryValues.GameRuleImpl<M>) r, a, b, c);
    }
    @Override public Set<Pose> validMannequinPoses() { throw todo("validMannequinPoses"); }
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
    @Override public ItemStack deserializeItem(byte[] b) { throw todo("deserializeItem"); }
    @Override public boolean hasDefaultEntityAttributes(NamespacedKey k) { throw todo("hasDefaultEntityAttributes"); }
    @Override public Attributable getDefaultEntityAttributes(NamespacedKey k) { throw todo("getDefaultEntityAttributes"); }
    @Override public String getStatisticCriteriaKey(Statistic s) { throw todo("getStatisticCriteriaKey"); }
    @Override public ItemStack createEmptyStack() { return new StackImpl(org.bukkit.Material.AIR, 0); }
    @Override public Component resolveWithContext(Component c, CommandSender s, Entity e, boolean b) { return c; }
    @Override public ComponentFlattener componentFlattener() { return ComponentFlattener.basic(); }
    @Override public <C> PersistentDataKey<C> createPersistentDataKey(Key k, PersistentDataType<?, C> t) {
        return new PersistentDataKeyImpl<>(new NamespacedKey(k.namespace(), k.value()), t);
    }

    private record PersistentDataKeyImpl<C>(NamespacedKey key, PersistentDataType<?, C> dataType)
            implements PersistentDataKey<C> {
    }
    @Override public EnchantmentOffer[] rollEnchantmentOffers(ItemStack i, int a, int b, int c) { throw todo("rollEnchantmentOffers"); }
}
