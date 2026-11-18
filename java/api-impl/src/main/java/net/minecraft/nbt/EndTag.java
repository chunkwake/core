package net.minecraft.nbt;

/** NBT end-тег. */
public final class EndTag implements Tag {
    public static final EndTag INSTANCE = new EndTag();

    private EndTag() {
    }

    @Override
    public byte getId() {
        return 0;
    }

    @Override
    public String toString() {
        return "END";
    }
}
