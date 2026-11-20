package dev.chunkwake.bridge;

import net.minecraft.world.item.component.ResolvableProfile;
import org.bukkit.inventory.meta.SkullMeta;

/**
 * Конкретная реализация {@link SkullMeta} для предметов-голов.
 *
 * <p>Нужна, потому что EconomyShopGUI ищет у меты объявленное поле
 * {@code profile} и проверяет его тип ({@code ResolvableProfile}, не
 * {@code GameProfile}); у динамического Proxy полей нет. Остальные методы —
 * заглушки: значения хранит Rust-стек, а плагинам важна совместимость типов.
 */
final class SkullMetaImpl implements SkullMeta {
    /** Поле, которое ищет EconomyShopGUI ({@code SkullUtil}). */
    public ResolvableProfile profile;

    SkullMetaImpl(ResolvableProfile profile) {
        this.profile = profile == null ? new ResolvableProfile() : profile;
    }

    @Override
    public boolean addAttributeModifier(org.bukkit.attribute.Attribute p0, org.bukkit.attribute.AttributeModifier p1) {
        return false;
    }
    @Override
    public boolean addEnchant(org.bukkit.enchantments.Enchantment p0, int p1, boolean p2) {
        return false;
    }
    @Override
    public void addItemFlags(org.bukkit.inventory.ItemFlag[] p0) {
    }
    @Override
    public org.bukkit.inventory.meta.SkullMeta clone() {
        return new SkullMetaImpl(this.profile);
    }

    @Override
    public void customName(net.kyori.adventure.text.Component p0) {
    }
    @Override
    public net.kyori.adventure.text.Component customName() {
        return null;
    }
    @Override
    public java.lang.String getAsComponentString() {
        return null;
    }
    @Override
    public java.lang.String getAsString() {
        return null;
    }
    @Override
    public java.util.Collection getAttributeModifiers(org.bukkit.attribute.Attribute p0) {
        return null;
    }
    @Override
    public com.google.common.collect.Multimap getAttributeModifiers() {
        return null;
    }
    @Override
    public com.google.common.collect.Multimap getAttributeModifiers(org.bukkit.inventory.EquipmentSlot p0) {
        return null;
    }
    @Override
    public java.util.Set getCanDestroy() {
        return null;
    }
    @Override
    public java.util.Set getCanPlaceOn() {
        return null;
    }
    @Override
    public int getCustomModelData() {
        return 0;
    }
    @Override
    public org.bukkit.inventory.meta.components.CustomModelDataComponent getCustomModelDataComponent() {
        return StackImpl.customModelData();
    }
    @Override
    public org.bukkit.inventory.meta.tags.CustomItemTagContainer getCustomTagContainer() {
        return null;
    }
    @Override
    public org.bukkit.Tag getDamageResistant() {
        return null;
    }
    @Override
    public io.papermc.paper.registry.set.RegistryKeySet getDamageResistantTypes() {
        return null;
    }
    @Override
    public java.util.Set getDestroyableKeys() {
        return null;
    }
    @Override
    public java.lang.String getDisplayName() {
        return null;
    }
    @Override
    public net.md_5.bungee.api.chat.BaseComponent[] getDisplayNameComponent() {
        return null;
    }
    @Override
    public int getEnchantLevel(org.bukkit.enchantments.Enchantment p0) {
        return 0;
    }
    @Override
    public int getEnchantable() {
        return 0;
    }
    @Override
    public java.lang.Boolean getEnchantmentGlintOverride() {
        return null;
    }
    @Override
    public java.util.Map getEnchants() {
        return null;
    }
    @Override
    public org.bukkit.inventory.meta.components.EquippableComponent getEquippable() {
        return null;
    }
    @Override
    public org.bukkit.inventory.meta.components.FoodComponent getFood() {
        return null;
    }
    @Override
    public java.util.Set getItemFlags() {
        return null;
    }
    @Override
    public org.bukkit.NamespacedKey getItemModel() {
        return null;
    }
    @Override
    public java.lang.String getItemName() {
        return null;
    }
    @Override
    public org.bukkit.inventory.meta.components.JukeboxPlayableComponent getJukeboxPlayable() {
        return null;
    }
    @Override
    public java.lang.String getLocalizedName() {
        return null;
    }
    @Override
    public java.util.List getLore() {
        return null;
    }
    @Override
    public java.util.List getLoreComponents() {
        return null;
    }
    @Override
    public int getMaxStackSize() {
        return 0;
    }
    @Override
    public org.bukkit.NamespacedKey getNoteBlockSound() {
        return null;
    }
    @Override
    public java.lang.String getOwner() {
        return null;
    }
    @Override
    public org.bukkit.profile.PlayerProfile getOwnerProfile() {
        return null;
    }
    @Override
    public org.bukkit.OfflinePlayer getOwningPlayer() {
        return null;
    }
    @Override
    public org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        return null;
    }
    @Override
    public java.util.Set getPlaceableKeys() {
        return null;
    }
    @Override
    public com.destroystokyo.paper.profile.PlayerProfile getPlayerProfile() {
        return null;
    }
    @Override
    public org.bukkit.inventory.ItemRarity getRarity() {
        return null;
    }
    @Override
    public org.bukkit.inventory.meta.components.ToolComponent getTool() {
        return null;
    }
    @Override
    public org.bukkit.NamespacedKey getTooltipStyle() {
        return null;
    }
    @Override
    public org.bukkit.inventory.meta.components.UseCooldownComponent getUseCooldown() {
        return null;
    }
    @Override
    public org.bukkit.inventory.ItemStack getUseRemainder() {
        return null;
    }
    @Override
    public boolean hasAttributeModifiers() {
        return false;
    }
    @Override
    public boolean hasConflictingEnchant(org.bukkit.enchantments.Enchantment p0) {
        return false;
    }
    @Override
    public boolean hasCustomModelData() {
        return false;
    }
    @Override
    public boolean hasCustomModelDataComponent() {
        return false;
    }
    @Override
    public boolean hasCustomName() {
        return false;
    }
    @Override
    public boolean hasDamageResistant() {
        return false;
    }
    @Override
    public boolean hasDestroyableKeys() {
        return false;
    }
    @Override
    public boolean hasEnchant(org.bukkit.enchantments.Enchantment p0) {
        return false;
    }
    @Override
    public boolean hasEnchantable() {
        return false;
    }
    @Override
    public boolean hasEnchantmentGlintOverride() {
        return false;
    }
    @Override
    public boolean hasEnchants() {
        return false;
    }
    @Override
    public boolean hasEquippable() {
        return false;
    }
    @Override
    public boolean hasFood() {
        return false;
    }
    @Override
    public boolean hasItemFlag(org.bukkit.inventory.ItemFlag p0) {
        return false;
    }
    @Override
    public boolean hasItemModel() {
        return false;
    }
    @Override
    public boolean hasItemName() {
        return false;
    }
    @Override
    public boolean hasJukeboxPlayable() {
        return false;
    }
    @Override
    public boolean hasLocalizedName() {
        return false;
    }
    @Override
    public boolean hasLore() {
        return false;
    }
    @Override
    public boolean hasMaxStackSize() {
        return false;
    }
    @Override
    public boolean hasOwner() {
        return false;
    }
    @Override
    public boolean hasPlaceableKeys() {
        return false;
    }
    @Override
    public boolean hasRarity() {
        return false;
    }
    @Override
    public boolean hasTool() {
        return false;
    }
    @Override
    public boolean hasTooltipStyle() {
        return false;
    }
    @Override
    public boolean hasUseCooldown() {
        return false;
    }
    @Override
    public boolean hasUseRemainder() {
        return false;
    }
    @Override
    public boolean isFireResistant() {
        return false;
    }
    @Override
    public boolean isGlider() {
        return false;
    }
    @Override
    public boolean isHideTooltip() {
        return false;
    }
    @Override
    public boolean isUnbreakable() {
        return false;
    }
    @Override
    public void itemName(net.kyori.adventure.text.Component p0) {
    }
    @Override
    public net.kyori.adventure.text.Component itemName() {
        return null;
    }
    @Override
    public void lore(java.util.List p0) {
    }
    @Override
    public java.util.List lore() {
        return null;
    }
    @Override
    public boolean removeAttributeModifier(org.bukkit.inventory.EquipmentSlot p0) {
        return false;
    }
    @Override
    public boolean removeAttributeModifier(org.bukkit.attribute.Attribute p0) {
        return false;
    }
    @Override
    public boolean removeAttributeModifier(org.bukkit.attribute.Attribute p0, org.bukkit.attribute.AttributeModifier p1) {
        return false;
    }
    @Override
    public boolean removeEnchant(org.bukkit.enchantments.Enchantment p0) {
        return false;
    }
    @Override
    public void removeEnchantments() {
    }
    @Override
    public void removeItemFlags(org.bukkit.inventory.ItemFlag[] p0) {
    }
    @Override
    public java.util.Map serialize() {
        return null;
    }
    @Override
    public void setAttributeModifiers(com.google.common.collect.Multimap p0) {
    }
    @Override
    public void setCanDestroy(java.util.Set p0) {
    }
    @Override
    public void setCanPlaceOn(java.util.Set p0) {
    }
    @Override
    public void setCustomModelData(java.lang.Integer p0) {
    }
    @Override
    public void setCustomModelDataComponent(org.bukkit.inventory.meta.components.CustomModelDataComponent p0) {
    }
    @Override
    public void setDamageResistant(org.bukkit.Tag p0) {
    }
    @Override
    public void setDamageResistantTypes(io.papermc.paper.registry.set.RegistryKeySet p0) {
    }
    @Override
    public void setDestroyableKeys(java.util.Collection p0) {
    }
    @Override
    public void setDisplayName(java.lang.String p0) {
    }
    @Override
    public void setDisplayNameComponent(net.md_5.bungee.api.chat.BaseComponent[] p0) {
    }
    @Override
    public void setEnchantable(java.lang.Integer p0) {
    }
    @Override
    public void setEnchantmentGlintOverride(java.lang.Boolean p0) {
    }
    @Override
    public void setEquippable(org.bukkit.inventory.meta.components.EquippableComponent p0) {
    }
    @Override
    public void setFireResistant(boolean p0) {
    }
    @Override
    public void setFood(org.bukkit.inventory.meta.components.FoodComponent p0) {
    }
    @Override
    public void setGlider(boolean p0) {
    }
    @Override
    public void setHideTooltip(boolean p0) {
    }
    @Override
    public void setItemModel(org.bukkit.NamespacedKey p0) {
    }
    @Override
    public void setItemName(java.lang.String p0) {
    }
    @Override
    public void setJukeboxPlayable(org.bukkit.inventory.meta.components.JukeboxPlayableComponent p0) {
    }
    @Override
    public void setLocalizedName(java.lang.String p0) {
    }
    @Override
    public void setLore(java.util.List p0) {
    }
    @Override
    public void setLoreComponents(java.util.List p0) {
    }
    @Override
    public void setMaxStackSize(java.lang.Integer p0) {
    }
    @Override
    public void setNoteBlockSound(org.bukkit.NamespacedKey p0) {
    }
    @Override
    public boolean setOwner(java.lang.String p0) {
        return false;
    }
    @Override
    public void setOwnerProfile(org.bukkit.profile.PlayerProfile p0) {
    }
    @Override
    public boolean setOwningPlayer(org.bukkit.OfflinePlayer p0) {
        return false;
    }
    @Override
    public void setPlaceableKeys(java.util.Collection p0) {
    }
    @Override
    public void setPlayerProfile(com.destroystokyo.paper.profile.PlayerProfile p0) {
    }
    @Override
    public void setRarity(org.bukkit.inventory.ItemRarity p0) {
    }
    @Override
    public void setTool(org.bukkit.inventory.meta.components.ToolComponent p0) {
    }
    @Override
    public void setTooltipStyle(org.bukkit.NamespacedKey p0) {
    }
    @Override
    public void setUnbreakable(boolean p0) {
    }
    @Override
    public void setUseCooldown(org.bukkit.inventory.meta.components.UseCooldownComponent p0) {
    }
    @Override
    public void setUseRemainder(org.bukkit.inventory.ItemStack p0) {
    }
    @Override
    public void setVersion(int p0) {
    }


}
