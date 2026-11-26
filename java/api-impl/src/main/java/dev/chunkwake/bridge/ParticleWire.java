package dev.chunkwake.bridge;

import java.nio.ByteBuffer;
import java.util.Arrays;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.world.level.block.state.BlockState;
import org.bukkit.Color;
import org.bukkit.Particle;
import org.bukkit.block.data.BlockData;
import org.bukkit.craftbukkit.block.data.CraftBlockData;
import org.bukkit.inventory.ItemStack;

/**
 * Общий путь {@code World.spawnParticle}/{@code Player.spawnParticle} в Rust:
 * ключ {@link Particle}, позиция, разброс, скорость и необязательное тело
 * сложной частицы (hex-строка байтов сетевого кодека).
 */
final class ParticleWire {
    /** Частицы, которым нужны данные; без тела такой пакет клиент не разберёт. */
    private static final Set<String> NEEDS_DATA = Set.of(
            "dust",
            "dust_color_transition",
            "block",
            "block_crumble",
            "block_marker",
            "dust_pillar",
            "falling_dust",
            "dragon_breath",
            "effect",
            "entity_effect",
            "instant_effect",
            "flash",
            "item",
            "item_cobweb",
            "sculk_charge",
            "shriek",
            "tinted_leaves",
            "trail",
            "vibration",
            "geyser",
            "geyser_base",
            "geyser_plume",
            "geyser_poof");

    private static final Set<String> WARNED = ConcurrentHashMap.newKeySet();

    private ParticleWire() {
    }

    /** Команда для Rust; {@code null} — частицу/данные мост пока не умеет. */
    static String command(Particle particle, double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ, double speed,
            Object data, boolean force) {
        if (particle == null || count < 0
                || !Double.isFinite(x) || !Double.isFinite(y) || !Double.isFinite(z)
                || !Double.isFinite(offsetX) || !Double.isFinite(offsetY)
                || !Double.isFinite(offsetZ) || !Double.isFinite(speed)) {
            return null;
        }
        String body = body(particle, data);
        if (body == null) {
            return null;
        }
        StringBuilder out = new StringBuilder("particle ");
        out.append(particle.getKey()).append(' ')
                .append(x).append(' ').append(y).append(' ').append(z).append(' ')
                .append(count).append(' ')
                .append(offsetX).append(' ').append(offsetY).append(' ').append(offsetZ).append(' ')
                .append(speed).append(force ? " 1" : " 0");
        if (!body.isEmpty()) {
            out.append(' ').append(body);
        }
        return out.toString();
    }

    /** Тело частицы в hex; пустая строка — тело не нужно; {@code null} — не поддержано. */
    static String body(Particle particle, Object data) {
        if (data == null) {
            String path = particle.getKey().getKey();
            if (NEEDS_DATA.contains(path)) {
                unsupported(particle, "null data");
                return null;
            }
            return "";
        }
        if (data instanceof Particle.DustTransition transition) {
            ByteBuffer body = ByteBuffer.allocate(12);
            body.putInt(transition.getColor().asARGB());
            body.putInt(transition.getToColor().asARGB());
            body.putFloat(transition.getSize());
            return hex(body.array());
        }
        if (data instanceof Particle.DustOptions dust) {
            ByteBuffer body = ByteBuffer.allocate(8);
            body.putInt(dust.getColor().asARGB());
            body.putFloat(dust.getSize());
            return hex(body.array());
        }
        if (data instanceof BlockData block) {
            byte[] state = blockState(block);
            if (state == null) {
                unsupported(particle, "unknown block state");
                return null;
            }
            return hex(state);
        }
        if (data instanceof Color color) {
            return hex(ByteBuffer.allocate(4).putInt(color.asRGB()).array());
        }
        if (data instanceof Float value) {
            return hex(ByteBuffer.allocate(4).putFloat(value).array());
        }
        if (data instanceof Integer value) {
            return hex(varInt(value));
        }
        if (data instanceof ItemStack) {
            // ItemParticleOption в текущем протоколе несёт ItemStackTemplate
            // с реестровым id предмета и патчем компонентов; Java-мост пока
            // не отдаёт id предмета, поэтому честно пропускаем без пакета.
            unsupported(particle, "ItemStack data");
            return null;
        }
        unsupported(particle, data.getClass().getSimpleName());
        return null;
    }

    /** `BlockParticleOption`: VarInt сетевого id состояния блока. */
    private static byte[] blockState(BlockData data) {
        BlockState state = null;
        try {
            BlockStateParser.BlockResult parsed =
                    BlockStateParser.parseForBlock(null, data.getAsString(), false);
            if (parsed != null) {
                state = parsed.blockState;
            }
        } catch (Throwable ignored) {
            // Ниже пробуем NMS-состояние самого BlockData.
        }
        if (state == null) {
            state = CraftBlockData.getState(data);
        }
        if (state == null || state.id() < 0) {
            return null;
        }
        return varInt(state.id());
    }

    private static void unsupported(Particle particle, String data) {
        String name = particle.getKey().toString();
        if (WARNED.add(name + '/' + data)) {
            Bridge.log(2, "spawnParticle: " + name + " data=" + data
                    + " не поддержано мостом; пакет не отправлен");
        }
    }

    private static byte[] varInt(int value) {
        byte[] out = new byte[5];
        int length = 0;
        while ((value & ~0x7F) != 0) {
            out[length++] = (byte) ((value & 0x7F) | 0x80);
            value >>>= 7;
        }
        out[length++] = (byte) value;
        return Arrays.copyOf(out, length);
    }

    private static String hex(byte[] bytes) {
        char[] out = new char[bytes.length * 2];
        final char[] digits = "0123456789abcdef".toCharArray();
        for (int i = 0; i < bytes.length; i++) {
            int value = bytes[i] & 0xFF;
            out[i * 2] = digits[value >>> 4];
            out[i * 2 + 1] = digits[value & 0x0F];
        }
        return new String(out);
    }
}
