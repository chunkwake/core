package net.minecraft;

/** Минимальный NMS {@code ReportedException} (InvSee++). */
public class ReportedException extends RuntimeException {
    public ReportedException() {
    }

    public ReportedException(Throwable cause) {
        super(cause);
    }
}
