package com.mojang.serialization;

/** Минимальный DFU-результат: ProtocolLib оборачивает его в WrappedDataResult. */
public final class DataResult<A> {
    private final A result;
    private final String error;

    private DataResult(A result, String error) {
        this.result = result;
        this.error = error;
    }

    public A result() {
        return this.result;
    }

    public A getOrThrow() {
        if (this.error != null) {
            throw new IllegalStateException(this.error);
        }
        return this.result;
    }

    public boolean isSuccess() {
        return this.error == null;
    }

    public java.util.Optional<String> error() {
        return java.util.Optional.ofNullable(this.error);
    }

    public static <A> DataResult<A> success(A value) {
        return new DataResult<>(value, null);
    }

    public static <A> DataResult<A> error(String message) {
        return new DataResult<>(null, message);
    }
}
