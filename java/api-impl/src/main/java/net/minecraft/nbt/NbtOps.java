package net.minecraft.nbt;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonPrimitive;
import com.mojang.datafixers.util.Pair;
import com.mojang.serialization.DataResult;
import com.mojang.serialization.DynamicOps;
import com.mojang.serialization.JsonOps;
import java.util.stream.Stream;

/** Рабочий DFU {@code NbtOps} поверх NMS NBT-тегов. */
public final class NbtOps implements DynamicOps<Tag> {
    public static final NbtOps INSTANCE = new NbtOps();

    private NbtOps() {
    }

    @Override
    public Tag empty() {
        return new CompoundTag();
    }

    @Override
    @SuppressWarnings("unchecked")
    public <U> U convertTo(DynamicOps<U> outOps, Tag input) {
        if (outOps == this) {
            return (U) input;
        }
        return JsonOps.INSTANCE.convertTo(outOps, toJson(input));
    }

    @Override
    public DataResult<Number> getNumberValue(Tag input) {
        if (input instanceof ByteTag tag) {
            return DataResult.success(tag.getAsByte());
        }
        if (input instanceof ShortTag tag) {
            return DataResult.success(tag.getAsShort());
        }
        if (input instanceof IntTag tag) {
            return DataResult.success(tag.getAsInt());
        }
        if (input instanceof LongTag tag) {
            return DataResult.success(tag.getAsLong());
        }
        if (input instanceof FloatTag tag) {
            return DataResult.success(tag.getAsFloat());
        }
        if (input instanceof DoubleTag tag) {
            return DataResult.success(tag.getAsDouble());
        }
        return DataResult.error(() -> "Not a number: " + input);
    }

    @Override
    public Tag createNumeric(Number value) {
        return DoubleTag.valueOf(value.doubleValue());
    }

    @Override
    public DataResult<Boolean> getBooleanValue(Tag input) {
        return input instanceof ByteTag tag
                ? DataResult.success(tag.getAsBoolean())
                : DataResult.error(() -> "Not a boolean: " + input);
    }

    @Override
    public Tag createBoolean(boolean value) {
        return ByteTag.valueOf(value);
    }

    @Override
    public DataResult<String> getStringValue(Tag input) {
        return input instanceof StringTag tag
                ? DataResult.success(tag.getAsString())
                : DataResult.error(() -> "Not a string: " + input);
    }

    @Override
    public Tag createString(String value) {
        return StringTag.valueOf(value);
    }

    @Override
    public DataResult<Tag> mergeToList(Tag prefix, Tag value) {
        ListTag list;
        if (prefix instanceof ListTag existing) {
            list = new ListTag();
            list.addAll(existing);
        } else {
            list = new ListTag();
        }
        list.add(value);
        return DataResult.success(list);
    }

    @Override
    public DataResult<Tag> mergeToMap(Tag prefix, Tag key, Tag value) {
        CompoundTag map = prefix instanceof CompoundTag existing ? existing : new CompoundTag();
        if (key instanceof StringTag string) {
            map.put(string.getAsString(), value);
            return DataResult.success(map);
        }
        return DataResult.error(() -> "Key is not a string: " + key);
    }

    @Override
    public DataResult<Stream<Pair<Tag, Tag>>> getMapValues(Tag input) {
        if (!(input instanceof CompoundTag tag)) {
            return DataResult.error(() -> "Not a compound: " + input);
        }
        return DataResult.success(tag.keySet().stream()
                .map(key -> Pair.of(StringTag.valueOf(key), tag.get(key))));
    }

    @Override
    public Tag createMap(Stream<Pair<Tag, Tag>> map) {
        CompoundTag tag = new CompoundTag();
        map.forEach(entry -> {
            if (entry.getFirst() instanceof StringTag key) {
                tag.put(key.getAsString(), entry.getSecond());
            }
        });
        return tag;
    }

    @Override
    public DataResult<Stream<Tag>> getStream(Tag input) {
        return input instanceof ListTag list
                ? DataResult.success(list.stream())
                : DataResult.error(() -> "Not a list: " + input);
    }

    @Override
    public Tag createList(Stream<Tag> input) {
        ListTag list = new ListTag();
        input.forEach(list::add);
        return list;
    }

    @Override
    public Tag remove(Tag input, String key) {
        if (input instanceof CompoundTag tag) {
            CompoundTag copy = new CompoundTag();
            for (String existing : tag.keySet()) {
                if (!existing.equals(key)) {
                    copy.put(existing, tag.get(existing));
                }
            }
            return copy;
        }
        return input;
    }

    /** NBT → Gson (для convertTo). */
    public static JsonElement toJson(Tag tag) {
        if (tag instanceof CompoundTag compound) {
            JsonObject object = new JsonObject();
            for (String key : compound.keySet()) {
                object.add(key, toJson(compound.get(key)));
            }
            return object;
        }
        if (tag instanceof ListTag list) {
            JsonArray array = new JsonArray();
            for (Tag element : list) {
                array.add(toJson(element));
            }
            return array;
        }
        if (tag instanceof StringTag string) {
            return new JsonPrimitive(string.getAsString());
        }
        if (tag instanceof ByteTag number) {
            return new JsonPrimitive(number.getAsBoolean());
        }
        if (tag instanceof ShortTag number) {
            return new JsonPrimitive(number.getAsShort());
        }
        if (tag instanceof IntTag number) {
            return new JsonPrimitive(number.getAsInt());
        }
        if (tag instanceof LongTag number) {
            return new JsonPrimitive(number.getAsLong());
        }
        if (tag instanceof FloatTag number) {
            return new JsonPrimitive(number.getAsFloat());
        }
        if (tag instanceof DoubleTag number) {
            return new JsonPrimitive(number.getAsDouble());
        }
        return new JsonPrimitive(String.valueOf(tag));
    }

}
