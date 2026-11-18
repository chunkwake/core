package net.minecraft.world.level.storage;

import java.util.Optional;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.ByteTag;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.DoubleTag;
import net.minecraft.nbt.FloatTag;
import net.minecraft.nbt.IntTag;
import net.minecraft.nbt.LongTag;
import net.minecraft.nbt.ShortTag;
import net.minecraft.nbt.StringTag;
import net.minecraft.nbt.Tag;
import net.minecraft.util.ProblemReporter;

/** Рабочий NMS {@code TagValueInput} поверх {@link CompoundTag}. */
public class TagValueInput implements ValueInput {
    /** Публичное поле: его читает InvSee (FakeCraftPlayer). */
    public final CompoundTag input;

    private TagValueInput(CompoundTag input) {
        this.input = input;
    }

    public static ValueInput create(ProblemReporter reporter, HolderLookup.Provider registries,
            CompoundTag input) {
        return new TagValueInput(input == null ? new CompoundTag() : input);
    }

    @Override
    public Optional<ValueInput> child(String key) {
        Tag child = this.input.get(key);
        return child instanceof CompoundTag compound
                ? Optional.of(new TagValueInput(compound))
                : Optional.empty();
    }

    @Override
    public Optional<Byte> getByte(String key) {
        return this.input.get(key) instanceof ByteTag tag
                ? Optional.of(tag.getAsByte())
                : Optional.empty();
    }

    @Override
    public Optional<Boolean> getBoolean(String key) {
        return this.input.get(key) instanceof ByteTag tag
                ? Optional.of(tag.getAsBoolean())
                : Optional.empty();
    }

    @Override
    public Optional<Short> getShort(String key) {
        return this.input.get(key) instanceof ShortTag tag
                ? Optional.of(tag.getAsShort())
                : Optional.empty();
    }

    @Override
    public Optional<Integer> getInt(String key) {
        return this.input.get(key) instanceof IntTag tag
                ? Optional.of(tag.getAsInt())
                : Optional.empty();
    }

    @Override
    public Optional<Long> getLong(String key) {
        return this.input.get(key) instanceof LongTag tag
                ? Optional.of(tag.getAsLong())
                : Optional.empty();
    }

    @Override
    public Optional<Float> getFloat(String key) {
        return this.input.get(key) instanceof FloatTag tag
                ? Optional.of(tag.getAsFloat())
                : Optional.empty();
    }

    @Override
    public Optional<Double> getDouble(String key) {
        return this.input.get(key) instanceof DoubleTag tag
                ? Optional.of(tag.getAsDouble())
                : Optional.empty();
    }

    @Override
    public Optional<String> getString(String key) {
        return this.input.get(key) instanceof StringTag tag
                ? Optional.of(tag.getAsString())
                : Optional.empty();
    }

    @Override
    public Optional<Tag> get(String key) {
        return Optional.ofNullable(this.input.get(key));
    }
}
