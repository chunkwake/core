package dev.chunkwake.bridge;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.util.UUID;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ClientboundAddEntityPacket;
import net.minecraft.network.protocol.game.ClientboundRemoveEntitiesPacket;
import net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket;
import net.minecraft.network.syncher.SynchedEntityData;

/**
 * NMS {@link Packet} → сетевые байты для Rust-транспорта.
 *
 * <p>Id пакетов и сериализаторов — протокол 26.3 (из reports/packets.json и
 * реального реестра EntityDataSerializers).
 */
final class PacketWire {
    private static final int ID_ADD_ENTITY = 1;
    private static final int ID_REMOVE_ENTITIES = 78;
    private static final int ID_SET_ENTITY_DATA = 101;
    private static final int ID_SET_PASSENGERS = 109;
    private static final int ID_TELEPORT_ENTITY = 128;

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
        if (packet instanceof ClientboundSetEntityDataPacket data && !data.values().isEmpty()) {
            return setEntityData(data);
        }
        if (packet instanceof net.minecraft.network.protocol.game.ClientboundTeleportEntityPacket tp) {
            return teleport(tp);
        }
        if (packet instanceof net.minecraft.network.protocol.game.ClientboundSetPassengersPacket ps) {
            return passengers(ps);
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

    static byte[] setEntityData(ClientboundSetEntityDataPacket p) {
        Out out = new Out(64);
        out.varInt(ID_SET_ENTITY_DATA);
        out.varInt(p.id());
        out.varInt(p.values().size());
        for (SynchedEntityData.DataValue<?> value : p.values()) {
            int serializer = value.serializer().id();
            out.varInt(value.id());
            out.varInt(serializer);
            if (!writeValue(out, serializer, value.value())) {
                return null;
            }
        }
        return out.bytes();
    }

    static byte[] teleport(
            net.minecraft.network.protocol.game.ClientboundTeleportEntityPacket p) {
        net.minecraft.world.entity.PositionMoveRotation change = p.change();
        net.minecraft.world.phys.Vec3 pos = change == null ? null : change.position();
        Out out = new Out(48);
        out.varInt(ID_TELEPORT_ENTITY);
        out.varInt(p.id());
        out.f64(pos == null ? 0.0 : pos.x());
        out.f64(pos == null ? 0.0 : pos.y());
        out.f64(pos == null ? 0.0 : pos.z());
        out.u8(0); // LpVec3: нулевая скорость
        out.f32(change == null ? 0f : change.yRot());
        out.f32(change == null ? 0f : change.xRot());
        out.bool(p.onGround());
        return out.bytes();
    }

    static byte[] passengers(
            net.minecraft.network.protocol.game.ClientboundSetPassengersPacket p) {
        int[] ids = p.getPassengers();
        Out out = new Out(16);
        out.varInt(ID_SET_PASSENGERS);
        out.varInt(p.getVehicle());
        out.varInt(ids.length);
        for (int id : ids) {
            out.varInt(id);
        }
        return out.bytes();
    }

    private static boolean writeValue(Out out, int serializer, Object value) {
        switch (serializer) {
            case 0 -> out.u8(value instanceof Byte b ? b : 0);
            case 1 -> out.varInt(value instanceof Integer i ? i : 0);
            case 3 -> out.f32(value instanceof Float f ? f : 0f);
            case 4 -> out.string(value == null ? "" : value.toString());
            case 5 -> out.component(value instanceof Component c ? c : null);
            case 6 -> {
                java.util.Optional<?> optional = value instanceof java.util.Optional<?> o
                        ? o : java.util.Optional.empty();
                out.bool(optional.isPresent());
                if (optional.isPresent()) {
                    out.component(optional.get() instanceof Component c ? c : null);
                }
            }
            case 10 -> out.bool(value instanceof Boolean b && b);
            case 21 -> {
                // Pose: enum ordinal как varint.
                Object pose = value;
                out.varInt(pose instanceof Enum<?> e ? e.ordinal() : 0);
            }
            case 39 -> {
                if (!writeFloats(out, value, 3)) {
                    return false;
                }
            }
            case 40 -> {
                if (!writeFloats(out, value, 4)) {
                    return false;
                }
            }
            default -> {
                return false;
            }
        }
        return true;
    }

    /** JOML-вектор/кватернион без зависимости от joml: x(), y(), z(), (w()). */
    private static boolean writeFloats(Out out, Object value, int count) {
        if (value == null) {
            return false;
        }
        String[] names = count == 3 ? new String[] {"x", "y", "z"}
                : new String[] {"x", "y", "z", "w"};
        float[] floats = new float[count];
        for (int i = 0; i < count; i++) {
            try {
                floats[i] = ((Number) value.getClass().getMethod(names[i]).invoke(value))
                        .floatValue();
            } catch (Throwable broken) {
                return false;
            }
        }
        for (float f : floats) {
            out.f32(f);
        }
        return true;
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

        void bool(boolean v) {
            this.buf.write(v ? 1 : 0);
        }

        void varInt(int value) {
            while ((value & ~0x7F) != 0) {
                this.buf.write((value & 0x7F) | 0x80);
                value >>>= 7;
            }
            this.buf.write(value);
        }

        void f32(float v) {
            int bits = Float.floatToIntBits(v);
            for (int shift = 24; shift >= 0; shift -= 8) {
                this.buf.write((bits >>> shift) & 0xFF);
            }
        }

        void f64(double v) {
            long bits = Double.doubleToLongBits(v);
            for (int shift = 56; shift >= 0; shift -= 8) {
                this.buf.write((int) (bits >>> shift) & 0xFF);
            }
        }

        void string(String value) {
            byte[] bytes = (value == null ? "" : value)
                    .getBytes(java.nio.charset.StandardCharsets.UTF_8);
            varInt(bytes.length);
            this.buf.write(bytes, 0, bytes.length);
        }

        void component(Component component) {
            net.minecraft.nbt.CompoundTag tag = new net.minecraft.nbt.CompoundTag();
            tag.putString("text", component == null ? "" : component.getString());
            try {
                net.minecraft.nbt.NbtIo.writeNetwork(tag, new DataOutputStream(this.buf));
            } catch (java.io.IOException broken) {
                throw new IllegalStateException(broken);
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
