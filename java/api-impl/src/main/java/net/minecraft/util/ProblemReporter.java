package net.minecraft.util;

/** NMS {@code ProblemReporter}. */
public interface ProblemReporter {
    ProblemReporter DISCARDING = new ProblemReporter() {
    };

    default void report(String message) {
        // по умолчанию проблемы игнорируются (как DISCARDING)
    }
}
