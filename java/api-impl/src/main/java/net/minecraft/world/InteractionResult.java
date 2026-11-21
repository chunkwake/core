package net.minecraft.world;

/** NMS {@code InteractionResult} (CraftEngine): статические исходы. */
public class InteractionResult {
    public static final InteractionResult SUCCESS = new InteractionResult(true);
    public static final InteractionResult SUCCESS_SERVER = SUCCESS;
    public static final InteractionResult CONSUME = new InteractionResult(true);
    public static final InteractionResult PASS = new InteractionResult(false);
    public static final InteractionResult FAIL = new InteractionResult(false);

    private final boolean consumesAction;

    private InteractionResult(boolean consumesAction) {
        this.consumesAction = consumesAction;
    }

    public boolean consumesAction() {
        return this.consumesAction;
    }
}
