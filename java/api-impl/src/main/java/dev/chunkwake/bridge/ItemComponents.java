package dev.chunkwake.bridge;

import io.papermc.paper.datacomponent.DataComponentType;
import io.papermc.paper.datacomponent.item.BannerPatternLayers;
import io.papermc.paper.datacomponent.item.BlockItemDataProperties;
import io.papermc.paper.datacomponent.item.BundleContents;
import io.papermc.paper.datacomponent.item.ChargedProjectiles;
import io.papermc.paper.datacomponent.item.CustomModelData;
import io.papermc.paper.datacomponent.item.Fireworks;
import io.papermc.paper.datacomponent.item.DamageResistant;
import io.papermc.paper.datacomponent.item.DyedItemColor;
import io.papermc.paper.datacomponent.item.Enchantable;
import io.papermc.paper.datacomponent.item.ItemArmorTrim;
import io.papermc.paper.datacomponent.item.ItemContainerContents;
import io.papermc.paper.datacomponent.item.ItemEnchantments;
import io.papermc.paper.datacomponent.item.ItemLore;
import io.papermc.paper.datacomponent.item.JukeboxPlayable;
import io.papermc.paper.datacomponent.item.LodestoneTracker;
import io.papermc.paper.datacomponent.item.MapDecorations;
import io.papermc.paper.datacomponent.item.MapId;
import io.papermc.paper.datacomponent.item.OminousBottleAmplifier;
import io.papermc.paper.datacomponent.item.PotDecorations;
import io.papermc.paper.datacomponent.item.Repairable;
import io.papermc.paper.datacomponent.item.SeededContainerLoot;
import io.papermc.paper.datacomponent.item.SuspiciousStewEffects;
import io.papermc.paper.datacomponent.item.TooltipDisplay;
import io.papermc.paper.datacomponent.item.UseRemainder;
import io.papermc.paper.registry.set.RegistryKeySet;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import io.papermc.paper.potion.SuspiciousEffectEntry;
import org.bukkit.block.banner.Pattern;
import net.kyori.adventure.key.Key;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.ComponentLike;
import org.bukkit.Color;
import org.bukkit.FireworkEffect;
import org.bukkit.JukeboxSong;
import org.bukkit.damage.DamageType;
import org.bukkit.enchantments.Enchantment;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.ItemType;
import org.bukkit.inventory.meta.trim.ArmorTrim;
import org.bukkit.map.MapCursor;

/**
 * Реальные значения для {@code ItemComponentTypesBridge}: билдеры хранят
 * данные и отдают неизменяемые снимки через {@code build()}.
 */
public final class ItemComponents {
    private ItemComponents() {
    }

    // --- lore ---

    public static ItemLore.Builder lore() {
        return new LoreBuilder();
    }

    static final class LoreBuilder implements ItemLore.Builder {
        private final List<Component> lines = new ArrayList<>();

        @Override public ItemLore.Builder lines(List<? extends ComponentLike> lines) {
            this.lines.clear();
            return addLines(lines);
        }

        @Override public ItemLore.Builder addLine(ComponentLike line) {
            if (line != null) {
                this.lines.add(line.asComponent());
            }
            return this;
        }

        @Override public ItemLore.Builder addLines(List<? extends ComponentLike> lines) {
            if (lines != null) {
                for (ComponentLike line : lines) {
                    addLine(line);
                }
            }
            return this;
        }

        @Override public ItemLore build() {
            return new LoreValue(List.copyOf(this.lines));
        }
    }

    record LoreValue(List<Component> lines) implements ItemLore {
        @Override public List<Component> styledLines() {
            return this.lines;
        }
    }

    // --- enchantments ---

    public static ItemEnchantments.Builder enchantments() {
        return new EnchantmentsBuilder();
    }

    static final class EnchantmentsBuilder implements ItemEnchantments.Builder {
        private final Map<Enchantment, Integer> enchantments = new LinkedHashMap<>();

        @Override public ItemEnchantments.Builder add(Enchantment enchantment, int level) {
            if (enchantment != null) {
                this.enchantments.put(enchantment, level);
            }
            return this;
        }

        @Override public ItemEnchantments.Builder addAll(Map<Enchantment, Integer> values) {
            if (values != null) {
                this.enchantments.putAll(values);
            }
            return this;
        }

        @Override public ItemEnchantments build() {
            return new EnchantmentsValue(Map.copyOf(this.enchantments));
        }
    }

    record EnchantmentsValue(Map<Enchantment, Integer> enchantments) implements ItemEnchantments {
    }

    // --- dyed color ---

    public static DyedItemColor.Builder dyedItemColor() {
        return new DyedColorBuilder();
    }

    static final class DyedColorBuilder implements DyedItemColor.Builder {
        private Color color;

        @Override public DyedItemColor.Builder color(Color color) {
            this.color = color;
            return this;
        }

        @Override public DyedItemColor build() {
            return new DyedColorValue(this.color);
        }
    }

    record DyedColorValue(Color color) implements DyedItemColor {
        @Override public Color color() {
            return this.color;
        }
    }

    // --- custom model data ---

    public static CustomModelData.Builder customModelData() {
        return new CustomModelDataBuilder();
    }

    static final class CustomModelDataBuilder implements CustomModelData.Builder {
        private final List<Float> floats = new ArrayList<>();
        private final List<Boolean> flags = new ArrayList<>();
        private final List<String> strings = new ArrayList<>();
        private final List<Color> colors = new ArrayList<>();

        @Override public CustomModelData.Builder addFloat(float value) {
            this.floats.add(value);
            return this;
        }

        @Override public CustomModelData.Builder addFloats(List<Float> values) {
            if (values != null) {
                this.floats.addAll(values);
            }
            return this;
        }

        @Override public CustomModelData.Builder addFlag(boolean value) {
            this.flags.add(value);
            return this;
        }

        @Override public CustomModelData.Builder addFlags(List<Boolean> values) {
            if (values != null) {
                this.flags.addAll(values);
            }
            return this;
        }

        @Override public CustomModelData.Builder addString(String value) {
            this.strings.add(value);
            return this;
        }

        @Override public CustomModelData.Builder addStrings(List<String> values) {
            if (values != null) {
                this.strings.addAll(values);
            }
            return this;
        }

        @Override public CustomModelData.Builder addColor(Color value) {
            this.colors.add(value);
            return this;
        }

        @Override public CustomModelData.Builder addColors(List<Color> values) {
            if (values != null) {
                this.colors.addAll(values);
            }
            return this;
        }

        @Override public CustomModelData build() {
            return new CustomModelDataValue(List.copyOf(this.floats), List.copyOf(this.flags),
                    List.copyOf(this.strings), List.copyOf(this.colors));
        }
    }

    record CustomModelDataValue(List<Float> floats, List<Boolean> flags, List<String> strings,
            List<Color> colors) implements CustomModelData {
    }

    // --- tooltip ---

    public static TooltipDisplay.Builder tooltipDisplay() {
        return new TooltipDisplayBuilder();
    }

    static final class TooltipDisplayBuilder implements TooltipDisplay.Builder {
        private boolean hide;
        private final Set<DataComponentType> hidden = new LinkedHashSet<>();

        @Override public TooltipDisplay.Builder hideTooltip(boolean hide) {
            this.hide = hide;
            return this;
        }

        @Override public TooltipDisplay.Builder addHiddenComponents(DataComponentType... types) {
            if (types != null) {
                for (DataComponentType type : types) {
                    if (type != null) {
                        this.hidden.add(type);
                    }
                }
            }
            return this;
        }

        @Override public TooltipDisplay.Builder hiddenComponents(Set<DataComponentType> types) {
            this.hidden.clear();
            if (types != null) {
                this.hidden.addAll(types);
            }
            return this;
        }

        @Override public TooltipDisplay build() {
            return new TooltipDisplayValue(this.hide, Set.copyOf(this.hidden));
        }
    }

    record TooltipDisplayValue(boolean hideTooltip, Set<DataComponentType> hiddenComponents)
            implements TooltipDisplay {
    }

    // --- pot decorations ---

    public static PotDecorations.Builder potDecorations() {
        return new PotDecorationsBuilder();
    }

    static final class PotDecorationsBuilder implements PotDecorations.Builder {
        private ItemType back;
        private ItemType left;
        private ItemType right;
        private ItemType front;

        @Override public PotDecorations.Builder back(ItemType type) {
            this.back = type;
            return this;
        }

        @Override public PotDecorations.Builder left(ItemType type) {
            this.left = type;
            return this;
        }

        @Override public PotDecorations.Builder right(ItemType type) {
            this.right = type;
            return this;
        }

        @Override public PotDecorations.Builder front(ItemType type) {
            this.front = type;
            return this;
        }

        @Override public PotDecorations build() {
            return new PotDecorationsValue(this.back, this.left, this.right, this.front);
        }
    }

    record PotDecorationsValue(ItemType back, ItemType left, ItemType right, ItemType front)
            implements PotDecorations {
    }

    // --- charged projectiles ---

    public static ChargedProjectiles.Builder chargedProjectiles() {
        return new ChargedProjectilesBuilder();
    }

    static final class ChargedProjectilesBuilder implements ChargedProjectiles.Builder {
        private final List<ItemStack> projectiles = new ArrayList<>();

        @Override public ChargedProjectiles.Builder add(ItemStack stack) {
            if (stack != null) {
                this.projectiles.add(stack.clone());
            }
            return this;
        }

        @Override public ChargedProjectiles.Builder addAll(List<ItemStack> stacks) {
            if (stacks != null) {
                for (ItemStack stack : stacks) {
                    add(stack);
                }
            }
            return this;
        }

        @Override public ChargedProjectiles build() {
            return new ChargedProjectilesValue(List.copyOf(this.projectiles));
        }
    }

    record ChargedProjectilesValue(List<ItemStack> projectiles) implements ChargedProjectiles {
    }

    // --- seeded container loot ---

    public static SeededContainerLoot.Builder seededContainerLoot(Key key) {
        return new SeededLootBuilder(key);
    }

    static final class SeededLootBuilder implements SeededContainerLoot.Builder {
        private Key lootTable;
        private long seed;

        SeededLootBuilder(Key lootTable) {
            this.lootTable = lootTable;
        }

        @Override public SeededContainerLoot.Builder lootTable(Key key) {
            this.lootTable = key;
            return this;
        }

        @Override public SeededContainerLoot.Builder seed(long seed) {
            this.seed = seed;
            return this;
        }

        @Override public SeededContainerLoot build() {
            return new SeededLootValue(this.lootTable, this.seed);
        }
    }

    record SeededLootValue(Key lootTable, long seed) implements SeededContainerLoot {
    }

    // --- простые значения ---

    public static MapId mapId(int id) {
        return new MapIdValue(id);
    }

    record MapIdValue(int id) implements MapId {
    }

    public static UseRemainder useRemainder(ItemStack stack) {
        return new UseRemainderValue(stack == null ? null : stack.clone());
    }

    record UseRemainderValue(ItemStack transformInto) implements UseRemainder {
    }

    public static OminousBottleAmplifier ominousBottleAmplifier(int amplifier) {
        return new OminousValue(amplifier);
    }

    record OminousValue(int amplifier) implements OminousBottleAmplifier {
    }

    public static Enchantable enchantable(int value) {
        return new EnchantableValue(value);
    }

    record EnchantableValue(int value) implements Enchantable {
    }

    public static DamageResistant damageResistant(RegistryKeySet<DamageType> types) {
        return new DamageResistantValue(types);
    }

    record DamageResistantValue(RegistryKeySet<DamageType> types) implements DamageResistant {
    }

    public static Repairable repairable(RegistryKeySet<ItemType> types) {
        return new RepairableValue(types);
    }

    record RepairableValue(RegistryKeySet<ItemType> types) implements Repairable {
    }

    // --- списки предметов ---

    public static BundleContents.Builder bundleContents() {
        return new BundleContentsBuilder();
    }

    static final class BundleContentsBuilder implements BundleContents.Builder {
        private final List<ItemStack> contents = new ArrayList<>();

        @Override public BundleContents.Builder add(ItemStack stack) {
            if (stack != null) {
                this.contents.add(stack.clone());
            }
            return this;
        }

        @Override public BundleContents.Builder addAll(List<ItemStack> stacks) {
            if (stacks != null) {
                for (ItemStack stack : stacks) {
                    add(stack);
                }
            }
            return this;
        }

        @Override public BundleContents build() {
            return new BundleContentsValue(List.copyOf(this.contents));
        }
    }

    record BundleContentsValue(List<ItemStack> contents) implements BundleContents {
    }

    public static ItemContainerContents.Builder itemContainerContents() {
        return new ContainerContentsBuilder();
    }

    static final class ContainerContentsBuilder implements ItemContainerContents.Builder {
        private final List<ItemStack> contents = new ArrayList<>();

        @Override public ItemContainerContents.Builder add(ItemStack stack) {
            if (stack != null) {
                this.contents.add(stack.clone());
            }
            return this;
        }

        @Override public ItemContainerContents.Builder addAll(List<ItemStack> stacks) {
            if (stacks != null) {
                for (ItemStack stack : stacks) {
                    add(stack);
                }
            }
            return this;
        }

        @Override public ItemContainerContents build() {
            return new ContainerContentsValue(List.copyOf(this.contents));
        }
    }

    record ContainerContentsValue(List<ItemStack> contents) implements ItemContainerContents {
    }

    public static SuspiciousStewEffects.Builder suspiciousStewEffects() {
        return new StewBuilder();
    }

    static final class StewBuilder implements SuspiciousStewEffects.Builder {
        private final List<SuspiciousEffectEntry> effects = new ArrayList<>();

        @Override public SuspiciousStewEffects.Builder add(SuspiciousEffectEntry entry) {
            if (entry != null) {
                this.effects.add(entry);
            }
            return this;
        }

        @Override public SuspiciousStewEffects.Builder addAll(
                java.util.Collection<SuspiciousEffectEntry> entries) {
            if (entries != null) {
                this.effects.addAll(entries);
            }
            return this;
        }

        @Override public SuspiciousStewEffects build() {
            return new StewValue(List.copyOf(this.effects));
        }
    }

    record StewValue(List<SuspiciousEffectEntry> effects) implements SuspiciousStewEffects {
    }

    public static BannerPatternLayers.Builder bannerPatternLayers() {
        return new BannerBuilder();
    }

    static final class BannerBuilder implements BannerPatternLayers.Builder {
        private final List<Pattern> patterns = new ArrayList<>();

        @Override public BannerPatternLayers.Builder add(Pattern pattern) {
            if (pattern != null) {
                this.patterns.add(pattern);
            }
            return this;
        }

        @Override public BannerPatternLayers.Builder addAll(List<Pattern> patterns) {
            if (patterns != null) {
                this.patterns.addAll(patterns);
            }
            return this;
        }

        @Override public BannerPatternLayers build() {
            return new BannerValue(List.copyOf(this.patterns));
        }
    }

    record BannerValue(List<Pattern> patterns) implements BannerPatternLayers {
    }

    // --- карты ---

    public static MapDecorations.Builder mapDecorations() {
        return new MapDecorationsBuilder();
    }

    static final class MapDecorationsBuilder implements MapDecorations.Builder {
        private final Map<String, MapDecorations.DecorationEntry> decorations = new LinkedHashMap<>();

        @Override public MapDecorations.Builder put(String id,
                MapDecorations.DecorationEntry entry) {
            if (id != null && entry != null) {
                this.decorations.put(id, entry);
            }
            return this;
        }

        @Override public MapDecorations.Builder putAll(
                Map<String, MapDecorations.DecorationEntry> entries) {
            if (entries != null) {
                this.decorations.putAll(entries);
            }
            return this;
        }

        @Override public MapDecorations build() {
            return new MapDecorationsValue(Map.copyOf(this.decorations));
        }
    }

    record MapDecorationsValue(Map<String, MapDecorations.DecorationEntry> decorations)
            implements MapDecorations {
        @Override public MapDecorations.DecorationEntry decoration(String id) {
            return this.decorations.get(id);
        }
    }

    public static MapDecorations.DecorationEntry mapDecorationEntry(MapCursor.Type type,
            double x, double z, float rotation) {
        return new DecorationEntryValue(type, x, z, rotation);
    }

    record DecorationEntryValue(MapCursor.Type type, double x, double z, float rotation)
            implements MapDecorations.DecorationEntry {
    }

    // --- блоки, музыка, броня, лодстон, фейерверки ---

    public static BlockItemDataProperties.Builder blockItemStateProperties() {
        return new BlockPropertiesBuilder();
    }

    static final class BlockPropertiesBuilder implements BlockItemDataProperties.Builder {
        @Override public BlockItemDataProperties build() {
            return new BlockPropertiesValue();
        }
    }

    record BlockPropertiesValue() implements BlockItemDataProperties {
        @Override public org.bukkit.block.data.BlockData createBlockData(
                org.bukkit.block.BlockType type) {
            return type == null ? null : type.createBlockData();
        }

        @Override public org.bukkit.block.data.BlockData applyTo(
                org.bukkit.block.data.BlockData data) {
            return data;
        }
    }

    public static JukeboxPlayable.Builder jukeboxPlayable(JukeboxSong song) {
        return new JukeboxBuilder(song);
    }

    static final class JukeboxBuilder implements JukeboxPlayable.Builder {
        private JukeboxSong song;

        JukeboxBuilder(JukeboxSong song) {
            this.song = song;
        }

        @Override public JukeboxPlayable.Builder jukeboxSong(JukeboxSong song) {
            this.song = song;
            return this;
        }

        @Override public JukeboxPlayable build() {
            return new JukeboxValue(this.song);
        }
    }

    record JukeboxValue(JukeboxSong jukeboxSong) implements JukeboxPlayable {
    }

    public static ItemArmorTrim.Builder itemArmorTrim(ArmorTrim trim) {
        return new ArmorTrimBuilder(trim);
    }

    static final class ArmorTrimBuilder implements ItemArmorTrim.Builder {
        private ArmorTrim trim;

        ArmorTrimBuilder(ArmorTrim trim) {
            this.trim = trim;
        }

        @Override public ItemArmorTrim.Builder armorTrim(ArmorTrim trim) {
            this.trim = trim;
            return this;
        }

        @Override public ItemArmorTrim build() {
            return new ArmorTrimValue(this.trim);
        }
    }

    record ArmorTrimValue(ArmorTrim armorTrim) implements ItemArmorTrim {
    }

    public static LodestoneTracker.Builder lodestoneTracker() {
        return new LodestoneBuilder();
    }

    static final class LodestoneBuilder implements LodestoneTracker.Builder {
        private org.bukkit.Location location;
        private boolean tracked;

        @Override public LodestoneTracker.Builder location(org.bukkit.Location location) {
            this.location = location;
            return this;
        }

        @Override public LodestoneTracker.Builder tracked(boolean tracked) {
            this.tracked = tracked;
            return this;
        }

        @Override public LodestoneTracker build() {
            return new LodestoneValue(this.location, this.tracked);
        }
    }

    record LodestoneValue(org.bukkit.Location location, boolean tracked)
            implements LodestoneTracker {
    }

    public static Fireworks.Builder fireworks() {
        return new FireworksBuilder();
    }

    static final class FireworksBuilder implements Fireworks.Builder {
        private final List<FireworkEffect> effects = new ArrayList<>();
        private int flightDuration;

        @Override public Fireworks.Builder flightDuration(int duration) {
            this.flightDuration = duration;
            return this;
        }

        @Override public Fireworks.Builder addEffect(FireworkEffect effect) {
            if (effect != null) {
                this.effects.add(effect);
            }
            return this;
        }

        @Override public Fireworks.Builder addEffects(List<FireworkEffect> effects) {
            if (effects != null) {
                this.effects.addAll(effects);
            }
            return this;
        }

        @Override public Fireworks build() {
            return new FireworksValue(List.copyOf(this.effects), this.flightDuration);
        }
    }

    record FireworksValue(List<FireworkEffect> effects, int flightDuration)
            implements Fireworks {
    }
}
