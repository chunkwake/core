package dev.chunkwake.bridge;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import org.bukkit.Bukkit;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.Registry;
import org.bukkit.block.Biome;
import org.bukkit.block.data.BlockData;

/** Снимок чанка, собранный из бинарного ответа Rust (`chunksnapshot`). */
final class ChunkSnapshotImpl implements org.bukkit.ChunkSnapshot {
    private static final int MAGIC = 0x31535743; // "CWS1" little-endian
    private static final Map<String, Integer> LEGACY_DATA = loadLegacyData();

    /** `CraftMagicNumbers.toLegacyData`: таблица снята с Paper 26.3 build 248. */
    private static Map<String, Integer> loadLegacyData() {
        try (var in = ChunkSnapshotImpl.class.getResourceAsStream("/legacy_data.tsv")) {
            if (in == null) {
                return Map.of();
            }
            Map<String, Integer> out = new HashMap<>(40000);
            BufferedReader reader = new BufferedReader(new InputStreamReader(in, StandardCharsets.UTF_8));
            String line;
            while ((line = reader.readLine()) != null) {
                int tab = line.lastIndexOf('\t');
                if (tab <= 0) {
                    continue;
                }
                try {
                    out.put(line.substring(0, tab), Integer.parseInt(line.substring(tab + 1)));
                } catch (NumberFormatException ignored) {
                    // Битый ресурс — пропускаем строку.
                }
            }
            return out;
        } catch (Exception e) {
            return Map.of();
        }
    }

    private final WorldImpl world;
    private final int x;
    private final int z;
    private final int minY;
    private final int height;
    private final int sections;
    private final int flags;
    private final long fullTime;
    private final BlockData[] blocks;
    private final short[] blockIndex;
    private final Biome[] biomes;
    private final double[] temperatures;
    private final short[] biomeIndex;
    private final double[] blockTemperatures;
    private final byte[] skyLight;
    private final byte[] blockLight;
    private final short[] heightmap;
    private final Set<String> blockPalette = new HashSet<>();
    private final Set<String> biomeKeys = new HashSet<>();

    private ChunkSnapshotImpl(WorldImpl world, ByteBuffer b) {
        this.world = world;
        if (b.getInt() != MAGIC) {
            throw new IllegalArgumentException("bad chunk snapshot magic");
        }
        this.x = b.getInt();
        this.z = b.getInt();
        this.minY = b.getInt();
        this.height = b.getInt();
        this.fullTime = b.getLong();
        this.flags = b.getInt();
        this.sections = Math.max(0, this.height / 16);

        int blockCount = Short.toUnsignedInt(b.getShort());
        this.blocks = new BlockData[blockCount];
        for (int i = 0; i < blockCount; i++) {
            int len = Short.toUnsignedInt(b.getShort());
            byte[] name = new byte[len];
            b.get(name);
            BlockData data;
            try {
                data = BlockDataImpl.parse(new String(name, StandardCharsets.UTF_8));
            } catch (RuntimeException e) {
                data = Material.AIR.createBlockData();
            }
            this.blocks[i] = data;
            this.blockPalette.add(canonical(data));
        }
        this.blockIndex = new short[Math.max(0, this.height * 256)];
        for (int i = 0; i < this.blockIndex.length; i++) {
            this.blockIndex[i] = b.getShort();
        }

        int biomeCount = Short.toUnsignedInt(b.getShort());
        this.biomes = new Biome[biomeCount];
        this.temperatures = new double[biomeCount];
        for (int i = 0; i < biomeCount; i++) {
            int len = Short.toUnsignedInt(b.getShort());
            byte[] name = new byte[len];
            b.get(name);
            String key = new String(name, StandardCharsets.UTF_8);
            this.biomes[i] = biome(key);
            this.temperatures[i] = b.getFloat();
            if (this.biomes[i] != null) {
                this.biomeKeys.add(this.biomes[i].getKey().toString());
            }
        }
        int cells = (this.flags & 0b110) == 0 ? 0 : this.sections * 64;
        this.biomeIndex = new short[cells];
        for (int i = 0; i < cells; i++) {
            this.biomeIndex[i] = b.getShort();
        }

        int tempCount = (this.flags & 0b110) == 0 ? 0 : this.height * 256;
        this.blockTemperatures = new double[tempCount];
        for (int i = 0; i < tempCount; i++) {
            this.blockTemperatures[i] = b.getFloat();
        }

        int lightBytes = (this.flags & 0b1000) == 0 ? 0 : this.sections * 2048;
        this.skyLight = new byte[lightBytes];
        this.blockLight = new byte[lightBytes];
        // Rust пишет послойно: sky[section], block[section], ...
        for (int si = 0; si < this.sections && lightBytes > 0; si++) {
            b.get(this.skyLight, si * 2048, 2048);
            b.get(this.blockLight, si * 2048, 2048);
        }

        int heightCount = (this.flags & 0b1) == 0 ? 0 : 256;
        this.heightmap = new short[heightCount];
        for (int i = 0; i < heightCount; i++) {
            this.heightmap[i] = b.getShort();
        }
    }

    static org.bukkit.ChunkSnapshot parse(WorldImpl world, int x, int z, int flags,
            byte[] compressed) {
        byte[] raw = inflate(compressed);
        ChunkSnapshotImpl snapshot = new ChunkSnapshotImpl(world,
                ByteBuffer.wrap(raw).order(ByteOrder.LITTLE_ENDIAN));
        if (snapshot.x != x || snapshot.z != z) {
            throw new IllegalArgumentException("chunk snapshot coordinates mismatch");
        }
        return snapshot;
    }

    private static byte[] inflate(byte[] compressed) {
        try {
            java.util.zip.Inflater inflater = new java.util.zip.Inflater();
            inflater.setInput(compressed);
            java.io.ByteArrayOutputStream out = new java.io.ByteArrayOutputStream(
                    Math.max(4096, compressed.length * 4));
            byte[] buf = new byte[8192];
            while (!inflater.finished()) {
                int n = inflater.inflate(buf);
                if (n == 0 && inflater.needsInput()) {
                    break;
                }
                out.write(buf, 0, n);
            }
            inflater.end();
            return out.toByteArray();
        } catch (java.util.zip.DataFormatException e) {
            throw new IllegalArgumentException("bad chunk snapshot payload", e);
        }
    }

    private static Biome biome(String key) {
        String path = key.contains(":") ? key.substring(key.indexOf(':') + 1) : key;
        Biome value = Registry.BIOME.get(NamespacedKey.minecraft(path));
        if (value != null) {
            return value;
        }
        try {
            return Biome.valueOf(path.toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException e) {
            return null;
        }
    }

    private static String canonical(BlockData data) {
        String s = data.getAsString();
        int open = s.indexOf('[');
        if (open < 0) {
            return s;
        }
        String name = s.substring(0, open);
        String body = s.substring(open + 1, s.length() - 1);
        if (body.isEmpty()) {
            return name;
        }
        String[] props = body.split(",");
        Arrays.sort(props);
        return name + "[" + String.join(",", props) + "]";
    }

    private int offset(int bx, int by, int bz) {
        if (bx < 0 || bx > 15 || bz < 0 || bz > 15) {
            throw new IllegalArgumentException("x/z out of range: " + bx + "/" + bz);
        }
        int ry = by - this.minY;
        if (ry < 0 || ry >= this.height) {
            throw new IllegalArgumentException("y out of range: " + by);
        }
        return (ry * 16 + bz) * 16 + bx;
    }

    private BlockData block(int bx, int by, int bz) {
        return this.blocks[Short.toUnsignedInt(this.blockIndex[offset(bx, by, bz)])];
    }

    private int light(byte[] layer, int bx, int by, int bz) {
        if (layer.length == 0) {
            return 0;
        }
        int ry = by - this.minY;
        int si = ry >> 4;
        int i = (ry & 15) * 256 + bz * 16 + bx;
        int v = layer[si * 2048 + (i >> 1)] & 0xFF;
        return (v >> ((i & 1) * 4)) & 15;
    }

    private int biomeCell(int bx, int by, int bz) {
        int ry = by - this.minY;
        int si = ry >> 4;
        int cy = (ry & 15) >> 2;
        return si * 64 + cy * 16 + (bz >> 2) * 4 + (bx >> 2);
    }

    @Override public int getX() { return this.x; }

    @Override public int getZ() { return this.z; }

    @Override public String getWorldName() { return this.world.api.getName(); }

    @Override public net.kyori.adventure.key.Key getWorldKey() {
        NamespacedKey key = this.world.api.getKey();
        return net.kyori.adventure.key.Key.key(key.getNamespace(), key.getKey());
    }

    @Override public Material getBlockType(int x, int y, int z) {
        return block(x, y, z).getMaterial();
    }

    @Override public BlockData getBlockData(int x, int y, int z) {
        return block(x, y, z);
    }

    @Override public int getData(int x, int y, int z) {
        offset(x, y, z);
        return LEGACY_DATA.getOrDefault(canonical(block(x, y, z)), 0);
    }

    @Override public int getBlockSkyLight(int x, int y, int z) {
        requireLight();
        offset(x, y, z);
        return light(this.skyLight, x, y, z);
    }

    @Override public int getBlockEmittedLight(int x, int y, int z) {
        requireLight();
        offset(x, y, z);
        return light(this.blockLight, x, y, z);
    }

    @Override public int getHighestBlockYAt(int x, int z) {
        if ((this.flags & 0b1) == 0 || this.heightmap.length == 0) {
            throw new IllegalStateException("ChunkSnapshot created without height map. "
                    + "Please call getSnapshot with includeMaxblocky=true");
        }
        if (x < 0 || x > 15 || z < 0 || z > 15) {
            throw new IllegalArgumentException("x/z out of range: " + x + "/" + z);
        }
        return this.heightmap[z * 16 + x];
    }

    private void requireLight() {
        if ((this.flags & 0b1000) == 0) {
            throw new IllegalStateException("ChunkSnapshot created without light data. "
                    + "Please call getSnapshot with includeLightData=true");
        }
    }

    private void requireBiome() {
        if ((this.flags & 0b10) == 0) {
            throw new IllegalStateException("ChunkSnapshot created without biome. "
                    + "Please call getSnapshot with includeBiome=true");
        }
    }

    @Override public Biome getBiome(int x, int z) {
        // Paper `CraftChunkSnapshot.getBiome(x, z)` = `getBiome(x, 0, z)`.
        return getBiome(x, 0, z);
    }

    @Override public Biome getBiome(int x, int y, int z) {
        requireBiome();
        offset(x, y, z);
        return this.biomes[Short.toUnsignedInt(this.biomeIndex[biomeCell(x, y, z)])];
    }

    @Override public double getRawBiomeTemperature(int x, int z) {
        // Paper `CraftChunkSnapshot.getRawBiomeTemperature(x, z)` = y=0.
        return getRawBiomeTemperature(x, 0, z);
    }

    @Override public double getRawBiomeTemperature(int x, int y, int z) {
        if ((this.flags & 0b110) == 0) {
            throw new IllegalStateException("ChunkSnapshot created without biome. "
                    + "Please call getSnapshot with includeBiome=true");
        }
        int index = offset(x, y, z);
        return this.blockTemperatures.length == 0 ? 0.0 : this.blockTemperatures[index];
    }

    @Override public long getCaptureFullTime() {
        return this.fullTime;
    }

    @Override public boolean isSectionEmpty(int sy) {
        // Paper `CraftChunkSnapshot.isSectionEmpty` индексирует массив секций
        // напрямую (0 = нижняя секция), несмотря на абсолютный Y в javadoc.
        if (sy < 0 || sy >= this.sections || this.blockIndex.length == 0) {
            return true;
        }
        int start = sy * 4096;
        int end = Math.min(this.blockIndex.length, start + 4096);
        for (int i = start; i < end; i++) {
            if (!this.blocks[Short.toUnsignedInt(this.blockIndex[i])].getMaterial().isAir()) {
                return false;
            }
        }
        return true;
    }

    @Override public boolean contains(BlockData data) {
        if (data == null) {
            throw new IllegalArgumentException("Block cannot be null");
        }
        return this.blockPalette.contains(canonical(data));
    }

    @Override public boolean contains(Biome biome) {
        if (biome == null) {
            throw new IllegalArgumentException("Biome cannot be null");
        }
        requireBiome();
        return this.biomeKeys.contains(biome.getKey().toString());
    }

    @Override public String toString() {
        return "ChunkSnapshot{" + this.x + "," + this.z + "}";
    }
}
