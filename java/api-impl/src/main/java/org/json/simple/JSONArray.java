package org.json.simple;

import java.util.ArrayList;
import java.util.Collection;

/** json-simple `JSONArray`: список значений, печатается как JSON. */
public class JSONArray extends ArrayList<Object> implements JSONAware, JSONStreamAware {
    public JSONArray() {
        super();
    }

    public JSONArray(Collection<?> values) {
        super();
        if (values != null) {
            addAll(values);
        }
    }

    @Override
    public void writeJSONString(java.io.Writer out) throws java.io.IOException {
        out.write(JSONValue.toJSONString(this));
    }

    @Override
    public String toJSONString() {
        return JSONValue.toJSONString(this);
    }
}
