package com.mojang.serialization;

/** Минимальный DFU {@code DataResult}. */
public interface DataResult<A> {
    A result();

    A getOrThrow();

    boolean isSuccess();

    java.util.Optional<String> error();

    static <A> DataResult<A> success(A value) {
        return new SimpleDataResult<>(value, null);
    }

    static <A> DataResult<A> error(String message) {
        return new SimpleDataResult<>(null, message);
    }

    final class SimpleDataResult<A> implements DataResult<A> {
        private final A result;
        private final String error;

        SimpleDataResult(A result, String error) {
            this.result = result;
            this.error = error;
        }

        @Override public A result() { return this.result; }

        @Override public A getOrThrow() {
            if (this.error != null) {
                throw new IllegalStateException(this.error);
            }
            return this.result;
        }

        @Override public boolean isSuccess() { return this.error == null; }

        @Override public java.util.Optional<String> error() {
            return java.util.Optional.ofNullable(this.error);
        }
    }
}
