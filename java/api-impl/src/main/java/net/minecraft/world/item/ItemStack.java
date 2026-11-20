package net.minecraft.world.item;

/**
 * Минимальный, но рабочий NMS {@code ItemStack}: хранит id предмета и
 * количество. Используется ProtocolLib для создания «пустого» стека и
 * конвертации Bukkit-предметов.
 */
public class ItemStack {
    public String item;
    public int count;
    /** Числовой id предмета из пакета (для перекодирования). */
    public int rawId = -1;
    /** Исходные байты патча компонентов (сохраняются при правке count). */
    public byte[] patch = new byte[0];

    public static final ItemStack EMPTY = new ItemStack("minecraft:air", 0);
    public static final net.minecraft.network.codec.StreamCodec<Object> STREAM_CODEC =
            net.minecraft.network.codec.SimpleStreamCodec.INSTANCE;

    public ItemStack() {
        this("minecraft:air", 0);
    }

    public ItemStack(Item item, int count) {
        this(item == null ? "minecraft:air" : item.getId(), count);
    }

    public ItemStack(String item, int count) {
        this.item = item == null ? "minecraft:air" : item;
        this.count = count;
    }

    public boolean isEmpty() {
        return this.count <= 0 || "minecraft:air".equals(this.item);
    }

    public int getCount() {
        return this.count;
    }

    public void setCount(int count) {
        this.count = count;
    }

    public String getItemId() {
        return this.item;
    }

    public Item getItem() {
        return new Item(this.item);
    }

    public ItemStack copy() {
        return new ItemStack(this.item, this.count);
    }

    @Override
    public String toString() {
        return this.item + " x " + this.count;
    }

    /** DataComponent-API (ImageFrame): значения не хранятся, но API есть. */
    public <T> T get(net.minecraft.core.component.DataComponentType<T> type) {
        return null;
    }

    public <T> T getOrDefault(net.minecraft.core.component.DataComponentType<T> type, T fallback) {
        return fallback;
    }

    public <T> void set(net.minecraft.core.component.DataComponentType<T> type, T value) {
    }

    public void applyComponents(net.minecraft.core.component.DataComponentPatch patch) {
    }
}
