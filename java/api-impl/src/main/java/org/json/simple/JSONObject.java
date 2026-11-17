package org.json.simple;

import java.util.LinkedHashMap;
import java.util.Map;

/**
 * json-simple `JSONObject`: упорядоченный словарь, печатается как JSON.
 * CraftBukkit поставляет этот класс для плагинов (Vault, старые экономики).
 */
public class JSONObject extends LinkedHashMap<String, Object>
        implements JSONAware, JSONStreamAware {
    public JSONObject() {
        super();
    }

    public JSONObject(Map<?, ?> map) {
        super();
        if (map != null) {
            for (Map.Entry<?, ?> e : map.entrySet()) {
                put(String.valueOf(e.getKey()), e.getValue());
            }
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
