package dev.chunkwake.bridge;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.UUID;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.serializer.gson.GsonComponentSerializer;
import net.kyori.adventure.text.serializer.legacy.LegacyComponentSerializer;
import org.bukkit.DyeColor;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.block.HangingSign;
import org.bukkit.block.Sign;
import org.bukkit.block.sign.Side;
import org.bukkit.block.sign.SignSide;
import org.bukkit.entity.Player;

/**
 * {@link Sign} и {@link HangingSign}: снимок блок-сущности таблички, как
 * {@code CraftSign}. Правки живут в снимке до {@code update()}, который пишет
 * текст, цвет, свечение и воск в Rust (запрос {@code sign x y z}).
 */
final class SignImpl {
    private static final GsonComponentSerializer GSON = GsonComponentSerializer.gson();
    private static final LegacyComponentSerializer LEGACY = LegacyComponentSerializer.legacySection();

    final Sign api;
    private final Block block;
    private final boolean hanging;
    private boolean waxed;
    private UUID editor;
    private final SideImpl front;
    private final SideImpl back;

    private SignImpl(Block block, JsonObject o) {
        this.block = block;
        this.hanging = o.get("hanging").getAsBoolean();
        this.waxed = o.get("waxed").getAsBoolean();
        JsonElement e = o.get("editor");
        this.editor = e == null || e.isJsonNull() ? null : UUID.fromString(e.getAsString());
        this.front = new SideImpl(o.getAsJsonObject("front"));
        this.back = new SideImpl(o.getAsJsonObject("back"));
        this.api = hanging ? Stub.of(HangingSign.class, this) : Stub.of(Sign.class, this);
    }

    /** Табличка в этом блоке; не табличка — {@code null}. */
    static Sign of(Block block) {
        String json = WorldImpl.request(block.getWorld(), "sign " + block.getX() + " " + block.getY() + " " + block.getZ());
        return of(block, json);
    }

    /** Снимок из ответа Rust ({@code sign x y z}); пусто — {@code null}. */
    static Sign of(Block block, String json) {
        if (json == null || json.isEmpty()) {
            return null;
        }
        return new SignImpl(block, JsonParser.parseString(json).getAsJsonObject()).api;
    }

    static Component parse(String json) {
        return GSON.deserialize(json);
    }

    static String json(Component c) {
        return GSON.serialize(c == null ? Component.empty() : c);
    }

    /** Одна сторона таблички. */
    final class SideImpl {
        final SignSide api = Stub.of(SignSide.class, this);
        private final List<Component> lines = new ArrayList<>(4);
        private DyeColor color;
        private boolean glowing;

        SideImpl(JsonObject o) {
            JsonArray l = o.getAsJsonArray("lines");
            for (int i = 0; i < 4; i++) {
                lines.add(i < l.size() ? GSON.deserializeFromTree(l.get(i)) : Component.empty());
            }
            color = DyeColor.valueOf(o.get("color").getAsString().toUpperCase(Locale.ROOT));
            glowing = o.get("glowing").getAsBoolean();
        }

        JsonObject toJson() {
            JsonObject o = new JsonObject();
            JsonArray l = new JsonArray();
            for (Component c : lines) {
                l.add(GSON.serializeToTree(c));
            }
            o.add("lines", l);
            o.addProperty("color", color.name().toLowerCase(Locale.ROOT));
            o.addProperty("glowing", glowing);
            return o;
        }

        public List<Component> lines() { return lines; }
        public Component line(int i) { return lines.get(i); }
        public void line(int i, Component c) {
            lines.set(i, c == null ? Component.empty() : c);
        }
        public String[] getLines() {
            String[] out = new String[4];
            for (int i = 0; i < 4; i++) out[i] = getLine(i);
            return out;
        }
        public String getLine(int i) { return LEGACY.serialize(lines.get(i)); }
        public void setLine(int i, String s) { line(i, s == null ? Component.empty() : LEGACY.deserialize(s)); }
        public boolean isGlowingText() { return glowing; }
        public void setGlowingText(boolean g) { glowing = g; }
        public DyeColor getColor() { return color; }
        public void setColor(DyeColor c) { color = c == null ? DyeColor.BLACK : c; }
    }

    // Лицевая сторона — устаревшие методы `Sign`, как в `CraftSign`.
    public List<Component> lines() { return front.lines(); }
    public Component line(int i) { return front.line(i); }
    public void line(int i, Component c) { front.line(i, c); }
    public String[] getLines() { return front.getLines(); }
    public String getLine(int i) { return front.getLine(i); }
    public void setLine(int i, String s) { front.setLine(i, s); }
    public boolean isGlowingText() { return front.isGlowingText(); }
    public void setGlowingText(boolean g) { front.setGlowingText(g); }
    public DyeColor getColor() { return front.getColor(); }
    public void setColor(DyeColor c) { front.setColor(c); }

    public boolean isEditable() { return !waxed; }
    public void setEditable(boolean e) { waxed = !e; }
    public boolean isWaxed() { return waxed; }
    public void setWaxed(boolean w) { waxed = w; }

    public SignSide getSide(Side side) {
        return (side == Side.BACK ? back : front).api;
    }

    public Side getInteractableSideFor(double x, double z) {
        String r = WorldImpl.request(block.getWorld(), "sign " + block.getX() + " " + block.getY() + " " + block.getZ()
                + " side " + x + " " + z);
        return "BACK".equals(r) ? Side.BACK : Side.FRONT;
    }

    public SignSide getTargetSide(Player p) {
        Location l = p.getLocation();
        return getSide(getInteractableSideFor(l.getX(), l.getZ()));
    }

    public UUID getAllowedEditorUniqueId() { return editor; }
    public void setAllowedEditorUniqueId(UUID id) { editor = id; }
    public Player getAllowedEditor() { return editor == null ? null : Bridge.server.getPlayer(editor); }

    // BlockState.
    public Block getBlock() { return block; }
    public Material getType() { return block.getType(); }
    public org.bukkit.block.data.BlockData getBlockData() { return block.getBlockData(); }
    public World getWorld() { return block.getWorld(); }
    public int getX() { return block.getX(); }
    public int getY() { return block.getY(); }
    public int getZ() { return block.getZ(); }
    public Location getLocation() { return block.getLocation(); }
    public Location getLocation(Location l) {
        if (l != null) {
            l.setWorld(block.getWorld());
            l.set(block.getX(), block.getY(), block.getZ());
            l.setYaw(0);
            l.setPitch(0);
        }
        return l;
    }
    public boolean isPlaced() { return true; }
    public boolean isSnapshot() { return true; }
    public boolean update() { return update(false, true); }
    public boolean update(boolean force) { return update(force, true); }

    /** Пишет снимок в мир; табличка исчезла — {@code false}, как у {@code CraftBlockEntityState}. */
    public boolean update(boolean force, boolean physics) {
        JsonObject o = new JsonObject();
        o.addProperty("waxed", waxed);
        o.add("front", front.toJson());
        o.add("back", back.toJson());
        String r = WorldImpl.request(block.getWorld(), "sign " + block.getX() + " " + block.getY() + " " + block.getZ()
                + "\n" + o);
        return "1".equals(r);
    }

    @Override
    public String toString() {
        return "CraftSign{" + block + "}";
    }
}
