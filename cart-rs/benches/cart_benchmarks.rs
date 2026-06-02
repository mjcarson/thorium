//! Benchmarks for the cart-rs crate
//!
//! Measures throughput of carting and uncarting data at various file sizes using
//! both the async `CartStream`/`UncartStream` APIs and the manual `CartStreamManual` API.

use std::io::Cursor;

use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use generic_array::{GenericArray, typenum::U16};
use tokio::io::{AsyncReadExt, BufReader};

use cart_rs::{CartStream, CartStreamBuilder, CartStreamManual, UncartStream};

/// The file sizes to benchmark against
const SIZES: [usize; 5] = [
    100 << 10, // 100 KiB
    1 << 20,   // 1 MiB
    3 << 20,   // 3 MiB
    10 << 20,  // 10 MiB
    100 << 20, // 100 MiB
];

/// Build the standard 16-byte encryption key used across all benchmarks
fn make_key() -> GenericArray<u8, U16> {
    GenericArray::clone_from_slice(b"SecretCornIsBest")
}

/// Generate deterministic pseudo-random data of the given size
///
/// Uses a prime modulus (251) to avoid compression-friendly patterns,
/// producing realistic throughput numbers.
///
/// # Arguments
///
/// * `size` - The number of bytes to generate
fn generate_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 251) as u8).collect()
}

/// Format a byte count as a human-readable size string for benchmark labels
///
/// # Arguments
///
/// * `size` - The size in bytes to format
fn format_size(size: usize) -> String {
    if size >= 1024 * 1024 {
        format!("{}_MiB", size / (1024 * 1024))
    } else if size >= 1024 {
        format!("{}_KiB", size / 1024)
    } else {
        format!("{}_B", size)
    }
}

/// Build a single-threaded tokio runtime for consistent benchmark results
fn make_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Cart the given data in-memory and return the carted bytes
///
/// Used to pre-generate carted data for the uncart benchmarks so that
/// the carting cost is not included in the uncart measurement.
///
/// # Arguments
///
/// * `rt` - The tokio runtime to use
/// * `data` - The raw data to cart
/// * `key` - The 16 byte encryption key
fn cart_to_bytes_v1(
    rt: &tokio::runtime::Runtime,
    data: &[u8],
    key: &GenericArray<u8, U16>,
) -> Vec<u8> {
    rt.block_on(async {
        let cursor = Cursor::new(data);
        let stream = CartStream::builder(key, cursor).build_v1().unwrap();
        // wrap in BufReader to ensure the internal read buffer is large enough for the CaRT header
        let mut buffered = BufReader::new(stream);
        let mut output = Vec::new();
        buffered.read_to_end(&mut output).await.unwrap();
        output
    })
}

/// Benchmark carting data using the async `CartStream` API
///
/// Measures the throughput of compressing and encrypting data through
/// the `AsyncRead`-based streaming interface.
fn bench_cart_v1(c: &mut Criterion) {
    let rt = make_runtime();
    let key = make_key();

    let mut group = c.benchmark_group("cart");
    for size in SIZES {
        let data = generate_data(size);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("CartStream", format_size(size)),
            &data,
            |b, data| {
                b.iter(|| {
                    rt.block_on(async {
                        // wrap our raw data in a cursor to provide an in-memory AsyncBufRead source
                        let cursor = Cursor::new(data.as_slice());
                        // build our cart stream
                        let stream = CartStream::builder(&key, cursor).build_v1().unwrap();
                        // copy the carted output to a sink to measure throughput without allocation noise
                        let bytes =
                            tokio::io::copy(&mut BufReader::new(stream), &mut tokio::io::sink())
                                .await
                                .unwrap();
                        bytes
                    })
                })
            },
        );
    }
    group.finish();
}

/// Benchmark carting data using the manual `CartStreamManual` API
///
/// Measures the throughput of the chunk-based carting interface that does not
/// require an `AsyncRead` source, processing data in 32 KiB chunks.
fn bench_cart_manual(c: &mut Criterion) {
    let key = make_key();
    let mut group = c.benchmark_group("cart_manual");
    for size in SIZES {
        let data = generate_data(size);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("CartStreamManual", format_size(size)),
            &data,
            |b, data| {
                b.iter(|| {
                    let chunk_size = 32_768;
                    // build our manual cart streamer
                    let mut cart = CartStreamManual::builder(&key, chunk_size).build_v1().unwrap();
                    let mut output = Vec::new();
                    // feed data in 32 KiB chunks
                    for chunk in data.chunks(chunk_size) {
                        // freeze this chunk into an immutable Bytes handle
                        let bytes = Bytes::copy_from_slice(chunk);
                        // queue these bytes for processing
                        if cart.next_bytes(bytes).unwrap() {
                            // keep processing until the current buffer is exhausted
                            while cart.process().unwrap() {
                                // drain the output buffer when it has enough data
                                if cart.ready() >= chunk_size {
                                    output.extend_from_slice(cart.carted_bytes());
                                    cart.consume();
                                }
                            }
                        }
                    }
                    // finish packing and write the CaRT footer
                    output.extend_from_slice(cart.finish().unwrap());
                    output
                })
            },
        );
    }
    group.finish();
}

/// Benchmark uncarting data using the async `UncartStream` API
///
/// Measures the throughput of decrypting and decompressing carted data through
/// the `AsyncRead`-based streaming interface. The input data is pre-carted
/// outside of the timed section.
fn bench_uncart(c: &mut Criterion) {
    let rt = make_runtime();
    let key = make_key();
    let mut group = c.benchmark_group("uncart");
    for size in SIZES {
        let data = generate_data(size);
        // pre-cart our test data so the carting cost is not included in the measurement
        let carted = cart_to_bytes_v1(&rt, &data, &key);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("UncartStream", format_size(size)),
            &carted,
            |b, carted| {
                b.iter(|| {
                    rt.block_on(async {
                        // wrap our carted data in a cursor to provide an in-memory AsyncBufRead source
                        let cursor = Cursor::new(carted.as_slice());
                        // build our uncart stream
                        let stream = UncartStream::new(cursor);
                        // copy the uncarted output to a sink to measure throughput without allocation noise
                        let bytes =
                            tokio::io::copy(&mut BufReader::new(stream), &mut tokio::io::sink())
                                .await
                                .unwrap();
                        bytes
                    })
                })
            },
        );
    }
    group.finish();
}

/// Cart data in-memory using V2 (AES-GCM + zstd) and return the carted bytes
fn cart_to_bytes_v2(
    rt: &tokio::runtime::Runtime,
    data: &[u8],
    key: &GenericArray<u8, U16>,
) -> Vec<u8> {
    rt.block_on(async {
        let cursor = Cursor::new(data);
        let stream = CartStreamBuilder::new(key, cursor).build_v2().unwrap();
        let mut buffered = BufReader::new(stream);
        let mut output = Vec::new();
        buffered.read_to_end(&mut output).await.unwrap();
        output
    })
}

/// Benchmark carting data using the V2 `CartStreamBuilder` API
fn bench_cart_v2(c: &mut Criterion) {
    let rt = make_runtime();
    let key = make_key();
    let mut group = c.benchmark_group("cart_v2");
    for size in SIZES {
        let data = generate_data(size);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("CartStream_V2", format_size(size)),
            &data,
            |b, data| {
                b.iter(|| {
                    rt.block_on(async {
                        let cursor = Cursor::new(data.as_slice());
                        let stream = CartStreamBuilder::new(&key, cursor).build_v2().unwrap();
                        let bytes =
                            tokio::io::copy(&mut BufReader::new(stream), &mut tokio::io::sink())
                                .await
                                .unwrap();
                        bytes
                    })
                })
            },
        );
    }
    group.finish();
}

/// Benchmark uncarting V2 data
fn bench_uncart_v2(c: &mut Criterion) {
    let rt = make_runtime();
    let key = make_key();

    let mut group = c.benchmark_group("uncart_v2");
    for size in SIZES {
        let data = generate_data(size);
        let carted = cart_to_bytes_v2(&rt, &data, &key);

        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("UncartStream_V2", format_size(size)),
            &carted,
            |b, carted| {
                b.iter(|| {
                    rt.block_on(async {
                        let cursor = Cursor::new(carted.as_slice());
                        let stream = UncartStream::new(cursor);
                        let bytes =
                            tokio::io::copy(&mut BufReader::new(stream), &mut tokio::io::sink())
                                .await
                                .unwrap();
                        bytes
                    })
                })
            },
        );
    }
    group.finish();
}

/// Benchmark carting data using the manual V2 `CartStreamManual` API
///
/// Measures the throughput of the chunk-based V2 (AES-GCM + zstd) carting
/// interface, processing data in 32 KiB chunks.
fn bench_cart_manual_v2(c: &mut Criterion) {
    let key = make_key();

    let mut group = c.benchmark_group("cart_manual_v2");
    for size in SIZES {
        let data = generate_data(size);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("CartStreamManual_V2", format_size(size)),
            &data,
            |b, data| {
                b.iter(|| {
                    let chunk_size = 32_768;
                    let mut cart = CartStreamManual::builder(&key, chunk_size)
                        .build_v2()
                        .unwrap();
                    let mut output = Vec::new();

                    for chunk in data.chunks(chunk_size) {
                        let bytes = Bytes::copy_from_slice(chunk);
                        if cart.next_bytes(bytes).unwrap() {
                            while cart.process().unwrap() {
                                if cart.ready() >= chunk_size {
                                    output.extend_from_slice(cart.carted_bytes());
                                    cart.consume();
                                }
                            }
                        }
                    }

                    output.extend_from_slice(cart.finish().unwrap());
                    output
                })
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_cart_v1,
    bench_cart_manual,
    bench_cart_manual_v2,
    bench_uncart,
    bench_cart_v2,
    bench_uncart_v2
);
criterion_main!(benches);
