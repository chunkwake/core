package org.json.simple;

import java.io.IOException;
import java.io.Writer;

/** json-simple API: объект, который умеет писать себя в поток. */
public interface JSONStreamAware {
    void writeJSONString(Writer out) throws IOException;

    @SuppressWarnings("resource")
    default String toJSONString() {
        java.io.StringWriter out = new java.io.StringWriter();
        try {
            writeJSONString(out);
        } catch (IOException error) {
            throw new IllegalStateException(error);
        }
        return out.toString();
    }
}
