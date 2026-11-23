package org.json.simple;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.google.gson.JsonPrimitive;
import java.util.Map;

/**
 * json-simple `JSONValue`: `parse`/`toJSONString` на базе Gson (он уже есть в
 * classpath сервера). Возвращает те же типы, что json-simple: `Long`, `Double`,
 * `Boolean`, `String`, {@link JSONObject}, {@link JSONArray}, `null`.
 */
public final class JSONValue {
    private JSONValue() {
    }

    public static Object parse(String text) {
        if (text == null) {
            return null;
        }
        try {
            return fromGson(com.google.gson.JsonParser.parseString(text));
        } catch (RuntimeException error) {
            return null;
        }
    }

    /** json-simple {@code parseWithException(String)}. */
    public static Object parseWithException(String text)
            throws org.json.simple.parser.ParseException {
        Object value = parse(text);
        if (value == null && text != null && !text.trim().equals("null") && !text.isBlank()) {
            throw new org.json.simple.parser.ParseException("Unexpected token", 0);
        }
        return value;
    }

    public static Object parse(java.io.Reader reader) {
        try {
            StringBuilder text = new StringBuilder();
            char[] buffer = new char[4096];
            int n;
            while ((n = reader.read(buffer)) >= 0) {
                text.append(buffer, 0, n);
            }
            return parse(text.toString());
        } catch (java.io.IOException error) {
            return null;
        }
    }

    public static String toJSONString(Object value) {
        return toGson(value).toString();
    }

    public static void writeJSONString(Object value, java.io.Writer out)
            throws java.io.IOException {
        out.write(toJSONString(value));
    }

    private static Object fromGson(JsonElement element) {
        if (element == null || element.isJsonNull()) {
            return null;
        }
        if (element.isJsonObject()) {
            JSONObject out = new JSONObject();
            for (Map.Entry<String, JsonElement> e : element.getAsJsonObject().entrySet()) {
                out.put(e.getKey(), fromGson(e.getValue()));
            }
            return out;
        }
        if (element.isJsonArray()) {
            JSONArray out = new JSONArray();
            for (JsonElement item : element.getAsJsonArray()) {
                out.add(fromGson(item));
            }
            return out;
        }
        JsonPrimitive primitive = element.getAsJsonPrimitive();
        if (primitive.isBoolean()) {
            return primitive.getAsBoolean();
        }
        if (primitive.isNumber()) {
            String raw = primitive.getAsString();
            if (!raw.contains(".") && !raw.contains("e") && !raw.contains("E")) {
                try {
                    return Long.valueOf(raw);
                } catch (NumberFormatException ignored) {
                    // падаем в Double ниже
                }
            }
            return primitive.getAsDouble();
        }
        return primitive.getAsString();
    }

    private static JsonElement toGson(Object value) {
        if (value == null) {
            return JsonNull.INSTANCE;
        }
        if (value instanceof JsonElement element) {
            return element;
        }
        if (value instanceof JSONObject object) {
            JsonObject out = new JsonObject();
            for (Map.Entry<String, Object> e : object.entrySet()) {
                out.add(e.getKey(), toGson(e.getValue()));
            }
            return out;
        }
        if (value instanceof JSONArray array) {
            JsonArray out = new JsonArray();
            for (Object item : array) {
                out.add(toGson(item));
            }
            return out;
        }
        if (value instanceof Map<?, ?> map) {
            JsonObject out = new JsonObject();
            for (Map.Entry<?, ?> e : map.entrySet()) {
                out.add(String.valueOf(e.getKey()), toGson(e.getValue()));
            }
            return out;
        }
        if (value instanceof Iterable<?> iterable) {
            JsonArray out = new JsonArray();
            for (Object item : iterable) {
                out.add(toGson(item));
            }
            return out;
        }
        if (value instanceof Number number) {
            return new JsonPrimitive(number);
        }
        if (value instanceof Boolean bool) {
            return new JsonPrimitive(bool);
        }
        if (value instanceof Character ch) {
            return new JsonPrimitive(String.valueOf(ch));
        }
        return new JsonPrimitive(String.valueOf(value));
    }
}
