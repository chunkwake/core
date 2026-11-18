package dev.chunkwake.bridge;

import io.papermc.paper.datacomponent.item.AttackRange;
import io.papermc.paper.datacomponent.item.BlocksAttacks;
import io.papermc.paper.datacomponent.item.Consumable;
import io.papermc.paper.datacomponent.item.DeathProtection;
import io.papermc.paper.datacomponent.item.Equippable;
import io.papermc.paper.datacomponent.item.FoodProperties;
import io.papermc.paper.datacomponent.item.ItemAdventurePredicate;
import io.papermc.paper.datacomponent.item.ItemAttributeModifiers;
import io.papermc.paper.datacomponent.item.KineticWeapon;
import io.papermc.paper.datacomponent.item.MobVisibility;
import io.papermc.paper.datacomponent.item.PiercingWeapon;
import io.papermc.paper.datacomponent.item.PotionContents;
import io.papermc.paper.datacomponent.item.SignText;
import io.papermc.paper.datacomponent.item.SulfurCubeContent;
import io.papermc.paper.datacomponent.item.SwingAnimation;
import io.papermc.paper.datacomponent.item.Tool;
import io.papermc.paper.datacomponent.item.UseCooldown;
import io.papermc.paper.datacomponent.item.UseEffects;
import io.papermc.paper.datacomponent.item.VillagerFood;
import io.papermc.paper.datacomponent.item.Weapon;
import io.papermc.paper.datacomponent.item.WritableBookContent;
import io.papermc.paper.datacomponent.item.WrittenBookContent;
import io.papermc.paper.datacomponent.item.blocksattacks.DamageReduction;
import io.papermc.paper.datacomponent.item.blocksattacks.ItemDamageFunction;
import io.papermc.paper.datacomponent.item.consumable.ConsumeEffect;
import io.papermc.paper.datacomponent.item.consumable.ItemUseAnimation;
import io.papermc.paper.datacomponent.item.attribute.AttributeModifierDisplay;
import io.papermc.paper.block.BlockPredicate;
import io.papermc.paper.registry.set.RegistryKeySet;
import io.papermc.paper.text.Filtered;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import net.kyori.adventure.inventory.Book;
import net.kyori.adventure.key.Key;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.ComponentLike;
import net.kyori.adventure.util.TriState;
import org.bukkit.Color;
import org.bukkit.DyeColor;
import org.bukkit.attribute.Attribute;
import org.bukkit.attribute.AttributeModifier;
import org.bukkit.block.BlockType;
import org.bukkit.damage.DamageType;
import org.bukkit.entity.EntityType;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.EquipmentSlotGroup;
import org.bukkit.inventory.ItemStack;
import org.bukkit.potion.PotionEffect;
import org.bukkit.potion.PotionType;

/** Редкие/сложные item-компоненты: реальные данные вместо SoftValue. */
public final class ItemComponents2 {
    private ItemComponents2() {
    }

    // --- food / consumable ---

    public static FoodProperties.Builder food() {
        return new FoodBuilder();
    }

    static final class FoodBuilder implements FoodProperties.Builder {
        private int nutrition;
        private float saturation;
        private boolean alwaysEat;

        @Override public FoodProperties.Builder nutrition(int value) {
            this.nutrition = value;
            return this;
        }

        @Override public FoodProperties.Builder saturation(float value) {
            this.saturation = value;
            return this;
        }

        @Override public FoodProperties.Builder canAlwaysEat(boolean value) {
            this.alwaysEat = value;
            return this;
        }

        @Override public FoodProperties build() {
            return new FoodValue(this.nutrition, this.saturation, this.alwaysEat);
        }
    }

    record FoodValue(int nutrition, float saturation, boolean canAlwaysEat)
            implements FoodProperties {
        @Override public FoodProperties.Builder toBuilder() {
            FoodBuilder builder = new FoodBuilder();
            builder.nutrition = this.nutrition;
            builder.saturation = this.saturation;
            builder.alwaysEat = this.canAlwaysEat;
            return builder;
        }
    }

    public static Consumable.Builder consumable() {
        return new ConsumableBuilder();
    }

    static final class ConsumableBuilder implements Consumable.Builder {
        private float seconds = 1.6f;
        private ItemUseAnimation animation = ItemUseAnimation.EAT;
        private Key sound;
        private boolean particles = true;
        private final List<ConsumeEffect> effects = new ArrayList<>();

        @Override public Consumable.Builder consumeSeconds(float value) {
            this.seconds = value;
            return this;
        }

        @Override public Consumable.Builder animation(ItemUseAnimation animation) {
            this.animation = animation;
            return this;
        }

        @Override public Consumable.Builder sound(Key sound) {
            this.sound = sound;
            return this;
        }

        @Override public Consumable.Builder hasConsumeParticles(boolean value) {
            this.particles = value;
            return this;
        }

        @Override public Consumable.Builder effects(List<ConsumeEffect> effects) {
            this.effects.clear();
            if (effects != null) {
                this.effects.addAll(effects);
            }
            return this;
        }

        @Override public Consumable.Builder addEffect(ConsumeEffect effect) {
            if (effect != null) {
                this.effects.add(effect);
            }
            return this;
        }

        @Override public Consumable.Builder addEffects(List<ConsumeEffect> effects) {
            if (effects != null) {
                this.effects.addAll(effects);
            }
            return this;
        }

        @Override public Consumable build() {
            return new ConsumableValue(this.seconds, this.animation, this.sound,
                    this.particles, List.copyOf(this.effects));
        }
    }

    record ConsumableValue(float consumeSeconds, ItemUseAnimation animation, Key sound,
            boolean hasConsumeParticles, List<ConsumeEffect> consumeEffects)
            implements Consumable {
        @Override public Consumable.Builder toBuilder() {
            ConsumableBuilder builder = new ConsumableBuilder();
            builder.seconds = this.consumeSeconds;
            builder.animation = this.animation;
            builder.sound = this.sound;
            builder.particles = this.hasConsumeParticles;
            builder.effects.addAll(this.consumeEffects);
            return builder;
        }
    }

    // --- tool ---

    public static Tool.Builder tool() {
        return new ToolBuilder();
    }

    static final class ToolBuilder implements Tool.Builder {
        private int damagePerBlock = 1;
        private float defaultMiningSpeed = 1.0f;
        private boolean canDestroyInCreative = true;
        private final List<Tool.Rule> rules = new ArrayList<>();

        @Override public Tool.Builder damagePerBlock(int value) {
            this.damagePerBlock = value;
            return this;
        }

        @Override public Tool.Builder defaultMiningSpeed(float value) {
            this.defaultMiningSpeed = value;
            return this;
        }

        @Override public Tool.Builder addRule(Tool.Rule rule) {
            if (rule != null) {
                this.rules.add(rule);
            }
            return this;
        }

        @Override public Tool.Builder canDestroyBlocksInCreative(boolean value) {
            this.canDestroyInCreative = value;
            return this;
        }

        @Override public Tool.Builder addRules(java.util.Collection<Tool.Rule> rules) {
            if (rules != null) {
                this.rules.addAll(rules);
            }
            return this;
        }

        @Override public Tool build() {
            return new ToolValue(this.defaultMiningSpeed, this.damagePerBlock,
                    List.copyOf(this.rules), this.canDestroyInCreative);
        }
    }

    record ToolValue(float defaultMiningSpeed, int damagePerBlock, List<Tool.Rule> rules,
            boolean canDestroyBlocksInCreative) implements Tool {
    }

    public static Tool.Rule toolRule(RegistryKeySet<BlockType> blocks, Float speed,
            TriState correct) {
        return new ToolRuleValue(blocks, speed, correct);
    }

    record ToolRuleValue(RegistryKeySet<BlockType> blocks, Float speed, TriState correctForDrops)
            implements Tool.Rule {
    }

    // --- equippable ---

    public static Equippable.Builder equippable(EquipmentSlot slot) {
        return new EquippableBuilder(slot);
    }

    static final class EquippableBuilder implements Equippable.Builder {
        private final EquipmentSlot slot;
        private Key equipSound;
        private Key assetId;
        private Key cameraOverlay;
        private RegistryKeySet<EntityType> allowedEntities;
        private boolean dispensable = true;
        private boolean swappable = true;
        private boolean damageOnHurt = true;
        private boolean equipOnInteract;
        private boolean canBeSheared;
        private Key shearSound;

        EquippableBuilder(EquipmentSlot slot) {
            this.slot = slot;
        }

        @Override public Equippable.Builder equipSound(Key key) {
            this.equipSound = key;
            return this;
        }
        @Override public Equippable.Builder assetId(Key key) {
            this.assetId = key;
            return this;
        }
        @Override public Equippable.Builder cameraOverlay(Key key) {
            this.cameraOverlay = key;
            return this;
        }
        @Override public Equippable.Builder allowedEntities(RegistryKeySet<EntityType> set) {
            this.allowedEntities = set;
            return this;
        }
        @Override public Equippable.Builder dispensable(boolean value) {
            this.dispensable = value;
            return this;
        }
        @Override public Equippable.Builder swappable(boolean value) {
            this.swappable = value;
            return this;
        }
        @Override public Equippable.Builder damageOnHurt(boolean value) {
            this.damageOnHurt = value;
            return this;
        }
        @Override public Equippable.Builder equipOnInteract(boolean value) {
            this.equipOnInteract = value;
            return this;
        }
        @Override public Equippable.Builder canBeSheared(boolean value) {
            this.canBeSheared = value;
            return this;
        }
        @Override public Equippable.Builder shearSound(Key key) {
            this.shearSound = key;
            return this;
        }

        @Override public Equippable build() {
            return new EquippableValue(this.slot, this.equipSound, this.assetId,
                    this.cameraOverlay, this.allowedEntities, this.dispensable, this.swappable,
                    this.damageOnHurt, this.equipOnInteract, this.canBeSheared, this.shearSound);
        }
    }

    record EquippableValue(EquipmentSlot slot, Key equipSound, Key assetId, Key cameraOverlay,
            RegistryKeySet<EntityType> allowedEntities, boolean dispensable, boolean swappable,
            boolean damageOnHurt, boolean equipOnInteract, boolean canBeSheared, Key shearSound)
            implements Equippable {
        @Override public Equippable.Builder toBuilder() {
            EquippableBuilder builder = new EquippableBuilder(this.slot);
            builder.equipSound = this.equipSound;
            builder.assetId = this.assetId;
            builder.cameraOverlay = this.cameraOverlay;
            builder.allowedEntities = this.allowedEntities;
            builder.dispensable = this.dispensable;
            builder.swappable = this.swappable;
            builder.damageOnHurt = this.damageOnHurt;
            builder.equipOnInteract = this.equipOnInteract;
            builder.canBeSheared = this.canBeSheared;
            builder.shearSound = this.shearSound;
            return builder;
        }
    }

    // --- weapons ---

    public static Weapon.Builder weapon() {
        return new WeaponBuilder();
    }

    static final class WeaponBuilder implements Weapon.Builder {
        private int damagePerAttack;
        private float disableBlocking;

        @Override public Weapon.Builder itemDamagePerAttack(int value) {
            this.damagePerAttack = value;
            return this;
        }

        @Override public Weapon.Builder disableBlockingForSeconds(float value) {
            this.disableBlocking = value;
            return this;
        }

        @Override public Weapon build() {
            return new WeaponValue(this.damagePerAttack, this.disableBlocking);
        }
    }

    record WeaponValue(int itemDamagePerAttack, float disableBlockingForSeconds)
            implements Weapon {
    }

    public static KineticWeapon.Builder kineticWeapon() {
        return new KineticBuilder();
    }

    static final class KineticBuilder implements KineticWeapon.Builder {
        private int contactCooldown;
        private int delay;
        private KineticWeapon.Condition dismount;
        private KineticWeapon.Condition knockback;
        private KineticWeapon.Condition damage;
        private float forwardMovement;
        private float damageMultiplier;
        private Key sound;
        private Key hitSound;

        @Override public KineticWeapon.Builder contactCooldownTicks(int value) {
            this.contactCooldown = value;
            return this;
        }
        @Override public KineticWeapon.Builder delayTicks(int value) {
            this.delay = value;
            return this;
        }
        @Override public KineticWeapon.Builder dismountConditions(KineticWeapon.Condition value) {
            this.dismount = value;
            return this;
        }
        @Override public KineticWeapon.Builder knockbackConditions(KineticWeapon.Condition value) {
            this.knockback = value;
            return this;
        }
        @Override public KineticWeapon.Builder damageConditions(KineticWeapon.Condition value) {
            this.damage = value;
            return this;
        }
        @Override public KineticWeapon.Builder forwardMovement(float value) {
            this.forwardMovement = value;
            return this;
        }
        @Override public KineticWeapon.Builder damageMultiplier(float value) {
            this.damageMultiplier = value;
            return this;
        }
        @Override public KineticWeapon.Builder sound(Key key) {
            this.sound = key;
            return this;
        }
        @Override public KineticWeapon.Builder hitSound(Key key) {
            this.hitSound = key;
            return this;
        }

        @Override public KineticWeapon build() {
            return new KineticValue(this.contactCooldown, this.delay, this.dismount,
                    this.knockback, this.damage, this.forwardMovement, this.damageMultiplier,
                    this.sound, this.hitSound);
        }
    }

    record KineticValue(int contactCooldownTicks, int delayTicks,
            KineticWeapon.Condition dismountConditions,
            KineticWeapon.Condition knockbackConditions, KineticWeapon.Condition damageConditions,
            float forwardMovement, float damageMultiplier, Key sound, Key hitSound)
            implements KineticWeapon {
    }

    public static KineticWeapon.Condition kineticWeaponCondition(int maxDuration, float minSpeed,
            float minRelativeSpeed) {
        return new KineticConditionValue(maxDuration, minSpeed, minRelativeSpeed);
    }

    record KineticConditionValue(int maxDurationTicks, float minSpeed, float minRelativeSpeed)
            implements KineticWeapon.Condition {
    }

    public static PiercingWeapon.Builder piercingWeapon() {
        return new PiercingBuilder();
    }

    static final class PiercingBuilder implements PiercingWeapon.Builder {
        private boolean knockback;
        private boolean dismounts;
        private Key sound;
        private Key hitSound;

        @Override public PiercingWeapon.Builder dealsKnockback(boolean value) {
            this.knockback = value;
            return this;
        }
        @Override public PiercingWeapon.Builder dismounts(boolean value) {
            this.dismounts = value;
            return this;
        }
        @Override public PiercingWeapon.Builder sound(Key key) {
            this.sound = key;
            return this;
        }
        @Override public PiercingWeapon.Builder hitSound(Key key) {
            this.hitSound = key;
            return this;
        }
        @Override public PiercingWeapon build() {
            return new PiercingValue(this.knockback, this.dismounts, this.sound, this.hitSound);
        }
    }

    record PiercingValue(boolean dealsKnockback, boolean dismounts, Key sound, Key hitSound)
            implements PiercingWeapon {
    }

    public static UseEffects.Builder useEffects() {
        return new UseEffectsBuilder();
    }

    static final class UseEffectsBuilder implements UseEffects.Builder {
        private boolean canSprint;
        private boolean vibrations;
        private float speed = 0.2f;

        @Override public UseEffects.Builder canSprint(boolean value) {
            this.canSprint = value;
            return this;
        }
        @Override public UseEffects.Builder interactVibrations(boolean value) {
            this.vibrations = value;
            return this;
        }
        @Override public UseEffects.Builder speedMultiplier(float value) {
            this.speed = value;
            return this;
        }
        @Override public UseEffects build() {
            return new UseEffectsValue(this.canSprint, this.vibrations, this.speed);
        }
    }

    record UseEffectsValue(boolean canSprint, boolean interactVibrations, float speedMultiplier)
            implements UseEffects {
    }

    public static AttackRange.Builder attackRange() {
        return new AttackRangeBuilder();
    }

    static final class AttackRangeBuilder implements AttackRange.Builder {
        private float minReach;
        private float maxReach;
        private float minCreative;
        private float maxCreative;
        private float hitboxMargin;
        private float mobFactor = 1.0f;

        @Override public AttackRange.Builder minReach(float value) {
            this.minReach = value;
            return this;
        }
        @Override public AttackRange.Builder maxReach(float value) {
            this.maxReach = value;
            return this;
        }
        @Override public AttackRange.Builder minCreativeReach(float value) {
            this.minCreative = value;
            return this;
        }
        @Override public AttackRange.Builder maxCreativeReach(float value) {
            this.maxCreative = value;
            return this;
        }
        @Override public AttackRange.Builder hitboxMargin(float value) {
            this.hitboxMargin = value;
            return this;
        }
        @Override public AttackRange.Builder mobFactor(float value) {
            this.mobFactor = value;
            return this;
        }
        @Override public AttackRange build() {
            return new AttackRangeValue(this.minReach, this.maxReach, this.minCreative,
                    this.maxCreative, this.hitboxMargin, this.mobFactor);
        }
    }

    record AttackRangeValue(float minReach, float maxReach, float minCreativeReach,
            float maxCreativeReach, float hitboxMargin, float mobFactor) implements AttackRange {
    }

    public static SwingAnimation.Builder swingAnimation() {
        return new SwingBuilder();
    }

    static final class SwingBuilder implements SwingAnimation.Builder {
        private SwingAnimation.Animation type;
        private int duration;

        @Override public SwingAnimation.Builder type(SwingAnimation.Animation type) {
            this.type = type;
            return this;
        }
        @Override public SwingAnimation.Builder duration(int duration) {
            this.duration = duration;
            return this;
        }
        @Override public SwingAnimation build() {
            return new SwingValue(this.type, this.duration);
        }
    }

    record SwingValue(SwingAnimation.Animation type, int duration) implements SwingAnimation {
    }

    // --- прочее ---

    public static ItemAdventurePredicate.Builder itemAdventurePredicate() {
        return new AdventureBuilder();
    }

    static final class AdventureBuilder implements ItemAdventurePredicate.Builder {
        private final List<BlockPredicate> predicates = new ArrayList<>();

        @Override public ItemAdventurePredicate.Builder addPredicate(BlockPredicate predicate) {
            if (predicate != null) {
                this.predicates.add(predicate);
            }
            return this;
        }
        @Override public ItemAdventurePredicate.Builder addPredicates(List<BlockPredicate> list) {
            if (list != null) {
                this.predicates.addAll(list);
            }
            return this;
        }
        @Override public ItemAdventurePredicate build() {
            return new AdventureValue(List.copyOf(this.predicates));
        }
    }

    record AdventureValue(List<BlockPredicate> predicates) implements ItemAdventurePredicate {
    }

    public static DeathProtection.Builder deathProtection() {
        return new DeathBuilder();
    }

    static final class DeathBuilder implements DeathProtection.Builder {
        private final List<ConsumeEffect> effects = new ArrayList<>();

        @Override public DeathProtection.Builder addEffect(ConsumeEffect effect) {
            if (effect != null) {
                this.effects.add(effect);
            }
            return this;
        }
        @Override public DeathProtection.Builder addEffects(List<ConsumeEffect> list) {
            if (list != null) {
                this.effects.addAll(list);
            }
            return this;
        }
        @Override public DeathProtection build() {
            return new DeathValue(List.copyOf(this.effects));
        }
    }

    record DeathValue(List<ConsumeEffect> deathEffects) implements DeathProtection {
    }

    public static BlocksAttacks.Builder blocksAttacks() {
        return new BlocksBuilder();
    }

    static final class BlocksBuilder implements BlocksAttacks.Builder {
        private float delay;
        private float cooldownScale = 1.0f;
        private final List<DamageReduction> reductions = new ArrayList<>();
        private ItemDamageFunction itemDamage;
        private RegistryKeySet<DamageType> bypassed;
        private Key blockSound;
        private Key disableSound;

        @Override public BlocksAttacks.Builder blockDelaySeconds(float value) {
            this.delay = value;
            return this;
        }
        @Override public BlocksAttacks.Builder disableCooldownScale(float value) {
            this.cooldownScale = value;
            return this;
        }
        @Override public BlocksAttacks.Builder addDamageReduction(DamageReduction reduction) {
            if (reduction != null) {
                this.reductions.add(reduction);
            }
            return this;
        }
        @Override public BlocksAttacks.Builder damageReductions(List<DamageReduction> list) {
            this.reductions.clear();
            if (list != null) {
                this.reductions.addAll(list);
            }
            return this;
        }
        @Override public BlocksAttacks.Builder itemDamage(ItemDamageFunction function) {
            this.itemDamage = function;
            return this;
        }
        @Override public BlocksAttacks.Builder bypassedBy(RegistryKeySet<DamageType> set) {
            this.bypassed = set;
            return this;
        }
        @Override public BlocksAttacks.Builder blockSound(Key key) {
            this.blockSound = key;
            return this;
        }
        @Override public BlocksAttacks.Builder disableSound(Key key) {
            this.disableSound = key;
            return this;
        }
        @Override public BlocksAttacks build() {
            return new BlocksValue(this.delay, this.cooldownScale,
                    List.copyOf(this.reductions), this.itemDamage, this.bypassed,
                    this.blockSound, this.disableSound);
        }
    }

    record BlocksValue(float blockDelaySeconds, float disableCooldownScale,
            List<DamageReduction> damageReductions, ItemDamageFunction itemDamage,
            RegistryKeySet<DamageType> bypassedBy, Key blockSound, Key disableSound)
            implements BlocksAttacks {
    }

    public static DamageReduction.Builder damageReduction() {
        return new DamageReductionBuilder();
    }

    static final class DamageReductionBuilder implements DamageReduction.Builder {
        private RegistryKeySet<DamageType> type;
        private float angle = 90.0f;
        private float base;
        private float factor;

        @Override public DamageReduction.Builder type(RegistryKeySet<DamageType> set) {
            this.type = set;
            return this;
        }
        @Override public DamageReduction.Builder horizontalBlockingAngle(float value) {
            this.angle = value;
            return this;
        }
        @Override public DamageReduction.Builder base(float value) {
            this.base = value;
            return this;
        }
        @Override public DamageReduction.Builder factor(float value) {
            this.factor = value;
            return this;
        }
        @Override public DamageReduction build() {
            return new DamageReductionValue(this.type, this.angle, this.base, this.factor);
        }
    }

    record DamageReductionValue(RegistryKeySet<DamageType> type, float horizontalBlockingAngle,
            float base, float factor) implements DamageReduction {
    }

    public static ItemDamageFunction.Builder itemDamageFunction() {
        return new ItemDamageBuilder();
    }

    static final class ItemDamageBuilder implements ItemDamageFunction.Builder {
        private float threshold;
        private float base;
        private float factor;

        @Override public ItemDamageFunction.Builder threshold(float value) {
            this.threshold = value;
            return this;
        }
        @Override public ItemDamageFunction.Builder base(float value) {
            this.base = value;
            return this;
        }
        @Override public ItemDamageFunction.Builder factor(float value) {
            this.factor = value;
            return this;
        }
        @Override public ItemDamageFunction build() {
            return new ItemDamageValue(this.threshold, this.base, this.factor);
        }
    }

    record ItemDamageValue(float threshold, float base, float factor)
            implements ItemDamageFunction {
        @Override public int damageToApply(float damage) {
            if (damage < this.threshold) {
                return 0;
            }
            return Math.max(1, (int) Math.floor(this.base + damage * this.factor));
        }
    }

    public static PotionContents.Builder potionContents() {
        return new PotionBuilder();
    }

    static final class PotionBuilder implements PotionContents.Builder {
        private PotionType potion;
        private Color color;
        private String name;
        private final List<PotionEffect> effects = new ArrayList<>();

        @Override public PotionContents.Builder potion(PotionType potion) {
            this.potion = potion;
            return this;
        }
        @Override public PotionContents.Builder customColor(Color color) {
            this.color = color;
            return this;
        }
        @Override public PotionContents.Builder customName(String name) {
            this.name = name;
            return this;
        }
        @Override public PotionContents.Builder addCustomEffect(PotionEffect effect) {
            if (effect != null) {
                this.effects.add(effect);
            }
            return this;
        }
        @Override public PotionContents.Builder addCustomEffects(List<PotionEffect> list) {
            if (list != null) {
                this.effects.addAll(list);
            }
            return this;
        }
        @Override public PotionContents build() {
            return new PotionValue(this.potion, this.color, List.copyOf(this.effects), this.name);
        }
    }

    record PotionValue(PotionType potion, Color customColor, List<PotionEffect> customEffects,
            String customName) implements PotionContents {
        @Override public List<PotionEffect> allEffects() {
            List<PotionEffect> all = new ArrayList<>();
            if (this.potion != null) {
                all.addAll(this.potion.getPotionEffects());
            }
            all.addAll(this.customEffects);
            return all;
        }

        @Override public Color computeEffectiveColor() {
            return this.customColor != null ? this.customColor : Color.WHITE;
        }
    }

    public static UseCooldown.Builder useCooldown(float seconds) {
        return new CooldownBuilder(seconds);
    }

    static final class CooldownBuilder implements UseCooldown.Builder {
        private final float seconds;
        private Key group;

        CooldownBuilder(float seconds) {
            this.seconds = seconds;
        }

        @Override public UseCooldown.Builder cooldownGroup(Key group) {
            this.group = group;
            return this;
        }

        @Override public UseCooldown build() {
            return new CooldownValue(this.seconds, this.group);
        }
    }

    record CooldownValue(float seconds, Key cooldownGroup) implements UseCooldown {
    }

    public static SulfurCubeContent sulfurCubeContent(ItemStack stack) {
        return new SulfurValue(stack == null ? null : stack.clone());
    }

    record SulfurValue(ItemStack absorbedItem) implements SulfurCubeContent {
    }

    public static MobVisibility mobVisibility(RegistryKeySet<EntityType> types, float visibility) {
        return new MobVisibilityValue(types, visibility);
    }

    record MobVisibilityValue(RegistryKeySet<EntityType> targetingEntityTypes, float visibility)
            implements MobVisibility {
    }

    public static VillagerFood villagerFood(int nutrition) {
        return new VillagerFoodValue(nutrition);
    }

    record VillagerFoodValue(int nutrition) implements VillagerFood {
    }

    // --- sign text ---

    public static SignText.Builder signText() {
        return new SignTextBuilder();
    }

    static final class SignTextBuilder implements SignText.Builder {
        private final List<Component> lines = new ArrayList<>();
        private DyeColor color;
        private boolean glowing;

        @Override public SignText.Builder line(int index, ComponentLike line) {
            while (this.lines.size() <= index) {
                this.lines.add(Component.empty());
            }
            this.lines.set(index, line.asComponent());
            return this;
        }

        @Override public SignText.Builder lines(List<? extends ComponentLike> list) {
            this.lines.clear();
            if (list != null) {
                for (ComponentLike line : list) {
                    this.lines.add(line.asComponent());
                }
            }
            return this;
        }
        @Override public SignText.Builder color(DyeColor color) {
            this.color = color;
            return this;
        }
        @Override public SignText.Builder hasGlowingText(boolean glowing) {
            this.glowing = glowing;
            return this;
        }
        @Override public SignText build() {
            return new SignTextValue(List.copyOf(this.lines), this.color, this.glowing);
        }
    }

    record SignTextValue(List<Component> lines, DyeColor color, boolean hasGlowingText)
            implements SignText {
        @Override public SignText.Builder toBuilder() {
            SignTextBuilder builder = new SignTextBuilder();
            builder.lines.addAll(this.lines);
            builder.color = this.color;
            builder.glowing = this.hasGlowingText;
            return builder;
        }
    }

    // --- attribute modifiers ---

    public static ItemAttributeModifiers.Builder itemAttributes() {
        return new AttributeBuilder();
    }

    static final class AttributeBuilder implements ItemAttributeModifiers.Builder {
        private final List<ItemAttributeModifiers.Entry> entries = new ArrayList<>();

        @Override public ItemAttributeModifiers.Builder addModifier(Attribute attribute,
                AttributeModifier modifier, EquipmentSlotGroup group,
                AttributeModifierDisplay display) {
            if (attribute != null && modifier != null) {
                this.entries.add(new AttributeEntryValue(attribute, modifier, group, display));
            }
            return this;
        }

        @Override public ItemAttributeModifiers build() {
            return new AttributeValue(List.copyOf(this.entries));
        }
    }

    record AttributeValue(List<ItemAttributeModifiers.Entry> modifiers)
            implements ItemAttributeModifiers {
    }

    record AttributeEntryValue(Attribute attribute, AttributeModifier modifier,
            EquipmentSlotGroup group, AttributeModifierDisplay display)
            implements ItemAttributeModifiers.Entry {
        @Override public EquipmentSlotGroup getGroup() {
            return this.group;
        }
    }

    // --- книги ---

    public static WrittenBookContent.Builder writtenBookContent(Filtered<String> title,
            String author) {
        return new WrittenBookBuilder(title, author);
    }

    static final class WrittenBookBuilder implements WrittenBookContent.Builder {
        private Filtered<String> title;
        private String author;
        private int generation;
        private boolean resolved = true;
        private final List<Filtered<Component>> pages = new ArrayList<>();

        WrittenBookBuilder(Filtered<String> title, String author) {
            this.title = title;
            this.author = author;
        }

        @Override public WrittenBookContent.Builder title(String title) {
            this.title = Filtered.of(title, title);
            return this;
        }
        @Override public WrittenBookContent.Builder filteredTitle(Filtered<String> title) {
            this.title = title;
            return this;
        }
        @Override public WrittenBookContent.Builder author(String author) {
            this.author = author;
            return this;
        }
        @Override public WrittenBookContent.Builder generation(int generation) {
            this.generation = generation;
            return this;
        }
        @Override public WrittenBookContent.Builder resolved(boolean resolved) {
            this.resolved = resolved;
            return this;
        }
        @Override public WrittenBookContent.Builder addPage(ComponentLike page) {
            return addFilteredPage(Filtered.of(page.asComponent(), page.asComponent()));
        }
        @Override public WrittenBookContent.Builder addPages(
                List<? extends ComponentLike> pages) {
            if (pages != null) {
                for (ComponentLike page : pages) {
                    addPage(page);
                }
            }
            return this;
        }
        @Override public WrittenBookContent.Builder addFilteredPage(
                Filtered<? extends ComponentLike> page) {
            if (page != null) {
                this.pages.add(Filtered.of(page.raw().asComponent(),
                        page.filtered() == null ? page.raw().asComponent()
                                : page.filtered().asComponent()));
            }
            return this;
        }
        @Override public WrittenBookContent.Builder addFilteredPages(
                List<Filtered<? extends ComponentLike>> pages) {
            if (pages != null) {
                for (Filtered<? extends ComponentLike> page : pages) {
                    addFilteredPage(page);
                }
            }
            return this;
        }
        @Override public WrittenBookContent build() {
            return new WrittenBookValue(this.title, this.author, this.generation,
                    List.copyOf(this.pages), this.resolved);
        }
    }

    record WrittenBookValue(Filtered<String> title, String author, int generation,
            List<Filtered<Component>> pages, boolean resolved) implements WrittenBookContent {
        @Override public Book asBook() {
            Component bookTitle = Component.text(this.title == null ? "" : this.title.raw());
            Component bookAuthor = Component.text(this.author == null ? "" : this.author);
            List<Component> rendered = new ArrayList<>();
            for (Filtered<Component> page : this.pages) {
                rendered.add(page.raw());
            }
            return Book.book(bookTitle, bookAuthor, rendered);
        }
    }

    public static WritableBookContent.Builder writeableBookContent() {
        return new WritableBookBuilder();
    }

    static final class WritableBookBuilder implements WritableBookContent.Builder {
        private final List<Filtered<String>> pages = new ArrayList<>();

        @Override public WritableBookContent.Builder addPage(String page) {
            return addFilteredPage(Filtered.of(page, page));
        }
        @Override public WritableBookContent.Builder addPages(List<String> pages) {
            if (pages != null) {
                for (String page : pages) {
                    addPage(page);
                }
            }
            return this;
        }
        @Override public WritableBookContent.Builder addFilteredPage(Filtered<String> page) {
            if (page != null) {
                this.pages.add(page);
            }
            return this;
        }
        @Override public WritableBookContent.Builder addFilteredPages(List<Filtered<String>> list) {
            if (list != null) {
                this.pages.addAll(list);
            }
            return this;
        }
        @Override public WritableBookContent build() {
            return new WritableBookValue(List.copyOf(this.pages));
        }
    }

    record WritableBookValue(List<Filtered<String>> pages) implements WritableBookContent {
        @Override public Book asBook() {
            List<Component> rendered = new ArrayList<>();
            for (Filtered<String> page : this.pages) {
                rendered.add(Component.text(page.raw()));
            }
            return Book.book(Component.text(""), Component.text(""), rendered);
        }
    }
}
