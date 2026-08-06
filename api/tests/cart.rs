//! Test that files survive being carted on their way into s3
//!
//! `api/tests/files.rs` already covers the happy path and the large multipart path from the
//! client's side. What is here is the part that only the `CaRT` rewrite made testable, and the
//! sizes that only break at a boundary:
//!
//! * A zero byte upload. Carting an empty body used to return `FinishBeforeData`, and every cart
//!   error maps to a 500, so uploading an empty file was an internal server error plus an
//!   orphaned multipart upload in s3.
//! * Both `CaRT` versions. The version a deployment writes with is startup config, so the
//!   in-process API these tests run against can only ever write one of them. Reaching both means
//!   driving `S3Client` directly with a cloned config, which has the side benefit of testing
//!   exactly the code the client path is made of rather than a layer above it.
//! * Sizes that straddle cart's internal boundaries. V2 frames its body into blocks and both
//!   versions prefix a header and append a footer, so the interesting inputs are the ones landing
//!   one byte either side of a block and of the header. The part flush threshold is `PART_SIZE`,
//!   16 MiB, so only `multipart_carted_upload_round_trips` is large enough to cross it.

use axum::extract::{DefaultBodyLimit, FromRequest, Multipart};
use bytes::Bytes;
use data_encoding::HEXLOWER;
use rand::RngCore;
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use cart_rs::{CartVersion, UncartStream};
use thorium::models::{Buffer, FileDownloadOpts, SampleRequest};
use thorium::test_utilities::{self, generators};
use thorium::utils::s3::S3;
use thorium::{Conf, is};

/// The size of the frames a multipart body is handed to axum in
///
/// `axum::body::Body::from(Vec<u8>)` is a single frame, which hands the carter the whole body in
/// one push and hides every bug that only appears across a read boundary. A real upload arrives
/// in socket sized pieces, so feed one in.
const FRAME_SIZE: usize = 64 * 1024;

/// Wrap a buffer in a multipart body and hand back the field for it
///
/// The carting entry points on `S3Client` all take an `axum` multipart `Field`, because that is
/// what the upload routes have. `Field` borrows from its `Multipart`, so the caller has to own
/// the `Multipart` for as long as it uses the field, which is why this returns the `Multipart`
/// rather than the field itself.
///
/// # Arguments
///
/// * `data` - The bytes to put in the field
///
/// # Panics
///
/// Panics if a multipart body we built ourselves cannot be parsed back.
async fn multipart_of(data: &[u8]) -> Multipart {
    // a boundary that cannot occur in the body, since the body is arbitrary bytes
    let boundary = format!("--------{}", Uuid::new_v4().simple());
    // assemble the body by hand, since there is no multipart writer in the dependency tree
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"sample\"\r\n\r\n",
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    // cut the body into frames so the carter is pushed to in pieces like a real upload is
    let frames = body
        .chunks(FRAME_SIZE)
        .map(Bytes::copy_from_slice)
        .collect::<Vec<Bytes>>();
    // yield between frames, because a stream that is always ready gets drained into a single
    // chunk before the field hands anything back and the framing above would buy us nothing
    let stream = futures::stream::unfold(frames.into_iter(), |mut frames| async move {
        // let the reader run before handing over the next frame
        tokio::task::yield_now().await;
        // hand over the next frame, ending the stream once they are gone
        frames
            .next()
            .map(|frame| (Ok::<Bytes, std::io::Error>(frame), frames))
    });
    // wrap it in the request axum's extractor expects
    let mut request = axum::http::Request::builder()
        .method("POST")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(axum::body::Body::from_stream(stream))
        .expect("failed to build a multipart request");
    // the api disables axum's 2 MiB default body limit on every route, but a hand built request
    // carries no extensions, so the extractor re-applies that limit unless we stamp the same
    // decision onto the request. without this anything over 2 MiB fails inside `next_field`
    DefaultBodyLimit::disable().apply(&mut request);
    Multipart::from_request(request, &())
        .await
        .expect("failed to parse a multipart body we built ourselves")
}

/// Build an s3 client set that carts with a specific `CaRT` version
///
/// `S3::new` builds its clients without talking to s3, so it neither creates nor checks for a
/// bucket. Bootstrapping the test API is the only thing in the harness that creates the buckets
/// these tests write to, so do that first rather than depending on a previous run having left
/// them behind.
///
/// # Arguments
///
/// * `version` - The `CaRT` version these clients should write with
async fn s3_writing(version: CartVersion) -> Result<S3, thorium::Error> {
    // stand the api up if it is not up yet, since that is what creates the buckets we write to
    test_utilities::admin_client().await?;
    // clone the test config so overriding the version cannot leak into another test
    let mut conf: Conf = test_utilities::CONF.clone();
    // only the files client reads this, so the other clients keep whatever the config asked for
    conf.thorium.files.cart_version = version;
    Ok(S3::new(&conf)?)
}

/// Cart a buffer into s3, read it back, and return what came out
///
/// # Arguments
///
/// * `s3` - The s3 clients to cart through, already set to the version under test
/// * `data` - The bytes to round trip
async fn round_trip(s3: &S3, data: &[u8]) -> Result<Vec<u8>, thorium::Error> {
    // a fresh path per call so concurrent round trips cannot collide in the bucket
    let path = format!("cart-test/{}", Uuid::new_v4());
    // cart the buffer into s3 through the same multipart machinery the upload routes use
    let mut multipart = multipart_of(data).await;
    let field = multipart
        .next_field()
        .await
        .expect("failed to read our own multipart field")
        .expect("our multipart body had no field");
    // the s3 layer speaks `ApiError` because it sits under the routes, so flatten those into
    // the error type the test harness returns rather than dragging the route error type in here
    s3.files
        .cart_and_stream(path.clone(), field)
        .await
        .map_err(|err| thorium::Error::new(err.to_string()))?;
    // read the carted object back out and uncart it
    let stream = s3
        .files
        .download(&path)
        .await
        .map_err(|err| thorium::Error::new(err.to_string()))?;
    let mut uncart = UncartStream::new(stream.into_async_read());
    let mut recovered = Vec::new();
    uncart.read_to_end(&mut recovered).await?;
    // clean up after ourselves so repeated runs do not fill the bucket
    s3.files
        .delete(&path)
        .await
        .map_err(|err| thorium::Error::new(err.to_string()))?;
    Ok(recovered)
}

/// Round trip a single size and check that what came back is what went in
///
/// # Arguments
///
/// * `s3` - The s3 clients to cart through, already set to the version under test
/// * `size` - The number of random bytes to round trip
async fn check_size(s3: &S3, size: usize) -> Result<(), thorium::Error> {
    // fill with random data so a codec cannot pass by compressing everything away
    let mut data = vec![0u8; size];
    rand::rng().fill_bytes(&mut data);
    // cart it into s3 and pull it back out
    let recovered = round_trip(s3, &data).await?;
    is!(recovered.len(), size);
    // compare digests rather than the buffers so a mismatch does not dump megabytes
    is!(
        HEXLOWER.encode(&Sha256::digest(&recovered)),
        HEXLOWER.encode(&Sha256::digest(&data))
    );
    Ok(())
}

/// A zero byte body must upload and download cleanly rather than 500
#[tokio::test]
async fn upload_empty_file() -> Result<(), thorium::Error> {
    // get admin client
    let client = test_utilities::admin_client().await?;
    // Create a group
    let group = generators::groups(1, &client).await?.remove(0).name;
    // upload a sample with no bytes in it at all
    let file_req = SampleRequest::new_buffer(Buffer::new(Vec::new()), vec![group])
        .description("an empty test file");
    let resp = client.files.create(file_req).await?;
    // the sha256 of an empty file is a known constant, so this also proves the hashers ran
    is!(
        resp.sha256,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned()
    );
    // download it back and make sure it is still empty rather than missing or corrupt
    let temp_path = std::env::temp_dir().join(format!("UNCARTED_EMPTY_{}", Uuid::new_v4()));
    let mut opts = FileDownloadOpts::default().uncart();
    client
        .files
        .download(&resp.sha256, &temp_path, &mut opts)
        .await?;
    let data = tokio::fs::read(&temp_path).await?;
    tokio::fs::remove_file(&temp_path).await?;
    is!(data.len(), 0);
    Ok(())
}

/// Both `CaRT` versions have to round trip byte for byte through the multipart upload path
#[tokio::test]
async fn both_cart_versions_round_trip() -> Result<(), thorium::Error> {
    // a size that spans more than one V2 block but stays small enough to compare directly
    let size = 3 * 1024 * 1024 + 7;
    for version in [CartVersion::V1, CartVersion::V2] {
        // build one set of clients for this version rather than one per round trip
        let s3 = s3_writing(version).await?;
        // cart it in and read it back
        check_size(&s3, size).await?;
    }
    Ok(())
}

/// Sizes on either side of cart's internal boundaries
///
/// The header is 38 bytes, the footer 28, and V2 blocks are a power of two, so a size that lands
/// one byte either side of any of those is where an off by one lives. Zero is in the list because
/// an empty body took a different path through the manual carter than a short one did.
#[tokio::test]
async fn cart_boundary_sizes_round_trip() -> Result<(), thorium::Error> {
    // one megabyte, which is the default V2 block size
    const BLOCK: usize = 1 << 20;
    let sizes = [
        0,
        1,
        37,
        38,
        39,
        65,
        66,
        67,
        BLOCK - 1,
        BLOCK,
        BLOCK + 1,
        2 * BLOCK,
    ];
    for version in [CartVersion::V1, CartVersion::V2] {
        // build one set of clients for every size at this version
        let s3 = s3_writing(version).await?;
        // round trip every size at once, since each one writes to a path of its own
        futures::future::try_join_all(sizes.iter().map(|size| check_size(&s3, *size))).await?;
    }
    Ok(())
}

/// A carted upload big enough to need several parts has to survive in the right order
///
/// Parts are submitted concurrently, so a bug in part numbering or in the owned handoff shows up
/// as bytes that are all present and in the wrong order — which a length check would miss and a
/// digest catches.
///
/// Run it with
/// `cargo test -p thorium-api --features test-utilities --test cart multipart -- --ignored`.
#[tokio::test]
#[ignore = "moves 192 MiB through s3, run manually when touching the carted multipart path"]
async fn multipart_carted_upload_round_trips() -> Result<(), thorium::Error> {
    // large enough to cross the 16 MiB part flush threshold several times over
    let size = 96 * 1024 * 1024;
    for version in [CartVersion::V1, CartVersion::V2] {
        // build one set of clients for this version rather than one per round trip
        let s3 = s3_writing(version).await?;
        // cart it in and read it back
        check_size(&s3, size).await?;
    }
    Ok(())
}
