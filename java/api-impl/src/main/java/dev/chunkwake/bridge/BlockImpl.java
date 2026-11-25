package dev.chunkwake.bridge;

import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;

/** {@link Block}: живой блок мира (тип читается из Rust; {@code key} — снимок события). */
final class BlockImpl {
    final Block api = Stub.of(Block.class, this);
    private final World world;
    private final int x, y, z;
    /** Ключ блока на момент события, например {@code minecraft:stone}; {@code null} — читать из мира. */
    private final String key;

    BlockImpl(World world, int x, int y, int z, String key) {
        this.world = world;
        this.x = x;
        this.y = y;
        this.z = z;
        this.key = key;
    }

    private void ensureChunk() {
        WorldImpl impl = WorldImpl.of(world);
        if (impl != null) {
            impl.ensureChunk(x, z);
        }
    }

    public int getX() { return x; }
    public int getY() { return y; }
    public int getZ() { return z; }
    public World getWorld() { return world; }
    public Location getLocation() { return new Location(world, x, y, z); }
    public Block getRelative(int dx, int dy, int dz) { return new BlockImpl(world, x + dx, y + dy, z + dz, null).api; }
    public Block getRelative(BlockFace f) { return getRelative(f.getModX(), f.getModY(), f.getModZ()); }
    public Block getRelative(BlockFace f, int n) {
        return getRelative(f.getModX() * n, f.getModY() * n, f.getModZ() * n);
    }

    public Material getType() {
        ensureChunk();
        Object custom = customState();
        if (custom != null) {
            org.bukkit.block.data.BlockData data = engineBlockData(custom);
            if (data != null) {
                return data.getMaterial();
            }
            return Material.BRICKS;
        }
        String k = key != null ? key : WorldImpl.request(world, "get " + x + " " + y + " " + z);
        Material m = k.isEmpty() ? null : Material.matchMaterial(k);
        return m == null ? Material.AIR : m;
    }

    /** NMS-состояние из Java-оверлея мира (CE пишет туда через LevelWriter). */
    private net.minecraft.world.level.block.state.BlockState nmsState() {
        WorldImpl impl = WorldImpl.of(world);
        if (impl == null) {
            return null;
        }
        return impl.getHandle().getBlockState(new net.minecraft.core.BlockPos(x, y, z));
    }

    /** ClassLoader плагина CraftEngine (API-классы не видны системному). */
    private static ClassLoader craftEngineLoader() {
        try {
            org.bukkit.plugin.Plugin plugin =
                    org.bukkit.Bukkit.getPluginManager().getPlugin("CraftEngine");
            if (plugin != null) {
                return plugin.getClass().getClassLoader();
            }
        } catch (Throwable ignored) {
            // CE не загружен
        }
        return null;
    }

    /** Custom-состояние CraftEngine в Java-оверлее NMS-мира, если есть. */
    private Object customState() {
        ClassLoader loader = craftEngineLoader();
        if (loader == null) {
            return null;
        }
        Object state = engineState(loader);
        if (state != null) {
            record(state);
            return state;
        }
        // Рестарт: оверлей пуст, но блок был сохранён CustomBlockStore.
        WorldImpl impl = WorldImpl.of(world);
        if (impl != null) {
            net.minecraft.world.level.block.state.BlockState restored =
                    impl.restoreCustomBlock(new net.minecraft.core.BlockPos(x, y, z));
            if (restored != null) {
                state = engineState(loader);
                if (state != null) {
                    record(state);
                    return state;
                }
            }
        }
        return null;
    }

    private Object engineState(ClassLoader loader) {
        try {
            Class<?> api = Class.forName(
                    "net.momirealms.craftengine.bukkit.api.CraftEngineBlocks", false, loader);
            return api.getMethod("getCustomBlockState", Block.class).invoke(null, this.api);
        } catch (Throwable t) {
            return null;
        }
    }

    private void record(Object state) {
        WorldImpl impl = WorldImpl.of(world);
        if (impl == null) {
            return;
        }
        // Храним полное CE-состояние, включая свойства (facing/lit/age...),
        // а не только id block definition.
        CustomBlockStore.record(impl.dim, new net.minecraft.core.BlockPos(x, y, z),
                String.valueOf(state));
    }

    private static org.bukkit.block.data.BlockData engineBlockData(Object state) {
        ClassLoader loader = craftEngineLoader();
        if (loader == null) {
            return null;
        }
        try {
            Class<?> api = Class.forName(
                    "net.momirealms.craftengine.bukkit.api.CraftEngineBlocks", false, loader);
            Class<?> immutable = Class.forName(
                    "net.momirealms.craftengine.core.block.ImmutableBlockState", false, loader);
            return (org.bukkit.block.data.BlockData) api
                    .getMethod("getBukkitBlockData", immutable).invoke(null, state);
        } catch (Throwable ignored) {
            return null;
        }
    }

    public boolean isEmpty() { return getType().isAir(); }

    public void setType(Material type) { setType(type, true); }

    /** Состояние по умолчанию для типа; с физикой соседи (редстоун) обновятся в тике. */
    public void setType(Material type, boolean physics) {
        if (!type.isBlock()) {
            throw new IllegalArgumentException("Material " + type + " is not a block");
        }
        ensureChunk();
        WorldImpl.request(world, "set " + x + " " + y + " " + z + " " + type.getKey() + (physics ? " 1" : " 0"));
    }

    public org.bukkit.block.data.BlockData getBlockData() {
        ensureChunk();
        Object custom = customState();
        if (custom != null) {
            org.bukkit.block.data.BlockData data = engineBlockData(custom);
            if (data != null) {
                return data;
            }
        }
        return BlockDataImpl.parse(WorldImpl.request(world, "data " + x + " " + y + " " + z));
    }

    /** Коллизия: у custom-блока CE берём форму его visual/carrier-состояния. */
    public org.bukkit.util.VoxelShape getCollisionShape() {
        Object custom = customState();
        if (custom != null) {
            try {
                Object visual = custom.getClass().getMethod("visualBlockState").invoke(custom);
                Object raw = visual.getClass().getMethod("minecraftState").invoke(visual);
                if (raw instanceof net.minecraft.world.level.block.state.BlockState state) {
                    return VoxelShapeImpl.of(CustomShapes.collision(state));
                }
            } catch (Throwable ignored) {
                // fallback ниже
            }
            return VoxelShapeImpl.fullCube();
        }
        Material type = getType();
        return type.isAir() ? VoxelShapeImpl.empty() : VoxelShapeImpl.fullCube();
    }

    public org.bukkit.util.BoundingBox getBoundingBox() {
        return customState() != null ? new org.bukkit.util.BoundingBox(0, 0, 0, 1, 1, 1)
                : new org.bukkit.util.BoundingBox(0, 0, 0, 1, 1, 1);
    }

    /** Дропы custom-блока через CE loot; для ванильного — сам блок. */
    public java.util.Collection<org.bukkit.inventory.ItemStack> getDrops(
            org.bukkit.inventory.ItemStack tool) {
        Object custom = customState();
        if (custom != null) {
            java.util.List<org.bukkit.inventory.ItemStack> out = new java.util.ArrayList<>();
            try {
                net.minecraft.world.level.storage.loot.LootParams.Builder builder =
                        new net.minecraft.world.level.storage.loot.LootParams.Builder(
                                (net.minecraft.server.level.ServerLevel) WorldImpl.of(world).getHandle());
                builder.withParameter(
                        net.minecraft.world.level.storage.loot.parameters.LootContextParams.ORIGIN,
                        new net.minecraft.world.phys.Vec3(x + 0.5, y + 0.5, z + 0.5));
                builder.withParameter(
                        net.minecraft.world.level.storage.loot.parameters.LootContextParams.TOOL,
                        org.bukkit.craftbukkit.inventory.CraftItemStack.asNMSCopy(tool));
                Object items = null;
                boolean found = false;
                for (java.lang.reflect.Method m : custom.getClass().getMethods()) {
                    if (m.getName().equals("getDrops") && m.getParameterCount() == 1
                            && m.getParameterTypes()[0].getName().endsWith("LootParams$Builder")) {
                        found = true;
                        items = m.invoke(custom, builder);
                        break;
                    }
                }
                if (!found) {
                    // ImmutableBlockState#getDrops(ContextHolder, World, Player):
                    // CE loot сам строит контекст для ore-дропа.
                    Object emptyContext = null;
                    try {
                        ClassLoader loader = custom.getClass().getClassLoader();
                        Class<?> context = Class.forName(
                                "net.momirealms.craftengine.core.plugin.context.ContextHolder",
                                true, loader);
                        emptyContext = context.getMethod("emptyImmutable").invoke(null);
                    } catch (Throwable ignored) {
                        // ниже null-context, CE может вернуть пусто
                    }
                    for (java.lang.reflect.Method m : custom.getClass().getMethods()) {
                        if (m.getName().equals("getDrops") && m.getParameterCount() == 3) {
                            found = true;
                            items = m.invoke(custom, emptyContext, null, null);
                            break;
                        }
                    }
                }
                if (items instanceof java.util.Collection<?> collection) {
                    for (Object item : collection) {
                        org.bukkit.inventory.ItemStack stack = bukkitItem(item);
                        if (stack != null && !stack.getType().isAir()) {
                            out.add(stack);
                        }
                    }
                }
            } catch (Throwable ignored) {
                // CE loot может требовать контекст игрока; не роняем API.
            }
            return out;
        }
        Material type = getType();
        return type.isAir() ? java.util.List.of()
                : java.util.List.of(new org.bukkit.inventory.ItemStack(type));
    }

    private static org.bukkit.inventory.ItemStack bukkitItem(Object item) {
        if (item == null) {
            return null;
        }
        if (item instanceof org.bukkit.inventory.ItemStack stack) {
            return stack;
        }
        try {
            java.lang.reflect.Method m = item.getClass().getMethod("getBukkitItem");
            Object value = m.invoke(item);
            if (value instanceof org.bukkit.inventory.ItemStack stack) {
                return stack;
            }
        } catch (Throwable ignored) {
            // fallback ниже
        }
        try {
            Object value = item.getClass().getMethod("platformItem").invoke(item);
            if (value instanceof org.bukkit.inventory.ItemStack stack) {
                return stack;
            }
        } catch (Throwable ignored) {
            // не Bukkit-предмет
        }
        return null;
    }

    public boolean breakNaturally(org.bukkit.inventory.ItemStack tool) {
        java.util.Collection<org.bukkit.inventory.ItemStack> drops = getDrops(tool);
        WorldImpl.request(world, "set " + x + " " + y + " " + z + " minecraft:air 1");
        WorldImpl impl = WorldImpl.of(world);
        if (impl != null) {
            CustomBlockStore.remove(impl.dim, new net.minecraft.core.BlockPos(x, y, z));
        }
        if (impl != null) {
            impl.getHandle().setBlock(new net.minecraft.core.BlockPos(x, y, z),
                    net.minecraft.world.level.block.Blocks.AIR.defaultBlockState(), 3);
        }
        Location dropAt = new Location(world, x + 0.5, y + 0.5, z + 0.5);
        for (org.bukkit.inventory.ItemStack stack : drops) {
            try {
                world.dropItemNaturally(dropAt, stack);
            } catch (Throwable ignored) {
                // мир мог быть уже выгружен
            }
        }
        return true;
    }

    /** Свет неба и блоков одной строкой `sky block`: BlueMap рисует по нему освещение. */
    private String light() {
        return WorldImpl.request(world, "light " + x + " " + y + " " + z);
    }

    public byte getLightFromSky() {
        String line = light();
        return line.isEmpty() ? 15 : Byte.parseByte(line.substring(0, line.indexOf(' ')));
    }

    public byte getLightFromBlocks() {
        String line = light();
        return line.isEmpty() ? 0 : Byte.parseByte(line.substring(line.indexOf(' ') + 1).trim());
    }

    public byte getLightLevel() {
        return (byte) Math.max(getLightFromSky(), getLightFromBlocks());
    }

    public void setBlockData(org.bukkit.block.data.BlockData data) { setBlockData(data, true); }

    public void setBlockData(org.bukkit.block.data.BlockData data, boolean physics) {
        ensureChunk();
        WorldImpl.request(world, "set " + x + " " + y + " " + z + " " + data.getAsString() + (physics ? " 1" : " 0"));
    }

    /** Снимок блока: у табличек — {@link org.bukkit.block.Sign}, иначе простой {@code BlockState}. */
    public org.bukkit.block.BlockState getState() {
        org.bukkit.block.Sign sign = SignImpl.of(api);
        return sign != null ? sign : Stub.of(org.bukkit.block.BlockState.class, new Events.StateImpl(api));
    }

    public org.bukkit.block.BlockState getState(boolean useSnapshot) { return getState(); }

    public void setMetadata(String k, org.bukkit.metadata.MetadataValue v) {
        Bridge.server.metadata().blocks.setMetadata(api, k, v);
    }
    public java.util.List<org.bukkit.metadata.MetadataValue> getMetadata(String k) {
        return Bridge.server.metadata().blocks.getMetadata(api, k);
    }
    public boolean hasMetadata(String k) { return Bridge.server.metadata().blocks.hasMetadata(api, k); }
    public void removeMetadata(String k, org.bukkit.plugin.Plugin owner) {
        Bridge.server.metadata().blocks.removeMetadata(api, k, owner);
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof BlockImpl b && b.world == world && b.x == x && b.y == y && b.z == z;
    }

    @Override
    public int hashCode() {
        return (x * 31 + y) * 31 + z;
    }

    @Override
    public String toString() {
        return "ChunkwakeBlock{" + key + " at " + x + "," + y + "," + z + "}";
    }
}
