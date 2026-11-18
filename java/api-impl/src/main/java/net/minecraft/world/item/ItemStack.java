package net.minecraft.world.item;

/**
 * Минимальный, но рабочий NMS {@code ItemStack}: хранит id предмета и
 * количество. Используется ProtocolLib для создания «пустого» стека и
 * конвертации Bukkit-предметов.
 */
public class ItemStack {
    public String item;
    public int count;

    public ItemStack() {
        this("minecraft:air", 0);
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

    public ItemStack copy() {
        return new ItemStack(this.item, this.count);
    }

    @Override
    public String toString() {
        return this.item + " x " + this.count;
    }
}
