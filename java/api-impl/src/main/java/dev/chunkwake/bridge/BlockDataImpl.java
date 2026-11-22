package dev.chunkwake.bridge;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import org.bukkit.Material;
import org.bukkit.block.BlockFace;
import org.bukkit.block.data.BlockData;

/**
 * {@link BlockData} над строкой состояния {@code minecraft:имя[ключ=значение,…]}.
 * Прокси получает интерфейсы по свойствам ({@code facing} → {@code Directional},
 * {@code powered} → {@code Powerable}…), так что приведения плагинов работают.
 */
public final class BlockDataImpl {
    private final String name;
    private final Map<String, String> props;
    private final net.minecraft.world.level.block.state.BlockState state;

    private BlockDataImpl(String name, Map<String, String> props) {
        this(name, props, null);
    }

    private BlockDataImpl(String name, Map<String, String> props,
            net.minecraft.world.level.block.state.BlockState state) {
        this.name = name;
        this.props = props;
        this.state = state;
    }

    /** BlockData поверх реального NMS-состояния (CE BlockStateBase#asBlockData). */
    public static BlockData fromState(net.minecraft.world.level.block.state.BlockState state) {
        if (state == null) {
            return parse("minecraft:air");
        }
        Map<String, String> props = new LinkedHashMap<>();
        for (Map.Entry<net.minecraft.world.level.block.state.properties.Property<?>, Comparable<?>> entry
                : state.getValues().entrySet()) {
            props.put(entry.getKey().getName(),
                    entry.getValue().toString().toLowerCase(Locale.ROOT));
        }
        return new BlockDataImpl(state.getBlock().id(), props, state).api();
    }

    /** Из строки моста; пустая строка — воздух. */
    public static BlockData parse(String s) {
        if (s == null || s.isEmpty()) {
            s = "minecraft:air";
        }
        Map<String, String> props = new LinkedHashMap<>();
        String name = s;
        int open = s.indexOf('[');
        if (open >= 0) {
            name = s.substring(0, open);
            for (String kv : s.substring(open + 1, s.length() - 1).split(",")) {
                int eq = kv.indexOf('=');
                if (eq > 0) {
                    props.put(kv.substring(0, eq).trim(), kv.substring(eq + 1).trim());
                }
            }
        }
        return new BlockDataImpl(name, props).api();
    }

    private BlockData api() {
        List<Class<?>> ifaces = new ArrayList<>();
        String path = name.substring(name.indexOf(':') + 1);
        if (path.equals("piston") || path.equals("sticky_piston")) {
            ifaces.add(org.bukkit.block.data.type.Piston.class);
        }
        if (path.equals("observer")) {
            ifaces.add(org.bukkit.block.data.type.Observer.class);
        }
        // Dispenser/Dropper: WorldGuard и другие плагины кастуют BlockData
        // к конкретному интерфейсу, а не к общему BlockData.
        if (path.endsWith("dispenser") || path.endsWith("dropper")) {
            ifaces.add(org.bukkit.block.data.type.Dispenser.class);
        }
        if (props.containsKey("facing")) ifaces.add(org.bukkit.block.data.Directional.class);
        if (props.containsKey("powered")) ifaces.add(org.bukkit.block.data.Powerable.class);
        if (props.containsKey("open")) ifaces.add(org.bukkit.block.data.Openable.class);
        if (props.containsKey("age")) ifaces.add(org.bukkit.block.data.Ageable.class);
        if (props.containsKey("level")) ifaces.add(org.bukkit.block.data.Levelled.class);
        if (props.containsKey("waterlogged")) ifaces.add(org.bukkit.block.data.Waterlogged.class);
        if (props.containsKey("axis")) ifaces.add(org.bukkit.block.data.Orientable.class);
        if (props.containsKey("lit")) ifaces.add(org.bukkit.block.data.Lightable.class);
        if (props.containsKey("power")) ifaces.add(org.bukkit.block.data.AnaloguePowerable.class);
        ifaces.add(BlockData.class);
        ifaces.add(org.bukkit.craftbukkit.block.data.CraftBlockData.class);
        return (BlockData) Stub.ofAll(ifaces.toArray(new Class<?>[0]), this);
    }

    public net.minecraft.world.level.block.state.BlockState getState() {
        if (state != null) {
            return state;
        }
        net.minecraft.world.level.block.Block block =
                net.minecraft.core.registries.BuiltInRegistries.BLOCK.getValue(
                        net.minecraft.resources.Identifier.parse(name));
        if (block == null) {
            block = new net.minecraft.world.level.block.Block(name);
        }
        return block.defaultBlockState();
    }

    public Material getMaterial() {
        // CE deceive-bukkit-material (config.yml, default: bricks).
        if (name.startsWith("craftengine:")) {
            return Material.BRICKS;
        }
        Material m = Material.matchMaterial(name);
        return m == null ? Material.AIR : m;
    }

    public String getAsString() {
        if (props.isEmpty()) {
            return name;
        }
        StringBuilder b = new StringBuilder(name).append('[');
        props.forEach((k, v) -> b.append(k).append('=').append(v).append(','));
        b.setCharAt(b.length() - 1, ']');
        return b.toString();
    }

    public String getAsString(boolean hideUnspecified) { return getAsString(); }

    public BlockData clone() { return new BlockDataImpl(name, new LinkedHashMap<>(props)).api(); }

    public BlockData merge(BlockData other) {
        BlockDataImpl o = (BlockDataImpl) Stub.impl(other);
        Map<String, String> p = new LinkedHashMap<>(props);
        if (o != null) {
            o.props.forEach((k, v) -> {
                if (p.containsKey(k)) {
                    p.put(k, v);
                }
            });
        }
        return new BlockDataImpl(name, p).api();
    }

    public boolean matches(BlockData other) {
        BlockDataImpl o = (BlockDataImpl) Stub.impl(other);
        return o != null && o.name.equals(name)
                && o.props.entrySet().stream().allMatch(e -> e.getValue().equals(props.get(e.getKey())));
    }

    private boolean flag(String k) { return "true".equals(props.get(k)); }
    private void flag(String k, boolean v) { props.put(k, Boolean.toString(v)); }
    private int num(String k) { return Integer.parseInt(props.getOrDefault(k, "0")); }

    public BlockFace getFacing() { return BlockFace.valueOf(props.get("facing").toUpperCase(Locale.ROOT)); }
    public void setFacing(BlockFace f) { props.put("facing", f.name().toLowerCase(Locale.ROOT)); }

    /** {@code org.bukkit.block.data.type.Dispenser#isTriggered}. */
    public boolean isTriggered() { return flag("triggered"); }

    /** {@code org.bukkit.block.data.type.Dispenser#setTriggered}. */
    public void setTriggered(boolean triggered) { flag("triggered", triggered); }

    public Set<BlockFace> getFaces() {
        return props.containsKey("facing")
                ? (name.endsWith("piston") || name.endsWith("observer") || name.endsWith("dispenser") || name.endsWith("dropper"))
                        ? Set.of(BlockFace.NORTH, BlockFace.SOUTH, BlockFace.WEST, BlockFace.EAST, BlockFace.UP, BlockFace.DOWN)
                        : Set.of(BlockFace.NORTH, BlockFace.SOUTH, BlockFace.WEST, BlockFace.EAST)
                : Set.of();
    }

    public boolean isPowered() { return flag("powered"); }
    public void setPowered(boolean v) { flag("powered", v); }
    public boolean isOpen() { return flag("open"); }
    public void setOpen(boolean v) { flag("open", v); }
    public boolean isExtended() { return flag("extended"); }
    public void setExtended(boolean v) { flag("extended", v); }
    public boolean isWaterlogged() { return flag("waterlogged"); }
    public void setWaterlogged(boolean v) { flag("waterlogged", v); }
    public boolean isLit() { return flag("lit"); }
    public void setLit(boolean v) { flag("lit", v); }
    public int getAge() { return num("age"); }
    public void setAge(int v) { props.put("age", Integer.toString(v)); }
    public int getLevel() { return num("level"); }
    public void setLevel(int v) { props.put("level", Integer.toString(v)); }
    public int getPower() { return num("power"); }
    public void setPower(int v) { props.put("power", Integer.toString(v)); }
    public int getMaximumPower() { return 15; }

    public org.bukkit.Axis getAxis() { return org.bukkit.Axis.valueOf(props.get("axis").toUpperCase(Locale.ROOT)); }
    public void setAxis(org.bukkit.Axis a) { props.put("axis", a.name().toLowerCase(Locale.ROOT)); }

    @Override
    public String toString() { return "CraftBlockData{" + getAsString() + "}"; }
}
