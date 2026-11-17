package org.json.simple.parser;

/** json-simple `ParseException`. */
public class ParseException extends Exception {
    private final int position;

    public ParseException() {
        this("Unexpected token", -1);
    }

    public ParseException(String message) {
        this(message, -1);
    }

    public ParseException(String message, int position) {
        super(message);
        this.position = position;
    }

    public int getPosition() {
        return this.position;
    }

    @Override
    public String toString() {
        return getMessage() + (this.position >= 0 ? " at position " + this.position : "");
    }
}
