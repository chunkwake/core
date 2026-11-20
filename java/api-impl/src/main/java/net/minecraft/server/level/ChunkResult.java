package net.minecraft.server.level;

import java.util.function.Consumer;
import java.util.function.Function;
import java.util.function.Supplier;

/** NMS {@code ChunkResult}. */
public interface ChunkResult<T> {
    static <T> ChunkResult<T> of(T value) {
        return new ChunkResult<>() {
            @Override
            public boolean isSuccess() {
                return true;
            }

            @Override
            public T orElse(T fallback) {
                return value;
            }

            @Override
            public String getError() {
                return null;
            }

            @Override
            public ChunkResult<T> ifSuccess(Consumer<T> consumer) {
                consumer.accept(value);
                return this;
            }

            @Override
            public <R> ChunkResult<R> map(Function<T, R> function) {
                return of(function.apply(value));
            }

            @Override
            public <E extends Throwable> T orElseThrow(Supplier<E> supplier) {
                return value;
            }
        };
    }

    static <T> ChunkResult<T> error(String message) {
        return error(() -> message);
    }

    static <T> ChunkResult<T> error(Supplier<String> message) {
        return new ChunkResult<>() {
            @Override
            public boolean isSuccess() {
                return false;
            }

            @Override
            public T orElse(T fallback) {
                return fallback;
            }

            @Override
            public String getError() {
                return message.get();
            }

            @Override
            public ChunkResult<T> ifSuccess(Consumer<T> consumer) {
                return this;
            }

            @Override
            public <R> ChunkResult<R> map(Function<T, R> function) {
                return error(message);
            }

            @Override
            public <E extends Throwable> T orElseThrow(Supplier<E> supplier) throws E {
                throw supplier.get();
            }
        };
    }

    @SuppressWarnings("unchecked")
    static <R> R orElse(ChunkResult<? extends R> result, R fallback) {
        return result.isSuccess() ? (R) result.orElse(null) : fallback;
    }

    boolean isSuccess();

    T orElse(T fallback);

    String getError();

    ChunkResult<T> ifSuccess(Consumer<T> consumer);

    <R> ChunkResult<R> map(Function<T, R> function);

    <E extends Throwable> T orElseThrow(Supplier<E> supplier) throws E;
}
