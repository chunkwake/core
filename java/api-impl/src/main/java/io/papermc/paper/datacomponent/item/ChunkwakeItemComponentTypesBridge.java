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
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.chargedProjectiles");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.PotDecorations.Builder potDecorations() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.potDecorations");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.ItemLore.Builder lore() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.lore");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.ItemEnchantments.Builder enchantments() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.enchantments");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.ItemAttributeModifiers.Builder modifiers() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.modifiers");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.FoodProperties.Builder food() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.food");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.DyedItemColor.Builder dyedItemColor() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.dyedItemColor");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.PotionContents.Builder potionContents() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.potionContents");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.BundleContents.Builder bundleContents() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.bundleContents");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.SuspiciousStewEffects.Builder suspiciousStewEffects() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.suspiciousStewEffects");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.MapDecorations.Builder mapDecorations() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.mapDecorations");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.MapDecorations.DecorationEntry mapDecorationEntry(org.bukkit.map.MapCursor.Type p0, double p1, double p2, float p3) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.mapDecorationEntry");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.SeededContainerLoot.Builder seededContainerLoot(net.kyori.adventure.key.Key p0) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.seededContainerLoot");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.WrittenBookContent.Builder writtenBookContent(io.papermc.paper.text.Filtered<java.lang.String> p0, java.lang.String p1) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.writtenBookContent");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.WritableBookContent.Builder writeableBookContent() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.writeableBookContent");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.ItemArmorTrim.Builder itemArmorTrim(org.bukkit.inventory.meta.trim.ArmorTrim p0) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.itemArmorTrim");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.LodestoneTracker.Builder lodestoneTracker() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.lodestoneTracker");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.Fireworks.Builder fireworks() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.fireworks");
        return dev.chunkwake.bridge.SoftValue.object();
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
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.bannerPatternLayers");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.BlockItemDataProperties.Builder blockItemStateProperties() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.blockItemStateProperties");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.ItemContainerContents.Builder itemContainerContents() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.itemContainerContents");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.JukeboxPlayable.Builder jukeboxPlayable(org.bukkit.JukeboxSong p0) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.jukeboxPlayable");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.Tool.Builder tool() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.tool");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.Tool.Rule toolRule(io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.block.BlockType> p0, java.lang.Float p1, net.kyori.adventure.util.TriState p2) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.toolRule");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.ItemAdventurePredicate.Builder itemAdventurePredicate() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.itemAdventurePredicate");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.CustomModelData.Builder customModelData() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.customModelData");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.MapId mapId(int p0) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.mapId");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.UseRemainder useRemainder(org.bukkit.inventory.ItemStack p0) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.useRemainder");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.Consumable.Builder consumable() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.consumable");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.UseCooldown.Builder useCooldown(float p0) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.useCooldown");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.DamageResistant damageResistant(io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.damage.DamageType> p0) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.damageResistant");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.Enchantable enchantable(int p0) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.enchantable");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.Repairable repairable(io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.inventory.ItemType> p0) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.repairable");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.Equippable.Builder equippable(org.bukkit.inventory.EquipmentSlot p0) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.equippable");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.DeathProtection.Builder deathProtection() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.deathProtection");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.OminousBottleAmplifier ominousBottleAmplifier(int p0) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.ominousBottleAmplifier");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.BlocksAttacks.Builder blocksAttacks() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.blocksAttacks");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.TooltipDisplay.Builder tooltipDisplay() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.tooltipDisplay");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.Weapon.Builder weapon() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.weapon");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.KineticWeapon.Builder kineticWeapon() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.kineticWeapon");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.KineticWeapon.Condition kineticWeaponCondition(int p0, float p1, float p2) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.kineticWeaponCondition");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.UseEffects.Builder useEffects() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.useEffects");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.PiercingWeapon.Builder piercingWeapon() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.piercingWeapon");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.AttackRange.Builder attackRange() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.attackRange");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.SwingAnimation.Builder swingAnimation() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.swingAnimation");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.SulfurCubeContent sulfurCubeContent(org.bukkit.inventory.ItemStack p0) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.sulfurCubeContent");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.MobVisibility mobVisibility(io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.entity.EntityType> p0, float p1) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.mobVisibility");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.VillagerFood villagerFood(int p0) {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.villagerFood");
        return dev.chunkwake.bridge.SoftValue.object();
    }

    @Override public io.papermc.paper.datacomponent.item.SignText.Builder signText() {
        dev.chunkwake.bridge.SoftValue.note("ItemComponentTypesBridge.signText");
        return dev.chunkwake.bridge.SoftValue.object();
    }
}
