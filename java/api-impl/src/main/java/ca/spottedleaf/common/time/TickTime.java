package ca.spottedleaf.common.time;

/** Moonrise {@code TickTime}: точка начала/конца отчёта. */
public class TickTime {
    public final long start;
    public final long end;

    public TickTime(long start, long end) {
        this.start = start;
        this.end = end;
    }
}
