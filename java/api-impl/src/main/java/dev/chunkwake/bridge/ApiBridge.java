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

    @Override public Biome constructLegacyCustomBiome() { throw todo("constructLegacyCustomBiome"); }
    @Override public CombatEntry createCombatEntry(LivingEntity e, DamageSource s, float d) { throw todo("createCombatEntry"); }
    @Override public CombatEntry createCombatEntry(DamageSource s, float d, FallLocationType f, float dist) { throw todo("createCombatEntry"); }
    @Override public Predicate<CommandSourceStack> restricted(Predicate<CommandSourceStack> p, boolean b) { return p; }
    @Override public ResolvableProfile defaultMannequinProfile() { throw todo("defaultMannequinProfile"); }
    @Override public SkinParts.Mutable allSkinParts() { throw todo("allSkinParts"); }
    @Override public Component defaultMannequinDescription() { throw todo("defaultMannequinDescription"); }
    @Override public <M, L> GameRule<L> legacyGameRuleBridge(GameRule<M> r, Function<L, M> a, Function<M, L> b, Class<L> c) { throw todo("legacyGameRuleBridge"); }
    @Override public Set<Pose> validMannequinPoses() { throw todo("validMannequinPoses"); }
    @Override public PoiType.Occupancy createOccupancy(String s) { throw todo("createOccupancy"); }
    @Override public DamageSource.Builder createDamageSourceBuilder(DamageType t) { return Events.sourceBuilder(t); }
    @Override public DamageEffect getDamageEffect(String s) { throw todo("getDamageEffect"); }
    @Override public String getTranslationKey(EntityType t) { throw todo("getTranslationKey"); }
    @Override public SpawnCategory getSpawnCategory(EntityType t) { throw todo("getSpawnCategory"); }
    @Override public ItemStack deserializeItem(byte[] b) { throw todo("deserializeItem"); }
    @Override public boolean hasDefaultEntityAttributes(NamespacedKey k) { throw todo("hasDefaultEntityAttributes"); }
    @Override public Attributable getDefaultEntityAttributes(NamespacedKey k) { throw todo("getDefaultEntityAttributes"); }
    @Override public String getStatisticCriteriaKey(Statistic s) { throw todo("getStatisticCriteriaKey"); }
    @Override public ItemStack createEmptyStack() { return new StackImpl(org.bukkit.Material.AIR, 0); }
    @Override public Component resolveWithContext(Component c, CommandSender s, Entity e, boolean b) { return c; }
    @Override public ComponentFlattener componentFlattener() { return ComponentFlattener.basic(); }
    @Override public <C> PersistentDataKey<C> createPersistentDataKey(Key k, PersistentDataType<?, C> t) { throw todo("createPersistentDataKey"); }
    @Override public EnchantmentOffer[] rollEnchantmentOffers(ItemStack i, int a, int b, int c) { throw todo("rollEnchantmentOffers"); }
}
