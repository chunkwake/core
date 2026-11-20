package ca.spottedleaf.common.time;

/**
 * Moonrise {@code TickData}: статистика длительности тиков.
 *
 * <p>TabTPS читает её через MethodHandle: {@code generateTickReport(TickTime,
 * long, long) -> TickReportData}, {@code TickReportData.timePerTickData() ->
 * SegmentedAverage}, {@code SegmentedAverage.rawData() -> long[]}.
 */
public class TickData {
    public TickReportData generateTickReport(TickTime start, long startTime, long endTime) {
        return new TickReportData();
    }

    /** {@code TickData.TickReportData}. */
    public static final class TickReportData {
        public SegmentedAverage timePerTickData() {
            return new SegmentedAverage();
        }
    }

    /** {@code TickData.SegmentedAverage}. */
    public static final class SegmentedAverage {
        public long[] rawData() {
            return new long[0];
        }
    }
}
