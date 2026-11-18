package com.mojang.serialization;

/**
 * Минимальная заглушка DataFixerUpper: ProtocolLib ищет этот класс при
 * инициализации обёрток (Paper несёт его в libraries, у нас — только api).
 */
public interface Codec<A> extends Encoder<A>, Decoder<A> {
}
