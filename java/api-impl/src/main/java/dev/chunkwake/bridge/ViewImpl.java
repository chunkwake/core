package dev.chunkwake.bridge;

import java.util.List;
import net.kyori.adventure.text.Component;
import org.bukkit.entity.HumanEntity;
import org.bukkit.event.inventory.InventoryType;
import org.bukkit.event.inventory.InventoryType.SlotType;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryView;
import org.bukkit.inventory.ItemStack;

/**
 * {@link InventoryView}: верх — окно (инвентарь плагина или блока), низ — инвентарь игрока.
 * Сырые слоты как у клиента: сначала верх, затем 27 основных и 9 хотбара.
 */
final class ViewImpl {
    final InventoryView api = Stub.of(InventoryView.class, this);
    private final PlayerImpl player;
    private final Inventory top;
    private final InventoryType type;
    private final Component title;
    /** Снимок клика: что было в нажатом слоте и на курсоре. */
    private int clicked = Integer.MIN_VALUE;
    private ItemStack current;
    private ItemStack cursor;

    ViewImpl(PlayerImpl player, Inventory top, InventoryType type, Component title) {
        this.player = player;
        this.top = top;
        this.type = type;
        this.title = title;
    }

    ViewImpl clicked(int raw, ItemStack current, ItemStack cursor) {
        this.clicked = raw;
        this.current = current;
        this.cursor = cursor;
        return this;
    }

    /** Верх без хранилища на стороне Java (сундук, печь, верстак, сетка игрока). */
    static Inventory blockTop(PlayerImpl p, InventoryType type, int size) {
        return Stub.of(Inventory.class, new BlockTop(p, type, size));
    }

    /** Окно блока: размер и тип; предметы видны через снимок клика. */
    record BlockTop(PlayerImpl p, InventoryType type, int size) {
        public int getSize() { return size; }
        public InventoryType getType() { return type; }
        public org.bukkit.inventory.InventoryHolder getHolder() { return type == InventoryType.CRAFTING ? p.api : null; }
        public List<HumanEntity> getViewers() { return List.of(p.api); }
        public ItemStack getItem(int i) { return null; }
        public ItemStack[] getContents() { return new ItemStack[size]; }
        public ItemStack[] getStorageContents() { return new ItemStack[size]; }
        public boolean isEmpty() { return true; }
        public org.bukkit.Location getLocation() { return null; }
    }

    private int topSize() { return top.getSize(); }
    private boolean playerMenu() { return type == InventoryType.CRAFTING; }

    public Inventory getTopInventory() { return top; }
    public Inventory getBottomInventory() { return player.api.getInventory(); }
    public HumanEntity getPlayer() { return player.api; }
    public InventoryType getType() { return type; }
    public Component title() { return title; }
    public String getTitle() { return ChunkwakeServer.plain(title); }
    public String getOriginalTitle() { return getTitle(); }
    public void setTitle(String t) {}
    public org.bukkit.inventory.MenuType getMenuType() { return null; }
    public boolean setProperty(InventoryView.Property p, int v) { return false; }
    public void open() {}
    public void close() { player.api.closeInventory(); }
    public int countSlots() { return playerMenu() ? 46 : topSize() + 36; }

    public ItemStack getCursor() { return cursor; }
    public void setCursor(ItemStack s) { cursor = s; }

    public Inventory getInventory(int raw) {
        if (raw < 0) {
            return null;
        }
        if (playerMenu()) {
            return raw < 5 ? top : getBottomInventory();
        }
        return raw < topSize() ? top : getBottomInventory();
    }

    /** Сырой слот → индекс в своём инвентаре (`CraftInventoryView.convertSlot`). */
    public int convertSlot(int raw) {
        if (playerMenu()) {
            if (raw < 5) {
                return raw;
            }
            if (raw < 9) {
                return 39 - (raw - 5);
            }
            if (raw == 45) {
                return 40;
            }
            return raw >= 36 ? raw - 36 : raw;
        }
        if (raw < topSize()) {
            return raw;
        }
        int b = raw - topSize();
        return b < 27 ? b + 9 : b - 27;
    }

    public SlotType getSlotType(int raw) {
        if (raw < 0) {
            return SlotType.OUTSIDE;
        }
        if (playerMenu()) {
            if (raw == 0) {
                return SlotType.RESULT;
            }
            if (raw < 5) {
                return SlotType.CRAFTING;
            }
            if (raw < 9) {
                return SlotType.ARMOR;
            }
            if (raw == 45) {
                return SlotType.QUICKBAR;
            }
            return raw >= 36 ? SlotType.QUICKBAR : SlotType.CONTAINER;
        }
        if (raw < topSize()) {
            return switch (type) {
                case WORKBENCH -> raw == 0 ? SlotType.RESULT : SlotType.CRAFTING;
                case FURNACE, BLAST_FURNACE, SMOKER -> raw == 2 ? SlotType.RESULT : raw == 1 ? SlotType.FUEL : SlotType.CRAFTING;
                case ENCHANTING -> SlotType.CRAFTING;
                case GRINDSTONE -> raw == 2 ? SlotType.RESULT : SlotType.CRAFTING;
                default -> SlotType.CONTAINER;
            };
        }
        return raw - topSize() >= 27 ? SlotType.QUICKBAR : SlotType.CONTAINER;
    }

    public ItemStack getItem(int raw) {
        if (raw == clicked) {
            return current;
        }
        Inventory inv = getInventory(raw);
        return inv == null ? null : inv.getItem(convertSlot(raw));
    }

    public void setItem(int raw, ItemStack s) {
        Inventory inv = getInventory(raw);
        if (inv != null) {
            inv.setItem(convertSlot(raw), s);
        }
    }
}
