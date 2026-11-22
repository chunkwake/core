package net.minecraft.world.item;

/**
 * Минимальный, но рабочий NMS {@code ItemStack}: хранит id предмета и
 * количество. Используется ProtocolLib для создания «пустого» стека и
 * конвертации Bukkit-предметов.
 */
public class ItemStack implements net.minecraft.core.component.DataComponentHolder, ItemInstance {
    public String item;
    public int count;
    /** Числовой id предмета из пакета (для перекодирования). */
    public int rawId = -1;
    /** Исходные байты патча компонентов (сохраняются при правке count). */
    public byte[] patch = new byte[0];
    /**
     * Компоненты предмета в сериализованном виде (`id count [json]` из
     * {@code StackImpl.line}). Нужны, чтобы {@code ItemStack.CODEC}
     * переносил display name/lore/PDC, а не только id+count: nightcore
     * проверяет round-trip через {@code ItemStack.isSimilar}.
     */
    public String meta = "";
    /** NBT в терминах старого API (DeluxeMenus: {@code u()}/{@code v()}/{@code c()}). */
    private net.minecraft.nbt.NBTTagCompound legacyTag;
    /** Компоненты предмета (как ванильный {@code ItemStack.components}). */
    public net.minecraft.core.component.PatchedDataComponentMap components =
            new net.minecraft.core.component.PatchedDataComponentMap(null);

    public static final ItemStack EMPTY = new ItemStack("minecraft:air", 0);
    public static final Object OPTIONAL_STREAM_CODEC = null;
    public static final Object OPTIONAL_UNTRUSTED_STREAM_CODEC = null;
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
                    if (input.meta != null && !input.meta.isEmpty()) {
                        // Base64url без padding: строка попадает в
                        // CompoundTag.toString() и разбирается нашим
                        // TagParser, поэтому не должна содержать запятых,
                        // кавычек и `=`.
                        tag.putString("meta", java.util.Base64.getUrlEncoder()
                                .withoutPadding()
                                .encodeToString(input.meta.getBytes(
                                        java.nio.charset.StandardCharsets.UTF_8)));
                    }
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
                        ItemStack stack = new ItemStack(id, count);
                        String meta = tag.getStringOr("meta", "");
                        if (!meta.isEmpty()) {
                            try {
                                stack.meta = new String(java.util.Base64.getUrlDecoder()
                                        .decode(meta), java.nio.charset.StandardCharsets.UTF_8);
                            } catch (IllegalArgumentException ignored) {
                                stack.meta = "";
                            }
                        }
                        return com.mojang.serialization.DataResult.success(
                                com.mojang.datafixers.util.Pair.of(stack, input));
                    }
                    return com.mojang.serialization.DataResult.error(
                            () -> "ItemStack tag must be a compound");
                }
            };

    public ItemStack() {
        this("minecraft:air", 0);
    }

    public ItemStack(net.minecraft.world.level.ItemLike item, int count) {
        this(item == null ? null : item.asItem(), count);
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

    @Override
    public int count() {
        return getCount();
    }

    @Override
    public net.minecraft.core.Holder<Item> typeHolder() {
        return null;
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

    public void hurtAndBreak(int amount, net.minecraft.server.level.ServerLevel level,
            net.minecraft.world.entity.LivingEntity entity,
            java.util.function.Consumer<ItemStack> onBreak) {
    }

    public void hurtAndBreak(int amount, net.minecraft.world.entity.LivingEntity entity,
            net.minecraft.world.InteractionHand hand) {
    }

    public void hurtAndBreak(int amount, net.minecraft.world.entity.LivingEntity entity,
            net.minecraft.world.entity.EquipmentSlot slot) {
    }

    public void grow(int amount) {
        this.setCount(this.getCount() + amount);
    }

    public void shrink(int amount) {
        this.setCount(Math.max(0, this.getCount() - amount));
    }

    public ItemStack copyWithCount(int count) {
        ItemStack copy = copy();
        copy.setCount(count);
        return copy;
    }

    public ItemStack transmuteCopy(net.minecraft.world.level.ItemLike item, int count) {
        return new ItemStack(item, count);
    }

    public boolean is(net.minecraft.world.item.Item item) {
        return this.getItem() == item;
    }

    private net.minecraft.nbt.CompoundTag modernTag;

    public net.minecraft.nbt.CompoundTag getTag() {
        return this.modernTag;
    }

    public void setTag(net.minecraft.nbt.CompoundTag tag) {
        this.modernTag = tag;
    }

    public net.minecraft.nbt.CompoundTag getOrCreateTag() {
        net.minecraft.nbt.CompoundTag tag = getTag();
        if (tag == null) {
            tag = new net.minecraft.nbt.CompoundTag();
            setTag(tag);
        }
        return tag;
    }

    public void save(net.minecraft.nbt.CompoundTag tag) {
    }

    public boolean hasNonDefault(net.minecraft.core.component.DataComponentType<?> type) {
        return false;
    }

    public net.minecraft.core.component.DataComponentPatch getComponentsPatch() {
        return new net.minecraft.core.component.DataComponentPatch();
    }

    public void setComponents(net.minecraft.core.component.DataComponentPatch patch) {
    }

    public boolean isCorrectToolForDrops(net.minecraft.world.level.block.state.BlockState state) {
        return false;
    }

    public boolean canBreakBlockInAdventureMode(net.minecraft.world.level.block.state.pattern.BlockInWorld state) {
        return true;
    }

    public boolean canPlaceOnBlockInAdventureMode(net.minecraft.world.level.block.state.pattern.BlockInWorld state) {
        return true;
    }

    public boolean hasAdventureModeBreakTagForBlock(net.minecraft.core.HolderSet<net.minecraft.world.level.block.Block> set,
            net.minecraft.world.level.block.state.BlockState state) {
        return false;
    }

    public boolean hasAdventureModePlaceTagForBlock(net.minecraft.core.HolderSet<net.minecraft.world.level.block.Block> set,
            net.minecraft.world.level.block.state.BlockState state) {
        return false;
    }

    public static net.minecraft.network.codec.StreamCodec<?> validatedStreamCodec(
            net.minecraft.network.codec.StreamCodec<?> codec) {
        return codec;
    }

    public static boolean isSameItemSameTags(ItemStack first, ItemStack second) {
        if (first == null || second == null) {
            return first == second;
        }
        return first.is(second.getItem())
                && java.util.Objects.equals(first.getTag(), second.getTag());
    }

    public static boolean isSameItemSameComponents(ItemStack first, ItemStack second) {
        return isSameItemSameTags(first, second);
    }

    public boolean isValidRepairItem(ItemStack repair) {
        return false;
    }

    public static int hashItemAndComponents(ItemStack stack) {
        return stack == null ? 0 : java.util.Objects.hash(stack.getItem(), stack.getTag());
    }

    public ItemStack copy() {
        return new ItemStack(this.item, this.count);
    }

    @Override
    public String toString() {
        return this.item + " x " + this.count;
    }

    /** DataComponent-API (ImageFrame): значения не хранятся, но API есть. */
    public <T> T get(net.minecraft.core.component.DataComponentType<? extends T> type) {
        return null;
    }

    public <T> T getOrDefault(net.minecraft.core.component.DataComponentType<? extends T> type, T fallback) {
        return fallback;
    }

    public <T> void set(net.minecraft.core.component.DataComponentType<T> type, T value) {
    }

    /** {@code ItemStack.remove(DataComponentType)} (CraftEngine). */
    public <T> T remove(net.minecraft.core.component.DataComponentType<? extends T> type) {
        return null;
    }

    public void applyComponents(net.minecraft.core.component.DataComponentPatch patch) {
    }

    /** Paper 26.x: {@code applyComponentsAndValidate(patch)} (ItemJoin). */
    public void applyComponentsAndValidate(net.minecraft.core.component.DataComponentPatch patch) {
        applyComponents(patch);
    }
}
