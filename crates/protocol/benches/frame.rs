//! Кодирование и разбор кадров: путь каждого пакета в обе стороны.
//!
//! Сжатие живёт в `FrameCodec`: один раз на пакет для всех получателей (так
//! делает сервер) плюс разбор входящего потока. Эти два числа — цена сети в
//! тике, поэтому они и меряются: маленький пакет движения (ниже порога, без
//! сжатия) и тело чанка (~5 КиБ, со сжатием).

use bytes::BytesMut;
use chunkwake_protocol::frame::FrameCodec;
use chunkwake_protocol::play;
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

/// Тело чанка, как его отдаёт `ChunkColumn::encode_body` для плоского мира:
/// плотный, но не константный поток байт (сжимается примерно вдвое).
fn chunk_body() -> Vec<u8> {
    let mut body = Vec::with_capacity(6 * 1024);
    for i in 0..6 * 1024u32 {
        body.push((i.wrapping_mul(37) % 251) as u8);
    }
    body
}

fn codec() -> FrameCodec {
    FrameCodec {
        threshold: Some(256),
    }
}

fn encode(c: &mut Criterion) {
    let codec = codec();
    let body = chunk_body();
    let chunk = play::level_chunk_with_light(0x20, 3, -7, &body);
    let small = play::move_entity_pos_rot(0x2c, 1234, [8, 0, -8], 90.0, 0.0, true);
    let mut group = c.benchmark_group("frame");
    group.bench_function("encode_chunk", |b| {
        let mut out = Vec::with_capacity(chunk.len() / 2 + 64);
        b.iter(|| {
            out.clear();
            codec.encode(black_box(&chunk), &mut out);
            black_box(out.len());
        });
    });
    group.bench_function("encode_small", |b| {
        let mut out = Vec::with_capacity(256);
        b.iter(|| {
            out.clear();
            codec.encode(black_box(&small), &mut out);
            black_box(out.len());
        });
    });
    let mut encoded = Vec::new();
    codec.encode(&chunk, &mut encoded);
    group.bench_function("decode_chunk", |b| {
        b.iter(|| {
            let mut buf = BytesMut::from(&encoded[..]);
            let packet = codec.decode(&mut buf).expect("frame");
            black_box(packet);
        });
    });
    let mut small_frame = Vec::new();
    codec.encode(&small, &mut small_frame);
    group.bench_function("decode_small", |b| {
        b.iter(|| {
            let mut buf = BytesMut::from(&small_frame[..]);
            let packet = codec.decode(&mut buf).expect("frame");
            black_box(packet);
        });
    });
    group.finish();
}

criterion_group!(benches, encode);
criterion_main!(benches);
