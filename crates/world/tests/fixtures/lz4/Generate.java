import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import net.jpountz.lz4.LZ4BlockInputStream;
import net.jpountz.lz4.LZ4BlockOutputStream;
import java.io.ByteArrayInputStream;

// Генератор независимых фикстур: lz4-java 1.8.0, без кода Chunkwake.
class Generate {
    public static void main(String[] args) throws Exception {
        Path dir = Path.of(args[0]);
        Files.createDirectories(dir);
        try (var out = new LZ4BlockOutputStream(Files.newOutputStream(dir.resolve("empty.lz4")), 1024)) {
        }
        byte[] payload = new byte[4096];
        int state = 0x12345678;
        for (int i = 0; i < payload.length; i++) {
            state ^= state << 13;
            state ^= state >>> 17;
            state ^= state << 5;
            payload[i] = i < 1536 ? (byte) state : (byte) (i / 7 % 13);
        }
        var buffer = new ByteArrayOutputStream();
        try (var nbt = new DataOutputStream(buffer)) {
            nbt.writeByte(10);
            nbt.writeUTF("");
            nbt.writeByte(7);
            nbt.writeUTF("payload");
            nbt.writeInt(payload.length);
            nbt.write(payload);
            nbt.writeByte(0);
        }
        byte[] raw = buffer.toByteArray();
        Files.write(dir.resolve("mixed.nbt"), raw);
        Path compressed = dir.resolve("mixed.lz4");
        try (var out = new LZ4BlockOutputStream(Files.newOutputStream(compressed), 1024)) {
            out.write(raw);
        }
        try (var in = new LZ4BlockInputStream(new ByteArrayInputStream(Files.readAllBytes(compressed)))) {
            if (!Arrays.equals(raw, in.readAllBytes())) throw new AssertionError("roundtrip");
        }
    }
}
