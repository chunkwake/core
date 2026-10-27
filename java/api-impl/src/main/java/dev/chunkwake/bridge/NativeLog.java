package dev.chunkwake.bridge;

import java.io.PrintWriter;
import java.io.StringWriter;
import java.util.logging.Handler;
import java.util.logging.Level;
import java.util.logging.LogManager;
import java.util.logging.LogRecord;
import java.util.logging.Logger;

/** java.util.logging → журнал сервера (tracing в Rust). */
final class NativeLog extends Handler {
    static void install() {
        LogManager.getLogManager().reset();
        Logger root = Logger.getLogger("");
        root.setLevel(Level.INFO);
        root.addHandler(new NativeLog());
    }

    @Override
    public void publish(LogRecord r) {
        if (!isLoggable(r)) {
            return;
        }
        int v = r.getLevel().intValue();
        int level = v >= Level.SEVERE.intValue() ? 0 : v >= Level.WARNING.intValue() ? 1
                : v >= Level.INFO.intValue() ? 2 : 3;
        String name = r.getLoggerName();
        StringBuilder msg = new StringBuilder();
        if (name != null && !name.isEmpty()) {
            msg.append('[').append(name).append("] ");
        }
        String text = r.getMessage();
        Object[] params = r.getParameters();
        if (text != null && params != null && params.length > 0) {
            text = java.text.MessageFormat.format(text, params);
        }
        msg.append(text);
        if (r.getThrown() != null) {
            StringWriter sw = new StringWriter();
            r.getThrown().printStackTrace(new PrintWriter(sw));
            msg.append('\n').append(sw);
        }
        Bridge.log(level, msg.toString());
    }

    @Override
    public void flush() {}

    @Override
    public void close() {}
}
