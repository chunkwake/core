package org.json.simple.parser;

import java.io.IOException;
import java.io.Reader;

/** json-simple `JSONParser` поверх Gson (см. {@code org.json.simple.JSONValue}). */
public class JSONParser {
    public Object parse(String text) throws ParseException {
        if (text == null) {
            throw new ParseException("Input must not be null", 0);
        }
        try {
            return org.json.simple.JSONValue.parse(text);
        } catch (RuntimeException error) {
            throw new ParseException(String.valueOf(error.getMessage()), 0);
        }
    }

    public Object parse(Reader reader) throws ParseException {
        try {
            StringBuilder text = new StringBuilder();
            char[] buffer = new char[4096];
            int n;
            while ((n = reader.read(buffer)) >= 0) {
                text.append(buffer, 0, n);
            }
            return parse(text.toString());
        } catch (IOException error) {
            throw new ParseException(String.valueOf(error.getMessage()), 0);
        }
    }

    public Object parse(String text, ContainerFactory factory) throws ParseException {
        return parse(text);
    }

    public Object parse(Reader reader, ContainerFactory factory) throws ParseException {
        return parse(reader);
    }
}
