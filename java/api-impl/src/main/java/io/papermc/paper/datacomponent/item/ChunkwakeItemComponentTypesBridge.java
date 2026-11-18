package io.papermc.paper.datacomponent.item;

/**
 * Реализация package-private {@link ItemComponentTypesBridge} для Chunkwake.
 *
 * <p>Большинство строителей компонентов пока не реализованы (возвращают
 * пустое значение через {@code SoftValue}), но профиль игрока и скин-патч
 * работают по-настоящему: они нужны {@code Mannequin}, головам игроков и
 * компонентам {@code ItemStack}.
 */
public final class ChunkwakeItemComponentTypesBridge implements ItemComponentTypesBridge {
    public ChunkwakeItemComponentTypesBridge() {
    }

    @Override public io.papermc.paper.datacomponent.item.ChargedProjectiles.Builder chargedProjectiles() {
        return dev.chunkwake.bridge.ItemComponents.chargedProjectiles();
    }

    @Override public io.papermc.paper.datacomponent.item.PotDecorations.Builder potDecorations() {
        return dev.chunkwake.bridge.ItemComponents.potDecorations();
    }

    @Override public io.papermc.paper.datacomponent.item.ItemLore.Builder lore() {
        return dev.chunkwake.bridge.ItemComponents.lore();
    }

    @Override public io.papermc.paper.datacomponent.item.ItemEnchantments.Builder enchantments() {
        return dev.chunkwake.bridge.ItemComponents.enchantments();
    }

    @Override public io.papermc.paper.datacomponent.item.ItemAttributeModifiers.Builder modifiers() {
        return dev.chunkwake.bridge.ItemComponents2.itemAttributes();
    }

    @Override public io.papermc.paper.datacomponent.item.FoodProperties.Builder food() {
        return dev.chunkwake.bridge.ItemComponents2.food();
    }

    @Override public io.papermc.paper.datacomponent.item.DyedItemColor.Builder dyedItemColor() {
        return dev.chunkwake.bridge.ItemComponents.dyedItemColor();
    }

    @Override public io.papermc.paper.datacomponent.item.PotionContents.Builder potionContents() {
        return dev.chunkwake.bridge.ItemComponents2.potionContents();
    }

    @Override public io.papermc.paper.datacomponent.item.BundleContents.Builder bundleContents() {
        return dev.chunkwake.bridge.ItemComponents.bundleContents();
    }

    @Override public io.papermc.paper.datacomponent.item.SuspiciousStewEffects.Builder suspiciousStewEffects() {
        return dev.chunkwake.bridge.ItemComponents.suspiciousStewEffects();
    }

    @Override public io.papermc.paper.datacomponent.item.MapDecorations.Builder mapDecorations() {
        return dev.chunkwake.bridge.ItemComponents.mapDecorations();
    }

    @Override public io.papermc.paper.datacomponent.item.MapDecorations.DecorationEntry mapDecorationEntry(org.bukkit.map.MapCursor.Type p0, double p1, double p2, float p3) {
        return dev.chunkwake.bridge.ItemComponents.mapDecorationEntry(p0, p1, p2, p3);
    }

    @Override public io.papermc.paper.datacomponent.item.SeededContainerLoot.Builder seededContainerLoot(net.kyori.adventure.key.Key p0) {
        return dev.chunkwake.bridge.ItemComponents.seededContainerLoot(p0);
    }

    @Override public io.papermc.paper.datacomponent.item.WrittenBookContent.Builder writtenBookContent(io.papermc.paper.text.Filtered<java.lang.String> p0, java.lang.String p1) {
        return dev.chunkwake.bridge.ItemComponents2.writtenBookContent(p0, p1);
    }

    @Override public io.papermc.paper.datacomponent.item.WritableBookContent.Builder writeableBookContent() {
        return dev.chunkwake.bridge.ItemComponents2.writeableBookContent();
    }

    @Override public io.papermc.paper.datacomponent.item.ItemArmorTrim.Builder itemArmorTrim(org.bukkit.inventory.meta.trim.ArmorTrim p0) {
        return dev.chunkwake.bridge.ItemComponents.itemArmorTrim(p0);
    }

    @Override public io.papermc.paper.datacomponent.item.LodestoneTracker.Builder lodestoneTracker() {
        return dev.chunkwake.bridge.ItemComponents.lodestoneTracker();
    }

    @Override public io.papermc.paper.datacomponent.item.Fireworks.Builder fireworks() {
        return dev.chunkwake.bridge.ItemComponents.fireworks();
    }

    @Override public io.papermc.paper.datacomponent.item.ResolvableProfile resolvableProfile(com.destroystokyo.paper.profile.PlayerProfile p0) {
        return dev.chunkwake.bridge.Components.resolvableProfile(p0);
    }

    @Override public io.papermc.paper.datacomponent.item.ResolvableProfile.Builder resolvableProfile() {
        return dev.chunkwake.bridge.Components.resolvableProfile();
    }

    @Override public io.papermc.paper.datacomponent.item.ResolvableProfile.SkinPatchBuilder skinPatch() {
        return dev.chunkwake.bridge.Components.skinPatch();
    }

    @Override public io.papermc.paper.datacomponent.item.ResolvableProfile.SkinPatch emptySkinPatch() {
        return dev.chunkwake.bridge.Components.emptySkinPatch();
    }

    @Override public io.papermc.paper.datacomponent.item.BannerPatternLayers.Builder bannerPatternLayers() {
        return dev.chunkwake.bridge.ItemComponents.bannerPatternLayers();
    }

    @Override public io.papermc.paper.datacomponent.item.BlockItemDataProperties.Builder blockItemStateProperties() {
        return dev.chunkwake.bridge.ItemComponents.blockItemStateProperties();
    }

    @Override public io.papermc.paper.datacomponent.item.ItemContainerContents.Builder itemContainerContents() {
        return dev.chunkwake.bridge.ItemComponents.itemContainerContents();
    }

    @Override public io.papermc.paper.datacomponent.item.JukeboxPlayable.Builder jukeboxPlayable(org.bukkit.JukeboxSong p0) {
        return dev.chunkwake.bridge.ItemComponents.jukeboxPlayable(p0);
    }

    @Override public io.papermc.paper.datacomponent.item.Tool.Builder tool() {
        return dev.chunkwake.bridge.ItemComponents2.tool();
    }

    @Override public io.papermc.paper.datacomponent.item.Tool.Rule toolRule(io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.block.BlockType> p0, java.lang.Float p1, net.kyori.adventure.util.TriState p2) {
        return dev.chunkwake.bridge.ItemComponents2.toolRule(p0, p1, p2);
    }

    @Override public io.papermc.paper.datacomponent.item.ItemAdventurePredicate.Builder itemAdventurePredicate() {
        return dev.chunkwake.bridge.ItemComponents2.itemAdventurePredicate();
    }

    @Override public io.papermc.paper.datacomponent.item.CustomModelData.Builder customModelData() {
        return dev.chunkwake.bridge.ItemComponents.customModelData();
    }

    @Override public io.papermc.paper.datacomponent.item.MapId mapId(int p0) {
        return dev.chunkwake.bridge.ItemComponents.mapId(p0);
    }

    @Override public io.papermc.paper.datacomponent.item.UseRemainder useRemainder(org.bukkit.inventory.ItemStack p0) {
        return dev.chunkwake.bridge.ItemComponents.useRemainder(p0);
    }

    @Override public io.papermc.paper.datacomponent.item.Consumable.Builder consumable() {
        return dev.chunkwake.bridge.ItemComponents2.consumable();
    }

    @Override public io.papermc.paper.datacomponent.item.UseCooldown.Builder useCooldown(float p0) {
        return dev.chunkwake.bridge.ItemComponents2.useCooldown(p0);
    }

    @Override public io.papermc.paper.datacomponent.item.DamageResistant damageResistant(io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.damage.DamageType> p0) {
        return dev.chunkwake.bridge.ItemComponents.damageResistant(p0);
    }

    @Override public io.papermc.paper.datacomponent.item.Enchantable enchantable(int p0) {
        return dev.chunkwake.bridge.ItemComponents.enchantable(p0);
    }

    @Override public io.papermc.paper.datacomponent.item.Repairable repairable(io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.inventory.ItemType> p0) {
        return dev.chunkwake.bridge.ItemComponents.repairable(p0);
    }

    @Override public io.papermc.paper.datacomponent.item.Equippable.Builder equippable(org.bukkit.inventory.EquipmentSlot p0) {
        return dev.chunkwake.bridge.ItemComponents2.equippable(p0);
    }

    @Override public io.papermc.paper.datacomponent.item.DeathProtection.Builder deathProtection() {
        return dev.chunkwake.bridge.ItemComponents2.deathProtection();
    }

    @Override public io.papermc.paper.datacomponent.item.OminousBottleAmplifier ominousBottleAmplifier(int p0) {
        return dev.chunkwake.bridge.ItemComponents.ominousBottleAmplifier(p0);
    }

    @Override public io.papermc.paper.datacomponent.item.BlocksAttacks.Builder blocksAttacks() {
        return dev.chunkwake.bridge.ItemComponents2.blocksAttacks();
    }

    @Override public io.papermc.paper.datacomponent.item.TooltipDisplay.Builder tooltipDisplay() {
        return dev.chunkwake.bridge.ItemComponents.tooltipDisplay();
    }

    @Override public io.papermc.paper.datacomponent.item.Weapon.Builder weapon() {
        return dev.chunkwake.bridge.ItemComponents2.weapon();
    }

    @Override public io.papermc.paper.datacomponent.item.KineticWeapon.Builder kineticWeapon() {
        return dev.chunkwake.bridge.ItemComponents2.kineticWeapon();
    }

    @Override public io.papermc.paper.datacomponent.item.KineticWeapon.Condition kineticWeaponCondition(int p0, float p1, float p2) {
        return dev.chunkwake.bridge.ItemComponents2.kineticWeaponCondition(p0, p1, p2);
    }

    @Override public io.papermc.paper.datacomponent.item.UseEffects.Builder useEffects() {
        return dev.chunkwake.bridge.ItemComponents2.useEffects();
    }

    @Override public io.papermc.paper.datacomponent.item.PiercingWeapon.Builder piercingWeapon() {
        return dev.chunkwake.bridge.ItemComponents2.piercingWeapon();
    }

    @Override public io.papermc.paper.datacomponent.item.AttackRange.Builder attackRange() {
        return dev.chunkwake.bridge.ItemComponents2.attackRange();
    }

    @Override public io.papermc.paper.datacomponent.item.SwingAnimation.Builder swingAnimation() {
        return dev.chunkwake.bridge.ItemComponents2.swingAnimation();
    }

    @Override public io.papermc.paper.datacomponent.item.SulfurCubeContent sulfurCubeContent(org.bukkit.inventory.ItemStack p0) {
        return dev.chunkwake.bridge.ItemComponents2.sulfurCubeContent(p0);
    }

    @Override public io.papermc.paper.datacomponent.item.MobVisibility mobVisibility(io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.entity.EntityType> p0, float p1) {
        return dev.chunkwake.bridge.ItemComponents2.mobVisibility(p0, p1);
    }

    @Override public io.papermc.paper.datacomponent.item.VillagerFood villagerFood(int p0) {
        return dev.chunkwake.bridge.ItemComponents2.villagerFood(p0);
    }

    @Override public io.papermc.paper.datacomponent.item.SignText.Builder signText() {
        return dev.chunkwake.bridge.ItemComponents2.signText();
    }
}
