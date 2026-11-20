package net.minecraft.world.scores;

/** NMS-заглушка {@code Objective} (CommandAPI проверяет наличие). */
public class Objective {

    private net.minecraft.network.chat.numbers.NumberFormat numberFormat;

    /** {@code Objective.setNumberFormat(NumberFormat)} (EliteMobs). */
    public void setNumberFormat(net.minecraft.network.chat.numbers.NumberFormat format) {
        this.numberFormat = format;
    }

    public net.minecraft.network.chat.numbers.NumberFormat numberFormat() {
        return this.numberFormat;
    }

    public Objective() {
    }
}
