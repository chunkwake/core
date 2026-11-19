package dev.chunkwake.bridge;

import java.io.ByteArrayOutputStream;
import java.util.UUID;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ClientboundAddEntityPacket;
import net.minecraft.network.protocol.game.ClientboundRemoveEntitiesPacket;

/**
 * NMS {@link Packet} → сетевые байты для Rust-транспорта.
 *
 * <p>Нужен плагинам (DecentHolograms/TAB), которые отправляют пакеты через
 * NMS {@code Connection.send(Packet)}, а не через Bukkit-API.
 */
final class PacketWire {
    /** id пакетов протокола 26.3 (reports/packets.json). */
    private static final int ID_ADD_ENTITY = 1;
    private static final int ID_REMOVE_ENTITIES = 78;

    private PacketWire() {
    }

    /** {@code null} — пакет пока не поддержан. */
    static byte[] encode(Packet packet) {
        if (packet instanceof ClientboundAddEntityPacket add) {
            return addEntity(add);
        }
        if (packet instanceof ClientboundRemoveEntitiesPacket remove) {
            return removeEntities(remove);
        }
        return null;
    }

    static byte[] addEntity(ClientboundAddEntityPacket p) {
        Out out = new Out(64);
        out.varInt(ID_ADD_ENTITY);
        out.varInt(p.id());
        out.uuid(p.uuid());
        out.varInt(p.type());
        out.f64(p.x());
        out.f64(p.y());
        out.f64(p.z());
        out.u8(0); // LpVec3: нулевая скорость
        out.u8(p.pitch() & 0xFF);
        out.u8(p.yaw() & 0xFF);
        out.u8(p.headYaw() & 0xFF);
        out.varInt(p.data());
        return out.bytes();
    }

    static byte[] removeEntities(ClientboundRemoveEntitiesPacket p) {
        int[] ids = p.ids();
        Out out = new Out(16);
        out.varInt(ID_REMOVE_ENTITIES);
        out.varInt(ids.length);
        for (int id : ids) {
            out.varInt(id);
        }
        return out.bytes();
    }

    /** Минимальный writer сетевого формата. */
    private static final class Out {
        private final ByteArrayOutputStream buf;

        Out(int capacity) {
            this.buf = new ByteArrayOutputStream(capacity);
        }

        void u8(int v) {
            this.buf.write(v & 0xFF);
        }

        void varInt(int value) {
            while ((value & ~0x7F) != 0) {
                this.buf.write((value & 0x7F) | 0x80);
                value >>>= 7;
            }
            this.buf.write(value);
        }

        void f64(double v) {
            long bits = Double.doubleToLongBits(v);
            for (int shift = 56; shift >= 0; shift -= 8) {
                this.buf.write((int) (bits >>> shift) & 0xFF);
            }
        }

        void uuid(UUID id) {
            writeLong(id.getMostSignificantBits());
            writeLong(id.getLeastSignificantBits());
        }

        private void writeLong(long v) {
            for (int shift = 56; shift >= 0; shift -= 8) {
                this.buf.write((int) (v >>> shift) & 0xFF);
            }
        }

        byte[] bytes() {
            return this.buf.toByteArray();
        }
    }
}
