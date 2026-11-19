import java.io.DataInputStream;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.zip.ZipEntry;
import java.util.zip.ZipInputStream;

/**
 * Патчит {@code com.mojang.brigadier.tree.CommandNode}: делает поле
 * {@code requirement} публичным, как в Paper. Плагины, собранные против
 * Paper (DreamDisplays), обращаются к нему напрямую; в ванильном Brigadier
 * оно private, отсюда IllegalAccessError.
 *
 * <p>Патчится только access_flags поля, байткод и раскладка класса не
 * меняются — подклассы из обычного brigadier.jar продолжают работать.
 *
 * <p>Аргументы: {@code <brigadier.jar> <выходной каталог>}.
 */
public final class PatchBrigadier {
    private static final int ACC_PRIVATE = 0x0002;
    private static final int ACC_PUBLIC = 0x0001;

    public static void main(String[] args) throws Exception {
        if (args.length != 2) {
            throw new IllegalArgumentException("usage: PatchBrigadier <brigadier.jar> <outdir>");
        }
        String target = "com/mojang/brigadier/tree/CommandNode.class";
        Path out = Path.of(args[1]).resolve(target);
        byte[] patched = null;
        try (ZipInputStream zip = new ZipInputStream(Files.newInputStream(Path.of(args[0])))) {
            for (ZipEntry entry; (entry = zip.getNextEntry()) != null; ) {
                if (!entry.getName().equals(target)) {
                    continue;
                }
                byte[] original = zip.readAllBytes();
                patched = patch(original, "requirement");
                break;
            }
        }
        if (patched == null) {
            throw new IllegalStateException("CommandNode.class not found in " + args[0]);
        }
        Files.createDirectories(out.getParent());
        Files.write(out, patched);
        System.out.println("patched " + target + " (" + patched.length + " bytes)");
    }

    /** Разбирает class-файл и меняет private на public у поля {@code fieldName}. */
    static byte[] patch(byte[] b, String fieldName) throws IOException {
        DataInputStream in = new DataInputStream(new java.io.ByteArrayInputStream(b));
        if (in.readInt() != 0xCAFEBABE) {
            throw new IOException("not a class file");
        }
        in.readUnsignedShort(); // minor
        in.readUnsignedShort(); // major
        int poolCount = in.readUnsignedShort();
        String[] utf8 = new String[poolCount];
        for (int i = 1; i < poolCount; i++) {
            int tag = in.readUnsignedByte();
            switch (tag) {
                case 1 -> utf8[i] = in.readUTF();
                case 3, 4 -> in.skipBytes(4);
                case 5, 6 -> {
                    in.skipBytes(8);
                    i++; // long/double занимают два слота
                }
                case 7, 8, 16, 19, 20 -> in.skipBytes(2);
                case 9, 10, 11, 12, 17, 18 -> in.skipBytes(4);
                case 15 -> in.skipBytes(3);
                default -> throw new IOException("unknown constant pool tag " + tag);
            }
        }
        in.readUnsignedShort(); // access
        in.readUnsignedShort(); // this
        in.readUnsignedShort(); // super
        int interfaces = in.readUnsignedShort();
        in.skipBytes(interfaces * 2);
        int fields = in.readUnsignedShort();
        int patchedOffset = -1;
        for (int f = 0; f < fields; f++) {
            int fieldStart = b.length - in.available();
            int access = in.readUnsignedShort();
            int nameIndex = in.readUnsignedShort();
            in.readUnsignedShort(); // descriptor
            int attributes = in.readUnsignedShort();
            for (int a = 0; a < attributes; a++) {
                in.readUnsignedShort();
                int length = in.readInt();
                in.skipBytes(length);
            }
            if (fieldName.equals(utf8[nameIndex])) {
                patchedOffset = fieldStart;
                b[patchedOffset] = (byte) ((access & ~ACC_PRIVATE | ACC_PUBLIC) >>> 8);
                b[patchedOffset + 1] = (byte) (access & ~ACC_PRIVATE | ACC_PUBLIC);
                break;
            }
        }
        if (patchedOffset < 0) {
            throw new IOException("field " + fieldName + " not found");
        }
        return b;
    }
}
