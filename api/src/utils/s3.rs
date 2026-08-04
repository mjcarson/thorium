//! Handles uploading files to s3

use aws_credential_types::provider::SharedCredentialsProvider;
use aws_sdk_s3::operation::get_object::GetObjectOutput;
use aws_sdk_s3::primitives::SdkBody;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart, Delete, ObjectIdentifier};
use aws_sdk_s3::{
    Client,
    config::{Credentials, retry::RetryConfig, timeout::TimeoutConfig},
    operation::head_object::HeadObjectError,
    primitives::ByteStream,
};
use axum::extract::multipart::Field;
use base64::Engine as _;
use bytes::{Bytes, BytesMut};
use cart_rs::{CartStreamManual, UncartStream};
use data_encoding::HEXLOWER;
use generic_array::{GenericArray, typenum::U16};
use md5::Md5;
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tracing::{Level, Span, event, instrument, span};
use uuid::Uuid;
use zip::unstable::write::FileOptionsExt;
use zip::write::ZipWriter;

use super::{ApiError, Shared};
use crate::models::ZipDownloadParams;
use crate::{Conf, bad, internal_err_unwrapped, unavailable};

/// The size in bytes each non-carted multipart upload part should target (16 MiB)
///
/// Larger parts mean fewer round trips to s3 for a large upload. This must stay at or
/// above the s3 minimum part size of 5 MiB for every part except the last.
const PART_SIZE: usize = 16 * 1024 * 1024;

/// The maximum number of part uploads that may be in flight at once
///
/// This bounds how much of the incoming file we hold in memory at any time (roughly
/// `MAX_CONCURRENT_UPLOADS * PART_SIZE`) while still letting uploads overlap so we keep
/// draining the incoming request body instead of stalling on each part upload.
const MAX_CONCURRENT_UPLOADS: usize = 8;

/// How often the stall watchdog logs the parts still in flight for an upload
///
/// A stalled part eventually blocks the read loop on the upload semaphore, so anything logged
/// from the submission loop goes silent exactly when an upload hangs. This watchdog ticks on
/// its own task so a stalled upload keeps reporting which parts it is waiting on.
const STALL_LOG_INTERVAL: Duration = Duration::from_secs(30);

/// A tuple of hashes (sha256, sha1, md5)
pub type Hashes = (String, String, String);

/// The standard hashes for a file
#[derive(Debug)]
pub struct StandardHashes {
    /// The sha256 hash
    pub sha256: String,
    /// The sha1 hash
    pub sha1: String,
    /// The md5 hash
    pub md5: String,
}

/// Hashes files with sha256, sha1, and md5
pub struct StandardHashers {
    /// The sha256 hasher
    pub sha256: Sha256,
    /// The sha1 hasher
    pub sha1: Sha1,
    /// The md5 hasher
    pub md5: Md5,
}

impl StandardHashers {
    /// Add a buffer to our hashers
    ///
    /// # Arguments
    ///
    /// * `buff` - The buffer to digest
    pub fn digest(&mut self, buff: &[u8]) {
        // digest this buffer with each of our hashers
        self.sha256.update(buff);
        self.sha1.update(buff);
        self.md5.update(buff);
    }

    /// Finalize our hashers and get our hashes
    pub fn finish(self) -> StandardHashes {
        // build our digests
        let sha256 = HEXLOWER.encode(&self.sha256.finalize());
        let sha1 = HEXLOWER.encode(&self.sha1.finalize());
        let md5 = HEXLOWER.encode(&self.md5.finalize());
        StandardHashes { sha256, sha1, md5 }
    }
}

impl Default for StandardHashers {
    /// Create default hashers
    fn default() -> Self {
        StandardHashers {
            sha256: Sha256::new(),
            sha1: Sha1::new(),
            md5: Md5::new(),
        }
    }
}

/// Get how many milliseconds have elapsed since an instant
///
/// [`Duration::as_millis`] returns a `u128` which cannot be logged as a tracing field, so this
/// saturates to a `u64` instead. No upload is going to run for the ~584 million years that
/// would take.
///
/// # Arguments
///
/// * `since` - The instant to measure from
fn elapsed_ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The mutable progress of a single multipart upload
struct ProgressState {
    /// The parts that have been submitted to s3 but have not finished yet and when each started
    in_flight: BTreeMap<i32, Instant>,
    /// The number of parts that have finished uploading
    completed: usize,
    /// The total number of bytes read from the incoming request body so far
    bytes_read: u64,
    /// The total number of bytes handed to s3 across every part so far
    bytes_sent: u64,
    /// When we last read a chunk from the incoming request body
    last_read: Instant,
    /// Whether the read loop is currently blocked waiting for an upload permit
    awaiting_permit: bool,
}

/// The progress of a single multipart upload
///
/// This is shared between the loop reading the incoming request body, every spawned part
/// upload task, and the stall watchdog, so all of its mutable state sits behind a lock. That
/// lock is only ever held for a handful of field updates and is never held across an await
/// point, so a blocking mutex is safe here.
struct MultipartProgress {
    /// The key in s3 this multipart upload is writing to
    path: String,
    /// The id of the multipart upload we are tracking
    upload_id: String,
    /// When this multipart upload started
    start: Instant,
    /// The mutable progress shared with the part upload tasks and the stall watchdog
    state: Mutex<ProgressState>,
}

impl MultipartProgress {
    /// Build the progress tracker for a new multipart upload
    ///
    /// # Arguments
    ///
    /// * `path` - The key in s3 this multipart upload is writing to
    /// * `upload_id` - The id of the multipart upload to track
    fn new(path: &str, upload_id: &str) -> Self {
        MultipartProgress {
            path: path.to_owned(),
            upload_id: upload_id.to_owned(),
            start: Instant::now(),
            state: Mutex::new(ProgressState {
                in_flight: BTreeMap::default(),
                completed: 0,
                bytes_read: 0,
                bytes_sent: 0,
                last_read: Instant::now(),
                awaiting_permit: false,
            }),
        }
    }

    /// Lock our progress state, recovering from a poisoned lock
    ///
    /// This state only exists to diagnose stalled uploads, so a part upload task panicking and
    /// poisoning this lock should not take down every upload that comes after it.
    fn state(&self) -> MutexGuard<'_, ProgressState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Render the parts currently in flight as `<part number>:<age>ms` pairs
    ///
    /// The upload semaphore caps this at `MAX_CONCURRENT_UPLOADS` entries so it is always small
    /// enough to log in full.
    fn in_flight(&self) -> String {
        // render each in flight part alongside how long it has been in flight
        self.state()
            .in_flight
            .iter()
            .map(|(part_num, started)| format!("{part_num}:{}ms", elapsed_ms(*started)))
            .collect::<Vec<String>>()
            .join(",")
    }

    /// Get the total number of bytes that have been handed to s3 for this upload
    fn bytes_sent(&self) -> u64 {
        self.state().bytes_sent
    }

    /// Record that a chunk was read from the incoming request body
    ///
    /// # Arguments
    ///
    /// * `read` - The number of bytes that were read
    fn record_read(&self, read: usize) {
        // track how much we have read and when we last read anything
        let mut state = self.state();
        state.bytes_read += read as u64;
        state.last_read = Instant::now();
    }

    /// Record that we have started or stopped waiting on an upload permit
    ///
    /// # Arguments
    ///
    /// * `waiting` - Whether the read loop is currently waiting on a permit
    fn permit_wait(&self, waiting: bool) {
        self.state().awaiting_permit = waiting;
    }

    /// Record that a part has been submitted to s3
    ///
    /// # Arguments
    ///
    /// * `part_num` - The part number that was submitted
    /// * `size` - The size in bytes of the part that was submitted
    fn start_part(&self, part_num: i32, size: usize) {
        // this part is now in flight
        let mut state = self.state();
        state.in_flight.insert(part_num, Instant::now());
        state.bytes_sent += size as u64;
    }

    /// Record that a part finished uploading and get how long it took in milliseconds
    ///
    /// # Arguments
    ///
    /// * `part_num` - The part number that finished uploading
    fn finish_part(&self, part_num: i32) -> u64 {
        // this part is no longer in flight
        let mut state = self.state();
        let elapsed = state.in_flight.remove(&part_num).map_or(0, elapsed_ms);
        // count this part towards the ones we have finished
        state.completed += 1;
        elapsed
    }

    /// Record that a part failed to upload and get how long it was in flight in milliseconds
    ///
    /// # Arguments
    ///
    /// * `part_num` - The part number that failed to upload
    fn fail_part(&self, part_num: i32) -> u64 {
        // this part is no longer in flight
        self.state()
            .in_flight
            .remove(&part_num)
            .map_or(0, elapsed_ms)
    }

    /// Log the parts this upload is still waiting on
    ///
    /// A hung upload shows up here as a part whose age keeps growing while nothing else makes
    /// progress. `awaiting_permit` and `last_read_ms_ago` separate the two ways an upload can
    /// stall: s3 not finishing a part we already sent, or the client not sending us any more
    /// of the request body.
    ///
    /// # Arguments
    ///
    /// * `span` - The span to log this report under
    fn log_stall(&self, span: &Span) {
        // render our in flight parts before we lock so we don't try to lock twice
        let in_flight = self.in_flight();
        // copy out the rest of the values we want to report on
        let state = self.state();
        let (completed, bytes_read, bytes_sent) =
            (state.completed, state.bytes_read, state.bytes_sent);
        let (last_read_ms_ago, awaiting_permit) =
            (elapsed_ms(state.last_read), state.awaiting_permit);
        // drop our lock before logging
        drop(state);
        // report on what this upload is still waiting on
        event!(
            parent: span,
            Level::INFO,
            msg = "Multipart upload in progress",
            path = self.path.as_str(),
            upload_id = self.upload_id.as_str(),
            in_flight,
            parts_completed = completed,
            bytes_read,
            bytes_sent,
            last_read_ms_ago,
            awaiting_permit,
            elapsed_ms = elapsed_ms(self.start),
        );
    }
}

/// A guard over the background task reporting on a multipart upload's progress
///
/// The watchdog is aborted when this guard is dropped so it can never outlive the upload it is
/// reporting on, including on the paths where an upload errors out part way through.
struct StallWatchdog {
    /// The handle of the spawned watchdog task
    handle: JoinHandle<()>,
}

impl StallWatchdog {
    /// Spawn a watchdog that periodically logs the parts an upload is still waiting on
    ///
    /// # Arguments
    ///
    /// * `progress` - The progress of the upload to report on
    fn spawn(progress: &Arc<MultipartProgress>) -> Self {
        // clone the progress the watchdog task needs to own
        let progress = Arc::clone(progress);
        // build a span so our reports stay attached to the trace of the request being uploaded
        let span = span!(Level::INFO, "S3Client::stall_watchdog");
        // report on this upload until it finishes and we get aborted
        let handle = tokio::spawn(async move {
            // build the interval we report on
            let mut interval = tokio::time::interval(STALL_LOG_INTERVAL);
            // the first tick of an interval completes immediately so burn it
            interval.tick().await;
            loop {
                // wait until its time for our next report
                interval.tick().await;
                // log the parts this upload is still waiting on
                progress.log_stall(&span);
            }
        });
        StallWatchdog { handle }
    }
}

impl Drop for StallWatchdog {
    /// Abort our watchdog task so it never outlives the upload it is reporting on
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// The state used to upload the parts of a single multipart upload
///
/// Every multipart upload in this module shares the same plumbing: a semaphore bounding how
/// many parts may be in flight, the handles of the part uploads that have been spawned, and
/// the next part number to hand out. Bundling them keeps that plumbing, and all of the
/// progress logging that goes with it, in one place instead of duplicated in every helper.
struct MultipartTracker {
    /// The semaphore bounding how many part uploads may be in flight at once
    semaphore: Arc<Semaphore>,
    /// The progress of this upload shared with the part tasks and the stall watchdog
    progress: Arc<MultipartProgress>,
    /// The watchdog reporting on this upload, aborted when this tracker is dropped
    _watchdog: StallWatchdog,
    /// The part upload tasks that have been spawned
    tasks: Vec<JoinHandle<Result<CompletedPart, ApiError>>>,
    /// The part number to assign to the next part we upload
    part_num: i32,
}

impl MultipartTracker {
    /// Start tracking a new multipart upload
    ///
    /// # Arguments
    ///
    /// * `path` - The key in s3 this multipart upload is writing to
    /// * `upload_id` - The id of the multipart upload to track
    fn new(path: &str, upload_id: &str) -> Self {
        // build the progress shared by this uploads parts and its watchdog
        let progress = Arc::new(MultipartProgress::new(path, upload_id));
        // start reporting on this upload so a stall doesn't just go silent
        let watchdog = StallWatchdog::spawn(&progress);
        // log that this upload started so it can be matched against the s3 access logs
        event!(
            Level::INFO,
            msg = "Started multipart upload",
            path,
            upload_id
        );
        MultipartTracker {
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_UPLOADS)),
            progress,
            _watchdog: watchdog,
            tasks: Vec::new(),
            part_num: 1,
        }
    }

    /// Record that a chunk was read from the incoming request body
    ///
    /// # Arguments
    ///
    /// * `read` - The number of bytes that were read
    fn record_read(&self, read: usize) {
        self.progress.record_read(read);
    }
}

/// A S3 client wrapper
pub struct S3 {
    /// The s3 bucket for files
    pub files: S3Client,
    /// The s3 bucket for result files
    pub results: S3Client,
    /// The s3 bucket for ephemeral files
    pub ephemeral: S3Client,
    /// The s3 bucket for reaction cache files
    pub reaction_cache: S3Client,
    /// The s3 bucket for comment attachemnts
    pub attachments: S3Client,
    /// The s3 bucket for zipped repositories
    pub repos: S3Client,
    /// s3 clients for graphics
    pub graphics: S3Client,
}

impl S3 {
    /// Build all of our s3 clients
    pub fn new(config: &Conf) -> Self {
        // build our clients
        let files = S3Client::new(
            &config.thorium.files.bucket,
            &config.thorium.files.password,
            &config.thorium.s3,
        );
        let results = S3Client::new(
            &config.thorium.results.bucket,
            // these aren't password protected so just use the files password
            &config.thorium.files.password,
            &config.thorium.s3,
        );
        let ephemeral = S3Client::new(
            &config.thorium.ephemeral.bucket,
            // these aren't password protected so just use the files password
            &config.thorium.files.password,
            &config.thorium.s3,
        );
        let reaction_cache = S3Client::new(
            &config.thorium.reaction_cache.bucket,
            &config.thorium.reaction_cache.password,
            &config.thorium.s3,
        );
        let attachments = S3Client::new(
            &config.thorium.attachments.bucket,
            // these aren't password protected so just use the files password
            &config.thorium.files.password,
            &config.thorium.s3,
        );
        let repos = S3Client::new(
            &config.thorium.repos.bucket,
            // these aren't password protected so just use the files password
            &config.thorium.files.password,
            &config.thorium.s3,
        );
        // build all of the graphics s3 clients
        let graphics = S3Client::new(
            &config.thorium.graphics.bucket,
            // these aren't password protected so just use the files password
            &config.thorium.files.password,
            &config.thorium.s3,
        );
        S3 {
            files,
            results,
            ephemeral,
            reaction_cache,
            attachments,
            repos,
            graphics,
        }
    }
}

pub struct S3Client {
    /// The bucket to write files too
    pub bucket: String,
    /// The password used to encrypt files
    password: GenericArray<u8, U16>,
    /// The test aws sdk s3 client
    pub client: Client,
}

impl S3Client {
    /// builds new s3 clients
    ///
    /// # Arguments
    ///
    /// * `config` - Thorium config options
    #[must_use]
    pub fn new(bucket: &str, password: &str, conf: &crate::conf::S3) -> Self {
        // build our generic array
        let gen_array: GenericArray<u8, U16> =
            GenericArray::clone_from_slice(&password.as_bytes()[..16]);
        // get our s3 credentials
        let creds = Credentials::new(&conf.access_key, &conf.secret_token, None, None, "Thorium");
        // build our timeout config so a stalled request is bounded and retried instead of
        // hanging or failing the entire (potentially very large) upload
        let timeout_config = TimeoutConfig::builder()
            .connect_timeout(std::time::Duration::from_secs(conf.connect_timeout))
            .operation_attempt_timeout(std::time::Duration::from_secs(conf.attempt_timeout))
            .build();
        // build our s3 config, retrying transient failures so a single bad part upload
        // doesn't fail a large multipart upload
        let mut s3_config_builder = aws_sdk_s3::config::Builder::new()
            .endpoint_url(&conf.endpoint)
            .credentials_provider(SharedCredentialsProvider::new(creds))
            .force_path_style(conf.use_path_style)
            .timeout_config(timeout_config)
            .retry_config(RetryConfig::standard().with_max_attempts(conf.max_attempts));
        // if we have a region set then add that to our config
        if let Some(region) = &conf.region {
            // set our region
            s3_config_builder =
                s3_config_builder.region(aws_types::region::Region::new(region.clone()));
        }
        // build our s3 config
        let s3_config = s3_config_builder.build();
        // build our s3 client
        let client = Client::from_conf(s3_config);
        S3Client {
            bucket: bucket.to_owned(),
            password: gen_array,
            client,
        }
    }

    /// Check if a file exists in s3 by path
    ///
    /// # Arguments
    ///
    /// * `path` - The path to check against
    #[instrument(name = "S3Client::exists", skip(self), err(Debug))]
    pub async fn exists(&self, path: &str) -> Result<bool, ApiError> {
        // head this path to see if it exists
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(sdk_err) => match sdk_err.into_service_error() {
                HeadObjectError::NotFound(_) => Ok(false),
                err => Err(ApiError::from(err)),
            },
        }
    }

    /// List the objects with the given prefix, truncated to 10,000 keys maximum
    ///
    /// Returns a list of keys matching the given prefix with a maximum of 10,000
    ///
    /// # Caveats
    ///
    /// The client will return a maximum of 1000 keys per page, and this function
    /// will concatenate a maximum of 10 pages, so 10,000 keys total. If there
    /// are more objects than 10,000, those will not be included
    ///
    /// # Arguments
    ///
    /// * `prefix` - The prefix to check for objects
    #[instrument(name = "S3Client::list_truncated", skip(self), err(Debug))]
    pub async fn list_truncated(&self, prefix: &str) -> Result<Vec<String>, ApiError> {
        // store a continuation token
        let mut continuation_token = None;
        // store our keys
        let mut keys = Vec::new();
        // count how many pages we've gotten
        let mut page: u8 = 1;
        loop {
            // list objects with the given prefix
            let mut resp = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                // match on the given prefix
                .prefix(prefix)
                // explicitly set max keys to 1000 per page
                .max_keys(1000)
                // set the token
                .set_continuation_token(continuation_token)
                .send()
                .await?;
            keys.extend(
                resp.contents
                    .take()
                    .into_iter()
                    // flatten None to an empty Vec
                    .flatten()
                    // get the keys for each object
                    .filter_map(|object| object.key),
            );
            // increment our page count
            page += 1;
            // if we've gotten 10 pages or we don't have a next token, return our keys
            if page > 10 || resp.next_continuation_token.is_none() {
                return Ok(keys);
            }
            // we must have a continuation token, so set it for next loop
            continuation_token = resp.next_continuation_token;
        }
    }

    /// Upload a single part of a multipart upload in the background
    ///
    /// Acquiring a permit before spawning applies backpressure to the incoming request body:
    /// once `MAX_CONCURRENT_UPLOADS` parts are in flight, the read loop waits here rather than
    /// buffering the whole file in memory. Uploading on its own task then lets the part make
    /// progress while we keep reading the incoming body instead of stalling the read on it.
    ///
    /// # Arguments
    ///
    /// * `tracker` - The tracker for the multipart upload this part belongs to
    /// * `body` - The bytes to upload for this part
    async fn submit_part(
        &self,
        tracker: &mut MultipartTracker,
        body: Bytes,
    ) -> Result<(), ApiError> {
        // flag that we are waiting on a permit so a stall here shows up in our stall reports
        tracker.progress.permit_wait(true);
        // wait until fewer than MAX_CONCURRENT_UPLOADS parts are in flight; the semaphore is
        // never closed so this only errors if the runtime is shutting down
        let permit = Arc::clone(&tracker.semaphore)
            .acquire_owned()
            .await
            .map_err(|err| internal_err_unwrapped!(format!("s3 upload semaphore closed: {err}")))?;
        // we have our permit so we are no longer waiting on one
        tracker.progress.permit_wait(false);
        // clone the values the spawned task needs to own
        let client = self.client.clone();
        let bucket = self.bucket.clone();
        let progress = Arc::clone(&tracker.progress);
        // grab the part number and size we are uploading
        let part_num = tracker.part_num;
        let part_size = body.len();
        // build a span so this parts events stay attached to the trace of the request it came from
        let span = span!(Level::INFO, "S3Client::upload_part", part_num);
        // record that this part is now in flight
        tracker.progress.start_part(part_num, part_size);
        // log that this part is on its way to s3
        event!(
            parent: &span,
            Level::INFO,
            msg = "Submitting part",
            part_num,
            part_size,
            in_flight = tracker.progress.in_flight(),
        );
        // upload this part in the background so it overlaps with reading the incoming body
        let handle = tokio::spawn(async move {
            // hold our concurrency permit until this part is done uploading
            let _permit = permit;
            // upload this part to s3
            let uploaded = client
                .upload_part()
                .bucket(&bucket)
                .key(&progress.path)
                .upload_id(&progress.upload_id)
                .body(ByteStream::from(SdkBody::from(body)))
                .part_number(part_num)
                .send()
                .await;
            // log whether this part made it to s3 before handing it back
            match uploaded {
                Ok(part) => {
                    // this part is done so take it out of flight
                    let elapsed_ms = progress.finish_part(part_num);
                    // log that this part made it to s3
                    event!(
                        parent: &span,
                        Level::INFO,
                        msg = "Part uploaded",
                        part_num,
                        part_size,
                        elapsed_ms,
                        e_tag = part.e_tag().unwrap_or_default(),
                        in_flight = progress.in_flight(),
                    );
                    // build the completed part record so the caller can finish the upload
                    Ok(CompletedPart::builder()
                        .e_tag(part.e_tag.unwrap_or_default())
                        .part_number(part_num)
                        .build())
                }
                Err(error) => {
                    // this part is done so take it out of flight
                    let elapsed_ms = progress.fail_part(part_num);
                    // convert this into an api error so we get the full s3 error message
                    let error = ApiError::from(error);
                    // log that this part failed instead of waiting for the caller to join us
                    event!(
                        parent: &span,
                        Level::ERROR,
                        msg = "Part upload failed",
                        part_num,
                        part_size,
                        elapsed_ms,
                        error = error.to_string(),
                    );
                    Err(error)
                }
            }
        });
        // track this part upload so we can wait on it when completing this upload
        tracker.tasks.push(handle);
        // move on to the next part number
        tracker.part_num += 1;
        Ok(())
    }

    /// Wait for every spawned part upload to finish and complete the multipart upload
    ///
    /// # Arguments
    ///
    /// * `tracker` - The tracker for the multipart upload to complete
    #[rustfmt::skip]
    #[instrument(name = "S3Client::complete_multipart", skip_all, fields(path = tracker.progress.path.as_str(), upload_id = tracker.progress.upload_id.as_str()), err(Debug))]
    async fn complete_multipart(&self, tracker: MultipartTracker) -> Result<(), ApiError> {
        // break our tracker apart, keeping the watchdog alive so a part that stalls while we
        // wait here keeps getting reported instead of the logs just going quiet
        let MultipartTracker {
            progress,
            _watchdog,
            tasks,
            ..
        } = tracker;
        // log that every part has been submitted and we are just waiting on them now
        event!(
            Level::INFO,
            msg = "Waiting on parts to finish uploading",
            parts = tasks.len(),
            in_flight = progress.in_flight(),
        );
        // collect every uploaded part, surfacing any upload or task join error
        let mut parts = Vec::with_capacity(tasks.len());
        for task in tasks {
            // a join error means the upload task panicked or was cancelled
            let part = task
                .await
                .map_err(|err| internal_err_unwrapped!(format!("s3 upload task failed: {err}")))??;
            parts.push(part);
        }
        // parts can finish out of order so sort them by part number before completing
        parts.sort_by_key(|part| part.part_number().unwrap_or_default());
        // grab how many parts we uploaded before we hand them off
        let uploaded = parts.len();
        // build our complete multipart upload object
        let completed_parts = CompletedMultipartUpload::builder()
            .set_parts(Some(parts))
            .build();
        // log that every part is uploaded and we are about to complete this upload; pairing this
        // with the event below tells us whether a hang is in our parts or in the complete call
        event!(
            Level::INFO,
            msg = "Completing multipart upload",
            parts = uploaded,
            bytes_sent = progress.bytes_sent(),
            elapsed_ms = elapsed_ms(progress.start),
        );
        // finish this multipart upload
        self.client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(&progress.path)
            .multipart_upload(completed_parts)
            .upload_id(&progress.upload_id)
            .send()
            .await?;
        // log that this upload is fully done
        event!(
            Level::INFO,
            msg = "Multipart upload complete",
            parts = uploaded,
            bytes_sent = progress.bytes_sent(),
            elapsed_ms = elapsed_ms(progress.start),
        );
        Ok(())
    }

    /// Abort a multipart upload that failed part way through
    ///
    /// The error that caused the abort is always the one returned. An abort failing is worth
    /// logging but it must not replace the error that actually failed the upload.
    ///
    /// # Arguments
    ///
    /// * `path` - The key in s3 the aborted multipart upload was writing to
    /// * `upload_id` - The id of the multipart upload to abort
    /// * `error` - The error that caused this upload to be aborted
    #[instrument(name = "S3Client::abort_multipart", skip(self, error))]
    async fn abort_multipart(&self, path: &str, upload_id: &str, error: ApiError) -> ApiError {
        // log the failure that is causing us to abort this upload
        event!(
            Level::ERROR,
            msg = "Aborting multipart upload",
            error = error.to_string(),
        );
        // abort this multipart upload so s3 doesn't hold onto its parts
        let aborted = self
            .client
            .abort_multipart_upload()
            .bucket(&self.bucket)
            .key(path)
            .upload_id(upload_id)
            .send()
            .await;
        // log any abort failure but keep returning the error that failed this upload
        if let Err(abort_error) = aborted {
            event!(
                Level::ERROR,
                msg = "Failed to abort multipart upload",
                error = ApiError::from(abort_error).to_string(),
            );
        }
        error
    }

    /// Stream a file into s3 while hashing and carting it
    ///
    /// # Arguments
    ///
    /// * `path` - The path to write this object to in s3
    /// * `upload_id` - The id of the multipart upload being used
    /// * `field` - The field to stream to s3
    #[instrument(
        name = "S3Client::hash_cart_and_stream_helper",
        skip(self, field),
        err(Debug)
    )]
    async fn hash_cart_and_stream_helper<'a>(
        &self,
        path: &str,
        upload_id: &str,
        mut field: Field<'a>,
    ) -> Result<StandardHashes, ApiError> {
        // init our cart streamer and hashers
        let mut cart = CartStreamManual::new(&self.password, 7_242_880)?;
        let mut hashers = StandardHashers::default();
        // track this uploads parts so we can bound, log, and wait on them
        let mut tracker = MultipartTracker::new(path, upload_id);
        // stream this fields data through our hashers, cart, and upload carted parts concurrently
        while let Some(raw) = field.chunk().await? {
            // track how much of the incoming body we have read
            tracker.record_read(raw.len());
            // pass this chunk through our hashers
            hashers.digest(&raw);
            // add this buffer to our cart streamer
            if cart.next_bytes(raw)? {
                // keep processing these bytes until they are finished
                while cart.process()? {
                    // if our input buffer is full then pack
                    if cart.ready() >= 5_242_880 {
                        // copy the carted bytes out so the upload task can own them while
                        // we keep carting the rest of the file
                        let chunk = Bytes::copy_from_slice(cart.carted_bytes());
                        // consume the bytes we have copied out
                        cart.consume();
                        // upload this carted part in the background, applying backpressure to
                        // the incoming body if too many uploads are already in flight
                        self.submit_part(&mut tracker, chunk).await?;
                    }
                }
            }
        }
        // finish carting our file and upload the final part
        let chunk = Bytes::copy_from_slice(cart.finish()?);
        self.submit_part(&mut tracker, chunk).await?;
        // wait for every part to finish uploading and complete the multipart upload
        self.complete_multipart(tracker).await?;
        Ok(hashers.finish())
    }

    /// Stream a file into s3 while hashing and carting it
    ///
    /// # Arguments
    ///
    /// * `s3_id` - The id to use for this object in s3
    /// * `field` - The field to stream to s3
    #[instrument(name = "S3Client::hash_cart_and_stream", skip(self, field), err(Debug))]
    pub async fn hash_cart_and_stream<'a>(
        &self,
        s3_id: &Uuid,
        field: Field<'a>,
    ) -> Result<StandardHashes, ApiError> {
        // build the path to write this file too
        let path = s3_id.to_string();
        // initiate a multipart upload to s3
        let init = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(&path)
            .content_type("application/octet-stream")
            .send()
            .await?;
        // get our upload id
        let upload_id = match init.upload_id() {
            Some(upload_id) => upload_id,
            None => return unavailable!("Failed to get multipart upload ID".to_owned()),
        };
        // cart and stream this file to s3
        match self
            .hash_cart_and_stream_helper(&path, upload_id, field)
            .await
        {
            Ok(hashes) => Ok(hashes),
            // abort this multipart upload and return the error that failed it
            Err(error) => Err(self.abort_multipart(&path, upload_id, error).await),
        }
    }

    /// Helps stream a file into s3 while sha256 and carting it
    ///
    /// # Arguments
    ///
    /// * `path` - The path to write this object to in s3
    /// * `upload_id` - The id of the multipart upload being used
    /// * `field` - The field to stream to s3
    #[instrument(
        name = "S3Client::sha256_cart_and_stream_helper",
        skip(self, field),
        err(Debug)
    )]
    async fn sha256_cart_and_stream_helper<'a>(
        &self,
        path: &str,
        upload_id: &str,
        mut field: Field<'a>,
    ) -> Result<String, ApiError> {
        // init our cart streamer and hashers
        let mut cart = CartStreamManual::new(&self.password, 7_242_880)?;
        let mut sha256 = Sha256::new();
        // track this uploads parts so we can bound, log, and wait on them
        let mut tracker = MultipartTracker::new(path, upload_id);
        // stream this fields data through our hasher, cart, and upload carted parts concurrently
        while let Some(raw) = field.chunk().await? {
            // track how much of the incoming body we have read
            tracker.record_read(raw.len());
            // pass this chunk through our hasher
            sha256.update(&raw);
            // add this buffer to our cart streamer
            if cart.next_bytes(raw)? {
                // keep processing these bytes until they are finished
                while cart.process()? {
                    // if our input buffer is full then pack
                    if cart.ready() >= 5_242_880 {
                        // copy the carted bytes out so the upload task can own them while
                        // we keep carting the rest of the file
                        let chunk = Bytes::copy_from_slice(cart.carted_bytes());
                        // consume the bytes we have copied out
                        cart.consume();
                        // upload this carted part in the background, applying backpressure to
                        // the incoming body if too many uploads are already in flight
                        self.submit_part(&mut tracker, chunk).await?;
                    }
                }
            }
        }
        // finish carting our file and upload the final part
        let chunk = Bytes::copy_from_slice(cart.finish()?);
        self.submit_part(&mut tracker, chunk).await?;
        // wait for every part to finish uploading and complete the multipart upload
        self.complete_multipart(tracker).await?;
        // get our final sha256 hash
        Ok(HEXLOWER.encode(&sha256.finalize()))
    }

    /// Stream a file into s3 while getting its sha256 and carting it
    ///
    /// # Arguments
    ///
    /// * `s3_id` - The id to use for this object in s3
    /// * `field` - The field to stream to s3
    #[instrument(
        name = "S3Client::sha256_cart_and_stream",
        skip(self, field),
        err(Debug)
    )]
    pub async fn sha256_cart_and_stream<'a>(
        &self,
        s3_id: &Uuid,
        field: Field<'a>,
    ) -> Result<String, ApiError> {
        // build the path to write this file too
        let path = s3_id.to_string();
        // initiate a multipart upload to s3
        let init = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(&path)
            .content_type("application/octet-stream")
            .send()
            .await?;
        // get our upload id
        let upload_id = match init.upload_id() {
            Some(upload_id) => upload_id,
            None => return unavailable!("Failed to get multipart upload ID".to_owned()),
        };
        // cart and stream this file to s3
        match self
            .sha256_cart_and_stream_helper(&path, upload_id, field)
            .await
        {
            Ok(sha256) => Ok(sha256),
            // abort this multipart upload and return the error that failed it
            Err(error) => Err(self.abort_multipart(&path, upload_id, error).await),
        }
    }

    /// Stream a file into s3 after carting it
    ///
    /// # Arguments
    ///
    /// * `path` - The path to write this object to in s3
    /// * `upload_id` - The id of the multipart upload being used
    /// * `field` - The field to stream to s3
    #[instrument(
        name = "S3Client::cart_and_stream_helper",
        skip(self, field),
        err(Debug)
    )]
    async fn cart_and_stream_helper<'a>(
        &self,
        path: &str,
        upload_id: &str,
        mut field: Field<'a>,
    ) -> Result<(), ApiError> {
        // init our cart streamer
        let mut cart = CartStreamManual::new(&self.password, 7_242_880)?;
        // track this uploads parts so we can bound, log, and wait on them
        let mut tracker = MultipartTracker::new(path, upload_id);
        // stream this fields data through our cart and upload carted parts concurrently
        while let Some(raw) = field.chunk().await? {
            // track how much of the incoming body we have read
            tracker.record_read(raw.len());
            // add this buffer to our cart streamer
            if cart.next_bytes(raw)? {
                // keep processing these bytes until they are finished
                while cart.process()? {
                    // if our input buffer is full then pack
                    if cart.ready() >= 5_242_880 {
                        // copy the carted bytes out so the upload task can own them while
                        // we keep carting the rest of the file
                        let chunk = Bytes::copy_from_slice(cart.carted_bytes());
                        // consume the bytes we have copied out
                        cart.consume();
                        // upload this carted part in the background, applying backpressure to
                        // the incoming body if too many uploads are already in flight
                        self.submit_part(&mut tracker, chunk).await?;
                    }
                }
            }
        }
        // finish carting our file and upload the final part
        let chunk = Bytes::copy_from_slice(cart.finish()?);
        self.submit_part(&mut tracker, chunk).await?;
        // wait for every part to finish uploading and complete the multipart upload
        self.complete_multipart(tracker).await
    }

    /// Stream a file into s3 after carting it
    ///
    /// # Arguments
    ///
    /// * `path` - The path to write this file to in s3
    /// * `field` - The field to stream to s3
    #[instrument(name = "S3Client::cart_and_stream", skip(self, field), err(Debug))]
    pub async fn cart_and_stream<'a, P: Into<String> + std::fmt::Debug>(
        &self,
        path: P,
        field: Field<'a>,
    ) -> Result<(), ApiError> {
        // convert our path into a string
        let path = path.into();
        // initiate a multipart upload to s3
        let init = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(&path)
            .content_type("application/octet-stream")
            .send()
            .await?;
        // get our upload id
        let upload_id = match init.upload_id() {
            Some(upload_id) => upload_id,
            None => return unavailable!("Failed to get multipart upload ID".to_owned()),
        };
        // cart and stream this file to s3
        match self.cart_and_stream_helper(&path, upload_id, field).await {
            Ok(()) => Ok(()),
            // abort this multipart upload and return the error that failed it
            Err(error) => Err(self.abort_multipart(&path, upload_id, error).await),
        }
    }

    /// Stream a file into s3 without carting it
    ///
    /// # Arguments
    ///
    /// * `path` - The path to write this object to in s3
    /// * `upload_id` - The id of the multipart upload being used
    /// * `field` - The field to stream to s3
    #[instrument(name = "S3Client::stream_helper", skip(self, field), err(Debug))]
    pub async fn stream_helper<'a>(
        &self,
        path: &str,
        upload_id: &str,
        mut field: Field<'a>,
    ) -> Result<(), ApiError> {
        // track this uploads parts so we can bound, log, and wait on them
        let mut tracker = MultipartTracker::new(path, upload_id);
        // buffer incoming bytes until we have a full part sized chunk to upload
        let mut buffer = BytesMut::with_capacity(PART_SIZE);
        // stream this fields data into part sized buffers and upload them concurrently
        while let Some(raw) = field.chunk().await? {
            // track how much of the incoming body we have read
            tracker.record_read(raw.len());
            // add our chunk to our part buffer
            buffer.extend_from_slice(&raw);
            // once we have a full part upload it in the background
            if buffer.len() >= PART_SIZE {
                // take the buffered bytes as an owned chunk, leaving capacity for the next part
                let chunk = buffer.split().freeze();
                // upload this part in the background, applying backpressure to the incoming
                // body if too many uploads are already in flight
                self.submit_part(&mut tracker, chunk).await?;
            }
        }
        // upload whatever is left as the final part; it has the highest part number so s3
        // allows it to be smaller than the 5 MiB minimum
        let chunk = buffer.split().freeze();
        self.submit_part(&mut tracker, chunk).await?;
        // wait for every part to finish uploading and complete the multipart upload
        self.complete_multipart(tracker).await
    }

    /// Stream a file into s3
    ///
    /// # Arguments
    ///
    /// * `s3_id` - The id to use for this object in s3
    /// * `field` - The field to stream to s3
    #[instrument(name = "S3Client::stream", skip(self, field), err(Debug))]
    pub async fn stream<'a>(&self, path: &str, field: Field<'a>) -> Result<(), ApiError> {
        // ban any paths that might contain traversal attacks
        if path.contains("..") {
            return bad!("S3 file names cannot contain '..'".to_owned());
        }
        // initiate a multipart upload to s3
        let init = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(path)
            .content_type("application/octet-stream")
            .send()
            .await?;
        // get our upload id
        let upload_id = match init.upload_id() {
            Some(upload_id) => upload_id,
            None => return unavailable!("Failed to get multipart upload ID".to_owned()),
        };
        // cart and stream this file to s3
        match self.stream_helper(path, upload_id, field).await {
            Ok(()) => Ok(()),
            // abort this multipart upload and return the error that failed it
            Err(error) => Err(self.abort_multipart(path, upload_id, error).await),
        }
    }

    /// Stream a file into s3 with the given content type
    ///
    /// # Arguments
    ///
    /// * `s3_id` - The id to use for this object in s3
    /// * `field` - The field to stream to s3
    /// * `content_type` - The content type to set for this file
    #[instrument(
        name = "S3Client::stream_with-content_type",
        skip(self, field),
        err(Debug)
    )]
    pub async fn stream_with_content_type<'a>(
        &self,
        path: &str,
        field: Field<'a>,
        content_type: &str,
    ) -> Result<(), ApiError> {
        // ban any paths that might contain traversal attacks
        if path.contains("..") {
            return bad!("S3 file names cannot contain '..'".to_owned());
        }
        // initiate a multipart upload to s3
        let init = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(path)
            .content_type(content_type)
            .send()
            .await?;
        // get our upload id
        let upload_id = match init.upload_id() {
            Some(upload_id) => upload_id,
            None => return unavailable!("Failed to get multipart upload ID".to_owned()),
        };
        // cart and stream this file to s3
        match self.stream_helper(path, upload_id, field).await {
            Ok(()) => Ok(()),
            // abort this multipart upload and return the error that failed it
            Err(error) => Err(self.abort_multipart(path, upload_id, error).await),
        }
    }

    /// decodes a base64 stream and uploads it to s3
    ///
    /// # Arguments
    ///
    /// * `path` - The path to upload this file to
    /// * `encoded` - The base64 encoded data to upload
    #[instrument(name = "S3Client::upload_base_64", skip(self, encoded), err(Debug))]
    pub async fn upload_base64(&self, path: &str, encoded: &str) -> Result<(), ApiError> {
        // log the size of our encoded data
        event!(Level::INFO, encoded_size = encoded.len());
        // ban any paths that might contain traversal attacks
        if path.contains("..") {
            return bad!("S3 file names cannot contain '..'".to_owned());
        }
        // decode our base64'd bytes
        let decoded = base64::engine::general_purpose::STANDARD.decode(encoded)?;
        // decode this file
        let decoded_stream = ByteStream::from(decoded);
        // write this file to s3
        if !self.exists(path).await? {
            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(path)
                .body(decoded_stream)
                .send()
                .await?;
        }
        Ok(())
    }

    /// download a file from s3
    ///
    /// # Arguments
    ///
    /// * `path` - The path to an object in s3
    #[instrument(name = "S3Client::download", skip(self), err(Debug))]
    pub async fn download(&self, path: &str) -> Result<ByteStream, ApiError> {
        // start downloading this file and stream it to the user
        let body = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await?
            .body;
        Ok(body)
    }

    /// Download an object from s3 with its metadata intact
    ///
    /// # Arguments
    ///
    /// * `path` - The path to an object in s3
    #[instrument(name = "S3Client::download_with_metadata", skip(self), err(Debug))]
    pub async fn download_with_metadata(&self, path: &str) -> Result<GetObjectOutput, ApiError> {
        // start downloading this file and stream it to the user
        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await?;
        Ok(output)
    }

    /// download a file from s3 and convert it to an encrypted zip
    ///
    /// This is not near as efficient as using CaRT and should not be used for large files.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to an object in s3
    #[instrument(name = "S3Client::download_as_zip", skip(self, shared), err(Debug))]
    pub async fn download_as_zip(
        &self,
        path: &str,
        sha256: &str,
        params: ZipDownloadParams,
        shared: &Shared,
    ) -> Result<Vec<u8>, ApiError> {
        // start downloading this file and stream it to the user
        let body = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await?
            .body;
        // get the password to use
        let password = params.get_password(shared).as_bytes();
        // setup our zip options
        let opts = zip::write::SimpleFileOptions::default().with_deprecated_encryption(password);
        // build our writer
        let mut writer = ZipWriter::new(std::io::Cursor::new(vec![]));
        // start our file
        writer.start_file(sha256, opts)?;
        // build our uncart stream object
        let mut uncart_stream = UncartStream::new(body.into_async_read());
        // build a vector to store our entire file that defaults to 1 mebibyte in size
        let mut uncarted = Vec::with_capacity(1_048_576);
        // uncart the entire file
        tokio::io::copy(&mut uncart_stream, &mut uncarted).await?;
        // spawn this task in a tokio task and wait for it to complete
        tokio::task::spawn_blocking(move || {
            // zip this file
            match writer.write_all(&uncarted) {
                // get our zipped data
                Ok(_) => match writer.finish() {
                    Ok(zipped) => Ok(zipped.into_inner()),
                    Err(err) => Err(ApiError::from(err)),
                },
                Err(err) => Err(ApiError::from(err)),
            }
        })
        .await?
    }

    /// deletes a file from s3
    ///
    /// # Arguments
    ///
    /// * `path` - The path of the file to delete
    #[instrument(name = "S3Client::delete", skip(self), err(Debug))]
    pub async fn delete(&self, path: &str) -> Result<(), ApiError> {
        // try to delete this object from s3
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await?;
        Ok(())
    }

    /// Delete all objects with the given prefix, truncated to 10,000 keys maximum
    ///
    /// Returns a list of keys that were deleted
    ///
    /// # Caveats
    ///
    /// The client will list a maximum of 1000 keys per page, and this function
    /// will concatenate a maximum of 10 pages, so 10,000 keys total. If there
    /// are more objects than 10,000, those will not be deleted
    ///
    /// # Arguments
    ///
    /// * `prefix` - The prefix to check for objects to delete
    #[instrument(name = "S3Client::delete_bulk_truncated", skip(self), err(Debug))]
    pub async fn delete_bulk_truncated(&self, prefix: &str) -> Result<Vec<String>, ApiError> {
        // store a continuation token
        let mut continuation_token = None;
        // store our keys
        let mut keys = Vec::new();
        // count how many pages we've gotten
        let mut page: u8 = 1;
        loop {
            // list objects with the given prefix
            let mut resp = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                // match on the given prefix
                .prefix(prefix)
                // explicitly set max keys to 1000 per page
                .max_keys(1000)
                // set the token
                .set_continuation_token(continuation_token)
                .send()
                .await?;
            // make a list of ObjectIdentifiers for deletion
            let object_identifiers: Vec<ObjectIdentifier> = resp
                .contents
                .take()
                .into_iter()
                .flatten()
                .filter_map(|object| {
                    object
                        .key()
                        // safe to unwrap because we're setting the key
                        .map(|key| ObjectIdentifier::builder().key(key).build().unwrap())
                })
                .collect();
            if !object_identifiers.is_empty() {
                // Delete objects in bulk
                let delete = Delete::builder()
                    .set_objects(Some(object_identifiers))
                    .build()
                    // safe to unwrap because we're setting objects above
                    .unwrap();
                // delete the objects
                let mut delete_resp = self
                    .client
                    .delete_objects()
                    .bucket(&self.bucket)
                    .delete(delete)
                    .send()
                    .await?;
                // add the deleted keys to our list
                keys.extend(
                    delete_resp
                        .deleted
                        .take()
                        .into_iter()
                        // flatten None to an empty Vec
                        .flatten()
                        // get the keys for each object
                        .filter_map(|object| object.key),
                );
            }
            // increment our page count
            page += 1;
            // if we've gotten 10 pages or we don't have a next token, return our keys
            if page > 10 || resp.next_continuation_token.is_none() {
                return Ok(keys);
            }
            // we must have a continuation token, so set it for next loop
            continuation_token = resp.next_continuation_token;
        }
    }
}

/// s3 clients pointing to buckets containing graphics
pub struct GraphicsS3Client {
    /// The s3 client for graphics
    pub client: S3Client,
}

impl GraphicsS3Client {
    pub fn new(config: &Conf) -> Self {
        // build all of the graphics s3 clients
        let client = S3Client::new(
            &config.thorium.graphics.bucket,
            // these aren't password protected so just use the files password
            &config.thorium.files.password,
            &config.thorium.s3,
        );
        Self { client }
    }
}
