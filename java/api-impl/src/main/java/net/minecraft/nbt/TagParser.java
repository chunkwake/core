package net.minecraft.nbt;

/** NMS {@code TagParser}: разбор SNBT/NBT-строк. */
public final class TagParser {
    private TagParser() {
    }

    /**
     * Разбор простого представления компаунда, которое отдаёт наш
     * {@link CompoundTag#toString()} (`{id=minecraft:stone, count=1}`) и
     * обычного SNBT (`{id:"minecraft:stone",count:1}`). Полный SNBT-парсер
     * не нужен: nightcore использует это для round-trip ItemStack.
     */
    public static CompoundTag parseCompoundFully(String input) {
        CompoundTag tag = new CompoundTag();
        if (input == null) {
            return tag;
        }
        String body = input.trim();
        if (body.startsWith("{")) {
            body = body.substring(1);
        }
        if (body.endsWith("}")) {
            body = body.substring(0, body.length() - 1);
        }
        if (body.isBlank()) {
            return tag;
        }
        for (String part : body.split(",")) {
            String entry = part.trim();
            int split = entry.indexOf('=');
            if (split < 0) {
                split = entry.indexOf(':');
            }
            if (split <= 0) {
                continue;
            }
            String key = entry.substring(0, split).trim();
            String value = entry.substring(split + 1).trim();
            if (value.length() >= 2
                    && (value.startsWith("\"") && value.endsWith("\"")
                            || value.startsWith("'") && value.endsWith("'"))) {
                value = value.substring(1, value.length() - 1);
            }
            try {
                tag.putInt(key, Integer.parseInt(value));
            } catch (NumberFormatException ignored) {
                tag.putString(key, value);
            }
        }
        return tag;
    }

    public static Tag parseTag(String input) {
        return parseCompoundFully(input);
    }
}
