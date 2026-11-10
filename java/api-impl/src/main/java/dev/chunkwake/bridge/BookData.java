package dev.chunkwake.bridge;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;

/**
 * Содержимое книги для моста: {@code writable_book_content} (страницы — простой
 * текст) или {@code written_book_content} (страницы — текстовые компоненты JSON,
 * название, автор, поколение). Формат строки Rust — объект {@code book}.
 */
final class BookData {
    /** {@code WritableBookContent.MAX_PAGES} и длины страниц и названия. */
    static final int MAX_PAGES = 100, WRITABLE_PAGE_LENGTH = 1024, TITLE_LENGTH = 32;

    boolean written;
    /** У черновой — текст, у подписанной — JSON компонента. */
    List<String> pages = new ArrayList<>();
    String title;
    String author;
    int generation;
    boolean resolved;

    BookData(boolean written) {
        this.written = written;
    }

    BookData copy() {
        BookData b = new BookData(written);
        b.pages = new ArrayList<>(pages);
        b.title = title;
        b.author = author;
        b.generation = generation;
        b.resolved = resolved;
        return b;
    }

    static BookData parse(JsonObject j) {
        BookData b = new BookData(j.has("written") && j.get("written").getAsBoolean());
        if (j.has("pages")) {
            for (JsonElement p : j.getAsJsonArray("pages")) {
                b.pages.add(!b.written && p.isJsonPrimitive() ? p.getAsString() : p.toString());
            }
        }
        if (b.written) {
            b.title = j.has("title") ? j.get("title").getAsString() : "";
            b.author = j.has("author") ? j.get("author").getAsString() : "";
            b.generation = j.has("generation") ? j.get("generation").getAsInt() : 0;
            b.resolved = j.has("resolved") && j.get("resolved").getAsBoolean();
        }
        return b;
    }

    JsonObject json() {
        JsonObject j = new JsonObject();
        j.addProperty("written", written);
        JsonArray a = new JsonArray();
        for (String p : pages) {
            a.add(written ? JsonParser.parseString(p) : new JsonPrimitive(p));
        }
        j.add("pages", a);
        if (written) {
            j.addProperty("title", title == null ? "" : title);
            j.addProperty("author", author == null ? "" : author);
            j.addProperty("generation", generation);
            j.addProperty("resolved", resolved);
        }
        return j;
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof BookData b && b.written == written && b.pages.equals(pages)
                && Objects.equals(b.title, title) && Objects.equals(b.author, author)
                && b.generation == generation && b.resolved == resolved;
    }

    @Override
    public int hashCode() {
        return Objects.hash(written, pages, title, author, generation, resolved);
    }
}
