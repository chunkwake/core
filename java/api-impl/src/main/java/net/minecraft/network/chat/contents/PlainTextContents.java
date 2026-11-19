package net.minecraft.network.chat.contents;

import net.minecraft.network.chat.ComponentContents;

/** NMS {@code PlainTextContents} (+ LiteralContents). */
public interface PlainTextContents extends ComponentContents {
    PlainTextContents EMPTY = new LiteralContents("");

    String text();

    /** NMS {@code PlainTextContents.LiteralContents}. */
    final class LiteralContents implements PlainTextContents {
        private final String text;

        public LiteralContents(String text) {
            this.text = text == null ? "" : text;
        }

        @Override
        public String text() {
            return this.text;
        }
    }
}
