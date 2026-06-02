# Cart-rs

Cart is a file format for storing and transferring malware in a safe way.

The primary way the file is made safe is by encrypting it to prevent accidental
execution. Files are also compressed to minimize space usage.

Cart-rs supports two versions of the CaRT format:

- **V1** (original): RC4 encryption + zlib compression
- **V2** (unofficial, Thorium-only): AES-128-GCM encryption + zstd compression

V2 is significantly faster than V1 thanks to AES-NI hardware acceleration and
zstd's superior compression speed. The `CartStreamBuilder` and
`CartStreamManualBuilder` default to V2.

Cart-rs does not currently support any of the optional fields in the header or
footer that the [Cart Spec](https://bitbucket.org/cse-assemblyline/cart/src/master/)
allows. We do not plan to ever support the optional footer fields as this would
come at the cost of supporting streaming downloads. This library is largely
intended to allow the Thorium API to stream CaRTed files to and from S3.

## CaRTing a File (async streaming)

Use `CartStream` with `tokio::io::copy` for async streaming:

```rust
use tokio::fs::{File, OpenOptions};
use tokio::io::{BufReader, BufWriter};
use cart_rs::{CartStream, CART_IO_BUF_SIZE};
use generic_array::{typenum::U16, GenericArray};

// have a cart password to use
let password: GenericArray<u8, U16> =
    GenericArray::clone_from_slice(&"SecretCornIsBest".as_bytes()[..16]);
// open the file to cart
let input = File::open("/tmp/EvilCorn").await?;
// use 256 KiB buffers for best throughput
let reader = BufReader::with_capacity(CART_IO_BUF_SIZE, input);
// build a V2 cart stream (the default)
let mut cart_stream = CartStream::builder(&password, reader).build()?;
// open a file to write the carted output to
let output = OpenOptions::new()
    .write(true)
    .create(true)
    .truncate(true)
    .open("/tmp/CartedCorn")
    .await?;
let mut writer = BufWriter::with_capacity(CART_IO_BUF_SIZE, output);
// stream the carted data to disk
tokio::io::copy(&mut cart_stream, &mut writer).await?;
```

To cart with V1 instead, use `.build_v1()`:

```rust
let mut cart_stream = CartStream::builder(&password, reader).build_v1()?;
```

## UnCaRTing a File

`UncartStream` automatically detects V1 and V2 from the header:

```rust
use tokio::fs::{File, OpenOptions};
use tokio::io::{BufReader, BufWriter};
use cart_rs::{UncartStream, CART_IO_BUF_SIZE};

// open our carted file with a 256 KiB buffer
let carted = File::open("/tmp/CartedCorn").await?;
let reader = BufReader::with_capacity(CART_IO_BUF_SIZE, carted);
// start uncarting this stream (auto-detects V1 or V2)
let mut uncart = UncartStream::new(reader);
// open a file to write the uncarted output to
let output = OpenOptions::new()
    .write(true)
    .create(true)
    .truncate(true)
    .open("/tmp/UnCartedCorn")
    .await?;
let mut writer = BufWriter::with_capacity(CART_IO_BUF_SIZE, output);
// stream the uncarted data to disk
tokio::io::copy(&mut uncart, &mut writer).await?;
```

## CaRTing a File Manually

Use `CartStreamManual` for non-async contexts where data arrives as `Bytes` chunks
(e.g., streaming uploads to S3):

```rust
use cart_rs::CartStreamManual;
use generic_array::{typenum::U16, GenericArray};
use bytes::Bytes;

// The max size of data to feed to a cart stream at once
const CHUNK_SIZE: usize = 32_768;
// The minimum amount of carted data to output/write
// This is primarily intended for use with s3 where there is a minium chunk size
// to write before writting the final chunk
const MIN_OUTPUT: usize = 5_242_880;

// have a cart password to use
let password: GenericArray<u8, U16> =
    GenericArray::clone_from_slice(&"SecretCornIsBest".as_bytes()[..16]);
// build a V2 manual cart stream (the default)
let mut cart = CartStreamManual::builder(&password, CHUNK_SIZE).build()?;
// feed data in chunks
for chunk in data.chunks(CHUNK_SIZE) {
    // normally this is used in a system where we already have a Bytes object
    // however this example 
    let bytes = Bytes::copy_from_slice(chunk);
    // feed in the next bytes to cart
    if cart.next_bytes(bytes)? {
        while cart.process()? {
            // check if we have enough data to output
            if cart.ready() >= MIN_OUTPUT {
                // write carted bytes to destination
                let writable = cart.carted_bytes();
                // dest.write_all(writable).await?;
                cart.consume();
            }
        }
    }
}
// flush remaining data and write the footer
let final_bytes = cart.finish()?;
// dest.write_all(final_bytes).await?;
```

## Builder Options

Both `CartStream::builder()` and `CartStreamManual::builder()` support:

- `.compression_level(level)` — zlib compression level for V1 (0–9)
- `.zstd_level(level)` — zstd compression level for V2 (1–22, default 3)
- `.segment_size(size)` — V2 encryption segment size (default 256 KiB)
- `.build_v1()` — build a V1 stream (RC4 + zlib)
- `.build_v2()` / `.build()` — build a V2 stream (AES-GCM + zstd, the default)

## Performance

For best throughput, wrap readers and writers in `BufReader::with_capacity` /
`BufWriter::with_capacity` using `CART_IO_BUF_SIZE` (256 KiB). The default
tokio buffer size of 8 KiB causes excessive poll overhead.

For V2, lower zstd levels trade compression ratio for speed. Level 1 is
significantly faster than the default level 3 with only marginally larger output:

```rust
let stream = CartStream::builder(&key, input)
    .zstd_level(1)
    .build()?;
```
