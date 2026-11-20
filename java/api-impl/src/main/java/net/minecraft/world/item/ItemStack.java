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
    /** NBT в терминах старого API (DeluxeMenus: {@code u()}/{@code v()}/{@code c()}). */
    private net.minecraft.nbt.NBTTagCompound legacyTag;
    /** Компоненты предмета (как ванильный {@code ItemStack.components}). */
    public net.minecraft.core.component.PatchedDataComponentMap components =
            new net.minecraft.core.component.PatchedDataComponentMap(null);

    public static final ItemStack EMPTY = new ItemStack("minecraft:air", 0);
    public static final net.minecraft.network.codec.StreamCodec<Object> STREAM_CODEC =
            net.minecraft.network.codec.SimpleStreamCodec.INSTANCE;

    /**
     * Минимальный {@code ItemStack.CODEC} (поле, которое nightcore берёт
     * рефлексией). Кодирует в настоящий CompoundTag (`id` + `count`):
     * nightcore проверяет результат именно на CompoundTag и затем
     * декодирует его обратно. Полные компоненты живут в Rust-модели и
     * через этот codec пока не переносятся.
     */
    public static final com.mojang.serialization.Codec<ItemStack> CODEC =
            new com.mojang.serialization.Codec<>() {
                @Override
                @SuppressWarnings("unchecked")
                public <T> com.mojang.serialization.DataResult<T> encode(
                        ItemStack input, com.mojang.serialization.DynamicOps<T> ops, T prefix) {
                    net.minecraft.nbt.CompoundTag tag = new net.minecraft.nbt.CompoundTag();
                    tag.putString("id", input.getItemId());
                    tag.putInt("count", input.getCount());
                    return com.mojang.serialization.DataResult.success((T) tag);
                }

                @Override
                public <T> com.mojang.serialization.DataResult<
                        com.mojang.datafixers.util.Pair<ItemStack, T>> decode(
                        com.mojang.serialization.DynamicOps<T> ops, T input) {
                    if (input instanceof net.minecraft.nbt.CompoundTag tag) {
                        String id = tag.getStringOr("id", "");
                        if (id.isEmpty()) {
                            return com.mojang.serialization.DataResult.error(
                                    () -> "No element id in the map");
                        }
                        int count = Math.max(tag.getIntOr("count", 1), 1);
                        return com.mojang.serialization.DataResult.success(
                                com.mojang.datafixers.util.Pair.of(
                                        new ItemStack(id, count), input));
                    }
                    return com.mojang.serialization.DataResult.error(
                            () -> "ItemStack tag must be a compound");
                }
            };

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

    /**
     * Старый {@code ItemStack.hasTag()}: DeluxeMenus ищет метод по имени
     * {@code u} (его таблица версий для 26.3 выбирает «обфусцированные»).
     */
    public boolean u() {
        return this.legacyTag != null && !this.legacyTag.getAllKeys().isEmpty();
    }

    /** Старый {@code ItemStack.getTag()}. */
    public net.minecraft.nbt.NBTTagCompound v() {
        return this.legacyTag;
    }

    /** Старый {@code ItemStack.setTag(NBTTagCompound)}. */
    public void c(net.minecraft.nbt.NBTTagCompound tag) {
        this.legacyTag = tag;
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
