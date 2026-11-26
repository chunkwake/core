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
    /** Реальный верх окна для синхронизации слотов на open/close. */
    final BlockTop blockTop;
    /** Снимок клика: что было в нажатом слоте и на курсоре. */
    private int clicked = Integer.MIN_VALUE;
    private ItemStack current;
    private ItemStack cursor;

    ViewImpl(PlayerImpl player, Inventory top, InventoryType type, Component title) {
        this(player, top, type, title, null);
    }

    ViewImpl(PlayerImpl player, Inventory top, InventoryType type, Component title,
            BlockTop blockTop) {
        this.player = player;
        this.top = top;
        this.type = type;
        this.title = title;
        this.blockTop = blockTop;
    }

    ViewImpl clicked(int raw, ItemStack current, ItemStack cursor) {
        this.clicked = raw;
        this.current = current;
        this.cursor = cursor;
        return this;
    }

    /** Верхний инвентарь окна: содержимое контейнера, а не пустышка. */
    static Inventory blockTop(PlayerImpl p, InventoryType type, int size) {
        return Stub.of(Inventory.class, new BlockTop(p, type, new ItemStack[size], null, ""));
    }

    /** Верхний инвентарь с настоящими предметами, позицией и типом блока. */
    static Inventory blockTop(PlayerImpl p, InventoryType type, ItemStack[] items,
            org.bukkit.Location location, String holderName) {
        return blockTop(new BlockTop(p, type, items, location, holderName));
    }

    /** Прокси верхнего инвентаря; impl остаётся доступен для синхронизации. */
    static Inventory blockTop(BlockTop impl) {
        Inventory inventory = Stub.of(Inventory.class, impl);
        impl.self = inventory;
        return inventory;
    }

    /**
     * Инвентарь контейнера для {@code InventoryOpenEvent}. Плагины (ChestSort)
     * читают {@code getContents()}, сортируют и вызывают {@code setContents()};
     * Rust получает изменённые слоты в ответе события.
     */
    static final class BlockTop {
        private final PlayerImpl p;
        private final InventoryType type;
        private final ItemStack[] items;
        private final org.bukkit.Location location;
        private final String holderName;
        private Inventory self;
        private org.bukkit.inventory.InventoryHolder holder;

        BlockTop(PlayerImpl p, InventoryType type, ItemStack[] items,
                org.bukkit.Location location, String holderName) {
            this.p = p;
            this.type = type;
            this.items = items;
            this.location = location;
            this.holderName = holderName;
        }

        public int getSize() { return items.length; }
        public InventoryType getType() { return type; }
        public org.bukkit.inventory.InventoryHolder getHolder() {
            if (type == InventoryType.CRAFTING) {
                return p.api;
            }
            if (holder == null) {
                holder = makeHolder();
            }
            return holder;
        }
        public org.bukkit.inventory.InventoryHolder getHolder(boolean useSnapshot) {
            return getHolder();
        }
        public List<HumanEntity> getViewers() { return List.of(p.api); }
        public ItemStack getItem(int i) {
            return i >= 0 && i < items.length ? items[i] : null;
        }
        public void setItem(int i, ItemStack s) {
            if (i >= 0 && i < items.length) {
                items[i] = s == null || s.getType().isAir() ? null : s;
            }
        }
        public ItemStack[] getContents() { return items.clone(); }
        public void setContents(ItemStack[] contents) {
            for (int i = 0; i < items.length; i++) {
                ItemStack s = contents != null && i < contents.length ? contents[i] : null;
                items[i] = s == null || s.getType().isAir() ? null : s;
            }
        }
        public ItemStack[] getStorageContents() { return getContents(); }
        public void setStorageContents(ItemStack[] contents) { setContents(contents); }
        public boolean isEmpty() {
            for (ItemStack s : items) {
                if (s != null && !s.getType().isAir() && s.getAmount() > 0) {
                    return false;
                }
            }
            return true;
        }
        public org.bukkit.Location getLocation() {
            return location == null ? null : location.clone();
        }
        public int getMaxStackSize() { return 64; }
        int size() { return items.length; }
        String line(int index) {
            if (index < 0 || index >= items.length) {
                return "-";
            }
            String line = StackImpl.line(items[index]);
            return line == null ? "-" : line;
        }
        void setLine(int index, String line) {
            if (index >= 0 && index < items.length) {
                items[index] = line == null || line.equals("-") ? null : StackImpl.parse(line);
            }
        }
        public java.util.ListIterator<ItemStack> iterator() {
            return java.util.Arrays.asList(getContents()).listIterator();
        }
        public java.util.ListIterator<ItemStack> iterator(int index) {
            return java.util.Arrays.asList(getContents()).listIterator(index);
        }
        public void clear() { java.util.Arrays.fill(items, null); }
        public void clear(int index) { setItem(index, null); }
        public void clear(int start, int end) {
            for (int i = Math.max(0, start); i < Math.min(items.length, end); i++) {
                items[i] = null;
            }
        }

        /**
         * Holder by real block material: ChestSort проверяет
         * {@code holder instanceof Chest} (или Barrel/ShulkerBox).
         */
        private org.bukkit.inventory.InventoryHolder makeHolder() {
            if (location == null || self == null) {
                return null;
            }
            org.bukkit.Material material =
                    org.bukkit.Material.matchMaterial(holderName == null ? "" : holderName);
            if (material == null || material.isAir()) {
                material = switch (type) {
                    case CHEST -> org.bukkit.Material.CHEST;
                    case HOPPER -> org.bukkit.Material.HOPPER;
                    case DISPENSER -> org.bukkit.Material.DISPENSER;
                    case FURNACE -> org.bukkit.Material.FURNACE;
                    case BLAST_FURNACE -> org.bukkit.Material.BLAST_FURNACE;
                    case SMOKER -> org.bukkit.Material.SMOKER;
                    default -> null;
                };
            }
            if (material == null) {
                return null;
            }
            String key = material.getKey().getKey();
            HolderImpl impl = new HolderImpl(material, self, location);
            if (key.equals("barrel")) {
                // ChestSort проверяет instanceof Chest, а не только Barrel.
                return (org.bukkit.inventory.InventoryHolder) Stub.ofAll(
                        new Class<?>[] {org.bukkit.block.Barrel.class, org.bukkit.block.Chest.class},
                        impl);
            }
            if (key.endsWith("shulker_box")) {
                return (org.bukkit.inventory.InventoryHolder) Stub.ofAll(
                        new Class<?>[] {org.bukkit.block.ShulkerBox.class,
                                org.bukkit.block.Chest.class},
                        impl);
            }
            if (key.equals("hopper")) {
                return Stub.of(org.bukkit.block.Hopper.class, impl);
            }
            if (key.equals("dispenser")) {
                return Stub.of(org.bukkit.block.Dispenser.class, impl);
            }
            if (key.equals("dropper")) {
                return Stub.of(org.bukkit.block.Dropper.class, impl);
            }
            if (key.equals("furnace")) {
                return Stub.of(org.bukkit.block.Furnace.class, impl);
            }
            if (key.equals("blast_furnace")) {
                return Stub.of(org.bukkit.block.BlastFurnace.class, impl);
            }
            if (key.equals("smoker")) {
                return Stub.of(org.bukkit.block.Smoker.class, impl);
            }
            if (key.equals("ender_chest")) {
                return null;
            }
            if (key.contains("chest") || type == InventoryType.CHEST) {
                return Stub.of(org.bukkit.block.Chest.class, impl);
            }
            return null;
        }
    }

    /** Общие методы block-state holder'а контейнера. */
    static final class HolderImpl {
        private final org.bukkit.Material material;
        private final Inventory inventory;
        private final org.bukkit.Location location;

        HolderImpl(org.bukkit.Material material, Inventory inventory,
                org.bukkit.Location location) {
            this.material = material;
            this.inventory = inventory;
            this.location = location;
        }

        public Inventory getInventory() { return inventory; }
        public org.bukkit.Material getType() { return material; }
        public org.bukkit.World getWorld() { return location.getWorld(); }
        public org.bukkit.Location getLocation() { return location.clone(); }
        public org.bukkit.block.Block getBlock() { return location.getBlock(); }
        public boolean isPlaced() { return true; }
        public void update() {}
        @Override
        public String toString() { return "Chunkwake" + material; }
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
