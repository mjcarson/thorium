//! Tracks the progress of the files we stream up to Thorium
//!
//! Once a file is handed to reqwest as a stream nothing in this crate is driving its transfer
//! anymore. Network backpressure does not surface as a [`std::task::Poll::Pending`] either, our
//! stream simply stops being polled at all, so a hung upload cannot be detected from inside the
//! stream. That is why every file gets a watchdog on its own task while it is streaming.
//!
//! Two caveats are worth knowing before trusting these numbers:
//!
//! - `bytes_sent` counts the bytes we handed to hyper, not the bytes that made it onto the wire.
//!   Hyper buffers, so the trustworthy signal is the pair of these events and the API's matching
//!   `"Uploading result file"` events, not either side on its own.
//! - The first file in a request would otherwise absorb the connect, the TLS handshake, and the
//!   time the API spent reading the text fields that came before it. That is why each file tracks
//!   both when it was queued and when it was first polled.

// everything in here needs the tracing crate, which only the client-trace feature pulls in
#[cfg(feature = "client-trace")]
mod tracked {
    use bytes::BytesMut;
    use futures::Stream;
    use std::io;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
    use std::task::{Context, Poll};
    use std::time::{Duration, Instant};
    use tokio::task::JoinHandle;
    use tracing::level_filters::LevelFilter;
    use tracing::{Level, Span, event};

    /// How often a stall watchdog reports on the transfer it is watching
    ///
    /// A transfer that hangs stops being polled entirely, so anything logged from inside the
    /// stream goes silent exactly when an upload gets stuck. This watchdog ticks on its own task
    /// so a stalled transfer keeps telling us how far it got and how long it has been there.
    const STALL_LOG_INTERVAL: Duration = Duration::from_secs(30);

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

    /// Get how many bytes a second a transfer has averaged
    ///
    /// # Arguments
    ///
    /// * `bytes` - The number of bytes that have been transferred
    /// * `elapsed_ms` - How many milliseconds those bytes took
    fn bytes_per_sec(bytes: u64, elapsed_ms: u64) -> u64 {
        // a transfer that finished within the same millisecond has no measurable duration
        bytes.saturating_mul(1000) / elapsed_ms.max(1)
    }

    /// Something a [`StallWatchdog`] can periodically report on
    ///
    /// Both a single file being streamed and the request that file is part of can stall, and the
    /// two stall in different places for different reasons. They share one watchdog and differ
    /// only in what they have to say when it ticks.
    pub(crate) trait StallReport {
        /// Log what this operation is still waiting on
        fn log_stall(&self);
    }

    /// A guard over the background task reporting on a stalled transfer
    ///
    /// The watchdog is aborted when this guard is dropped so it can never outlive the transfer it
    /// is reporting on, including on the paths where that transfer errors out part way through.
    struct StallWatchdog {
        /// The handle of the spawned watchdog task
        handle: JoinHandle<()>,
    }

    impl StallWatchdog {
        /// Spawn a watchdog that periodically reports on a transfer that may stall
        ///
        /// This returns [`None`] rather than a watchdog when there is nothing to report to. Every
        /// by path upload in the client runs through here, including in test suites that never
        /// install a subscriber, so we only burn a task when someone is actually listening. It
        /// also returns [`None`] when we are somehow off of a tokio runtime instead of panicking
        /// in the middle of an upload.
        ///
        /// # Arguments
        ///
        /// * `progress` - The progress of the transfer to report on
        pub(crate) fn spawn<T>(progress: &Arc<T>) -> Option<Self>
        where
            T: StallReport + Send + Sync + 'static,
        {
            // skip the watchdog entirely if nobody is subscribed to what it would say
            if LevelFilter::current() < Level::INFO {
                return None;
            }
            // we can only spawn a watchdog if we have a runtime to spawn it onto
            let runtime = tokio::runtime::Handle::try_current().ok()?;
            // clone the progress the watchdog task needs to own
            let progress = Arc::clone(progress);
            // report on this transfer until it finishes and we get aborted
            let handle = runtime.spawn(async move {
                // build the interval we report on
                let mut interval = tokio::time::interval(STALL_LOG_INTERVAL);
                // the first tick of an interval completes immediately so burn it
                interval.tick().await;
                loop {
                    // wait until its time for our next report
                    interval.tick().await;
                    // report on how far this transfer has gotten
                    progress.log_stall();
                }
            });
            Some(StallWatchdog { handle })
        }
    }

    impl Drop for StallWatchdog {
        /// Abort our watchdog task so it never outlives the transfer it is reporting on
        fn drop(&mut self) {
            self.handle.abort();
        }
    }

    /// The mutable progress of a single file being streamed
    struct ProgressState {
        /// The total number of bytes of this file that have been handed to hyper so far
        bytes_sent: u64,
        /// The number of chunks this file has been read in so far
        chunks: usize,
        /// When we last handed a chunk of this file to hyper
        last_chunk: Instant,
    }

    /// The progress of a single file being streamed into a multipart form
    ///
    /// This is shared between the task polling the file's stream and the stall watchdog, so its
    /// mutable state sits behind a lock. That lock is only ever held for a handful of field
    /// updates and is never held across an await point, so a blocking mutex is safe here.
    pub(crate) struct FileProgress {
        /// The name this file is being uploaded under
        file_name: String,
        /// The total number of bytes we told the API to expect for this file
        total: u64,
        /// When this file was added to the form
        queued_at: Instant,
        /// When hyper first asked us for a chunk of this file
        started_at: OnceLock<Instant>,
        /// The span this file's events are logged under
        ///
        /// Hyper polls the request body on a different task where [`Span::current`] is empty, so
        /// a span opened from inside the stream would be a root span and every event would lose
        /// the sample and tool it belongs to. Capturing the span up front is what keeps each
        /// file's events attributable.
        span: Span,
        /// The mutable progress shared with the stall watchdog
        state: Mutex<ProgressState>,
    }

    impl FileProgress {
        /// Build the progress tracker for a file we are about to stream
        ///
        /// # Arguments
        ///
        /// * `file_name` - The name this file is being uploaded under
        /// * `total` - The total number of bytes we expect to stream
        fn new(file_name: &str, total: u64) -> Self {
            FileProgress {
                file_name: file_name.to_owned(),
                total,
                queued_at: Instant::now(),
                started_at: OnceLock::new(),
                span: Span::current(),
                state: Mutex::new(ProgressState {
                    bytes_sent: 0,
                    chunks: 0,
                    last_chunk: Instant::now(),
                }),
            }
        }

        /// Lock our progress state, recovering from a poisoned lock
        ///
        /// This state only exists to diagnose stalled uploads, so a panic while it is held should
        /// not take down every upload that comes after it.
        fn state(&self) -> MutexGuard<'_, ProgressState> {
            self.state.lock().unwrap_or_else(PoisonError::into_inner)
        }

        /// Get when this file actually started streaming
        ///
        /// Files that were never polled fall back to when they were queued so our timings are
        /// still bounded by something real.
        fn started(&self) -> Instant {
            self.started_at.get().copied().unwrap_or(self.queued_at)
        }

        /// Record that hyper has started reading this file and log that it is on the wire
        ///
        /// This is only ever called once, on this file's first poll.
        fn start(&self) {
            // stamp when we were first polled so our timings don't include the files before us
            let _ = self.started_at.set(Instant::now());
            // log that this file is the one being streamed now
            event!(
                parent: &self.span,
                Level::INFO,
                msg = "Streaming upload file",
                file_name = self.file_name.as_str(),
                total_bytes = self.total,
                queued_ms_ago = elapsed_ms(self.queued_at),
            );
        }

        /// Record that a chunk of this file was handed to hyper
        ///
        /// # Arguments
        ///
        /// * `sent` - The number of bytes that were handed over
        fn record_chunk(&self, sent: usize) {
            // track how much we have handed over and when we last handed anything over
            let mut state = self.state();
            state.bytes_sent += sent as u64;
            state.chunks += 1;
            state.last_chunk = Instant::now();
        }

        /// Log that this file finished streaming
        ///
        /// A file that streamed fewer bytes than the content length we already told the API to
        /// expect is worth shouting about. That content length is snapshotted when the form is
        /// built but the bytes are read later, so a file that was truncated in between ends the
        /// body short, hyper aborts the write, and the API sits waiting on a chunk that will
        /// never come until our upload timeout finally fires hours later. That hang is otherwise
        /// indistinguishable from a network stall.
        fn finish(&self) {
            // copy out the values we want to report on
            let state = self.state();
            let (bytes_sent, chunks) = (state.bytes_sent, state.chunks);
            // drop our lock before logging
            drop(state);
            // measure how long this file took from when we were first polled
            let elapsed_ms = elapsed_ms(self.started());
            // report that this file made it to hyper
            event!(
                parent: &self.span,
                Level::INFO,
                msg = "Upload file streamed",
                file_name = self.file_name.as_str(),
                bytes_sent,
                chunks,
                elapsed_ms,
                bytes_per_sec = bytes_per_sec(bytes_sent, elapsed_ms),
            );
            // warn if we sent a different number of bytes then the content length we promised
            if bytes_sent != self.total {
                event!(
                    parent: &self.span,
                    Level::WARN,
                    msg = "Upload file length mismatch",
                    file_name = self.file_name.as_str(),
                    bytes_sent,
                    total_bytes = self.total,
                );
            }
        }

        /// Log that we failed to read this file off of disk
        ///
        /// # Arguments
        ///
        /// * `error` - The error we ran into while reading this file
        fn fail(&self, error: &io::Error) {
            // render our error so it can be logged as a field
            let error = error.to_string();
            // report how far we got before this file failed to read
            event!(
                parent: &self.span,
                Level::ERROR,
                msg = "Upload file read failed",
                file_name = self.file_name.as_str(),
                bytes_sent = self.state().bytes_sent,
                total_bytes = self.total,
                elapsed_ms = elapsed_ms(self.started()),
                error = error.as_str(),
            );
        }

        /// Log that this file was dropped before it finished streaming
        ///
        /// This is the event that names the file a timed out or reset upload died on.
        fn abandon(&self) {
            // report how far this file got before it was dropped
            event!(
                parent: &self.span,
                Level::WARN,
                msg = "Upload file abandoned before completion",
                file_name = self.file_name.as_str(),
                bytes_sent = self.state().bytes_sent,
                total_bytes = self.total,
                elapsed_ms = elapsed_ms(self.started()),
            );
        }
    }

    impl StallReport for FileProgress {
        /// Log how far this file has gotten and how long it has been since it last moved
        ///
        /// A hung upload shows up here as a `last_chunk_ms_ago` that keeps growing while
        /// `bytes_sent` stays put.
        fn log_stall(&self) {
            // copy out the values we want to report on
            let state = self.state();
            let (bytes_sent, chunks) = (state.bytes_sent, state.chunks);
            let last_chunk_ms_ago = elapsed_ms(state.last_chunk);
            // drop our lock before logging
            drop(state);
            // report on what this file is still waiting on
            event!(
                parent: &self.span,
                Level::INFO,
                msg = "Upload file in progress",
                file_name = self.file_name.as_str(),
                bytes_sent,
                total_bytes = self.total,
                chunks,
                last_chunk_ms_ago,
                elapsed_ms = elapsed_ms(self.started()),
            );
        }
    }

    /// The progress of a request we are waiting on a response for
    ///
    /// Once the last file's stream ends its watchdog dies with it, but the request is not over.
    /// The API still has to finish its own upload to s3 before it answers us, and that is exactly
    /// where these uploads have been getting stuck. This keeps reporting through that window.
    struct RequestProgress {
        /// A short description of the request we are waiting on
        what: String,
        /// When we handed this request off to reqwest
        start: Instant,
        /// The span this request's events are logged under
        span: Span,
    }

    impl RequestProgress {
        /// Start tracking a request we are about to send
        ///
        /// # Arguments
        ///
        /// * `what` - A short description of the request being sent
        fn new(what: &str) -> Self {
            RequestProgress {
                what: what.to_owned(),
                start: Instant::now(),
                span: Span::current(),
            }
        }
    }

    impl StallReport for RequestProgress {
        /// Log that we are still waiting on a response for this request
        fn log_stall(&self) {
            // report on how long we have been waiting for this request to come back
            event!(
                parent: &self.span,
                Level::INFO,
                msg = "Request in progress",
                what = self.what.as_str(),
                elapsed_ms = elapsed_ms(self.start),
            );
        }
    }

    /// A guard over the reports on a request we are waiting for a response to
    ///
    /// This is dropped once the response comes back, which stops the watchdog behind it.
    pub(crate) struct RequestWatch {
        /// The progress shared with our stall watchdog
        progress: Arc<RequestProgress>,
        /// The watchdog reporting on this request, aborted when this guard is dropped
        _watchdog: Option<StallWatchdog>,
    }

    impl RequestWatch {
        /// Start reporting on a request we are about to send
        ///
        /// # Arguments
        ///
        /// * `what` - A short description of the request being sent
        pub(crate) fn start(what: &str) -> Self {
            // build the progress this request and its watchdog share
            let progress = Arc::new(RequestProgress::new(what));
            // start reporting on this request so a stalled response doesn't just go silent
            let watchdog = StallWatchdog::spawn(&progress);
            // log that this request is on its way
            event!(
                parent: &progress.span,
                Level::DEBUG,
                msg = "Sending request",
                what,
            );
            RequestWatch {
                progress,
                _watchdog: watchdog,
            }
        }

        /// Log that this request came back and stop reporting on it
        ///
        /// # Arguments
        ///
        /// * `success` - Whether this request came back successfully
        pub(crate) fn finish(self, success: bool) {
            // report how long we waited on this request
            event!(
                parent: &self.progress.span,
                Level::INFO,
                msg = "Request sent",
                what = self.progress.what.as_str(),
                success,
                elapsed_ms = elapsed_ms(self.progress.start),
            );
        }
    }

    /// A file stream that reports on its own progress as hyper drains it
    ///
    /// [`tokio_util::codec::FramedRead`] over a [`tokio::fs::File`] is [`Unpin`], so this bounds
    /// its stream on [`Unpin`] and skips pinning projections entirely.
    pub struct TrackedUpload<S> {
        /// The file stream we are wrapping
        inner: S,
        /// The progress shared with our stall watchdog
        progress: Arc<FileProgress>,
        /// The watchdog reporting on this file, aborted when this stream is dropped
        watchdog: Option<StallWatchdog>,
        /// Whether this file has finished streaming one way or another
        ///
        /// Reqwest drops each part as soon as it drains it, so every file gets dropped on the
        /// happy path too. Without this flag every single upload would warn that it was
        /// abandoned and the warning that actually matters would be lost in the noise.
        finished: bool,
    }

    impl<S> TrackedUpload<S> {
        /// Start tracking a file we are about to stream
        ///
        /// # Arguments
        ///
        /// * `inner` - The file stream to track
        /// * `file_name` - The name this file is being uploaded under
        /// * `total` - The total number of bytes we expect to stream
        fn new(inner: S, file_name: &str, total: u64) -> Self {
            // build the progress this file and its watchdog share
            let progress = Arc::new(FileProgress::new(file_name, total));
            // log that this file is queued so we can tell a slow upload from a large form
            event!(
                parent: &progress.span,
                Level::DEBUG,
                msg = "Queued upload file",
                file_name,
                total_bytes = total,
            );
            TrackedUpload {
                inner,
                progress,
                watchdog: None,
                finished: false,
            }
        }

        /// Mark this file as done and stop reporting on it
        fn finish(&mut self) {
            // this file is done so a drop from here on is not an abandonment
            self.finished = true;
            // stop reporting on a file that is no longer moving
            self.watchdog.take();
        }
    }

    impl<S> Stream for TrackedUpload<S>
    where
        S: Stream<Item = Result<BytesMut, io::Error>> + Unpin,
    {
        type Item = Result<BytesMut, io::Error>;

        /// Poll our file stream and report on how far it has gotten
        ///
        /// # Arguments
        ///
        /// * `cx` - The context to poll our file stream with
        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            // our stream is Unpin so we can get at it without a pinning projection
            let this = self.get_mut();
            // the first poll is when hyper actually started reading this file
            if this.watchdog.is_none() && !this.finished {
                // stamp when we started and log that this file is on the wire
                this.progress.start();
                // start reporting on this file so a stall doesn't just go silent
                this.watchdog = StallWatchdog::spawn(&this.progress);
            }
            match Pin::new(&mut this.inner).poll_next(cx) {
                // we read another chunk of this file
                Poll::Ready(Some(Ok(chunk))) => {
                    // track how much of this file we have handed over
                    this.progress.record_chunk(chunk.len());
                    Poll::Ready(Some(Ok(chunk)))
                }
                // we failed to read this file off of disk
                Poll::Ready(Some(Err(error))) => {
                    // log how far we got before this file failed
                    this.progress.fail(&error);
                    // this file is not going to make any more progress
                    this.finish();
                    Poll::Ready(Some(Err(error)))
                }
                // this file has been fully handed to hyper
                Poll::Ready(None) => {
                    // log that this file streamed and check that it was the size we promised
                    this.progress.finish();
                    // this file is done so stop reporting on it
                    this.finish();
                    Poll::Ready(None)
                }
                // this file has more to give but isn't ready yet
                Poll::Pending => Poll::Pending,
            }
        }
    }

    impl<S> Drop for TrackedUpload<S> {
        /// Warn if this file was dropped before it finished streaming
        fn drop(&mut self) {
            // a file dropped mid stream is a file some upload timed out or reset on
            if !self.finished {
                self.progress.abandon();
            }
        }
    }

    /// Wrap a file stream so it reports on its own progress as it is uploaded
    ///
    /// # Arguments
    ///
    /// * `stream` - The stream of the file being uploaded
    /// * `file_name` - The name this file is being uploaded under
    /// * `total` - The total number of bytes we expect to stream
    pub fn track<S>(stream: S, file_name: &str, total: u64) -> TrackedUpload<S> {
        TrackedUpload::new(stream, file_name, total)
    }

    #[cfg(test)]
    mod tests {
        use super::{TrackedUpload, track};
        use bytes::BytesMut;
        use futures::StreamExt;
        use std::io;
        use std::sync::Arc;

        /// The tracker sits on every by path upload in the client, so make sure it hands back
        /// exactly the chunks it was given and counts every byte of them.
        #[tokio::test]
        async fn forwards_chunks_untouched() {
            // build a few chunks to stream through our tracker
            let chunks = vec![
                BytesMut::from(&b"the "[..]),
                BytesMut::from(&b"corn "[..]),
                BytesMut::from(&b"peeps"[..]),
            ];
            // total up how many bytes we expect to stream
            let total = chunks.iter().map(|chunk| chunk.len() as u64).sum::<u64>();
            // build a stream over our chunks
            let inner = futures::stream::iter(
                chunks
                    .clone()
                    .into_iter()
                    .map(Ok::<BytesMut, io::Error>)
                    .collect::<Vec<Result<BytesMut, io::Error>>>(),
            );
            // wrap our stream in a tracker
            let tracked: TrackedUpload<_> = track(inner, "corn/peeps.txt", total);
            // hold onto the progress so we can check it after our stream is consumed
            let progress = Arc::clone(&tracked.progress);
            // drain our tracked stream
            let streamed = tracked.map(Result::unwrap).collect::<Vec<BytesMut>>().await;
            // our chunks should have come back byte for byte
            assert_eq!(streamed, chunks);
            // and every one of their bytes should have been counted
            assert_eq!(progress.state().bytes_sent, total);
            assert_eq!(progress.state().chunks, chunks.len());
        }
    }
}

#[cfg(feature = "client-trace")]
pub(crate) use tracked::RequestWatch;
#[cfg(feature = "client-trace")]
pub use tracked::{TrackedUpload, track};

/// A no op stand in for a request's progress reports when tracing is not compiled in
///
/// This mirrors the tracked version so the client can bracket its requests unconditionally
/// instead of littering every send with a cfg.
#[cfg(not(feature = "client-trace"))]
pub(crate) struct RequestWatch;

#[cfg(not(feature = "client-trace"))]
impl RequestWatch {
    /// Pretend to start reporting on a request we are about to send
    ///
    /// # Arguments
    ///
    /// * `what` - A short description of the request being sent
    pub(crate) fn start(what: &str) -> Self {
        // we have nowhere to report progress to so there is nothing to track
        let _ = what;
        RequestWatch
    }

    /// Pretend to log that this request came back
    ///
    /// # Arguments
    ///
    /// * `success` - Whether this request came back successfully
    pub(crate) fn finish(self, success: bool) {
        // we have nowhere to report progress to so there is nothing to log
        let _ = success;
    }
}

/// Pass a file stream through untouched when tracing is not compiled in
///
/// This mirrors the tracked version's signature so [`multipart_file!`](crate::multipart_file) can
/// call it unconditionally. Both are fed straight into [`reqwest::Body::wrap_stream`], which is
/// generic over its stream, so the differing return types never have to be named.
///
/// # Arguments
///
/// * `stream` - The stream of the file being uploaded
/// * `file_name` - The name this file is being uploaded under
/// * `total` - The total number of bytes we expect to stream
#[cfg(not(feature = "client-trace"))]
pub fn track<S>(stream: S, file_name: &str, total: u64) -> S {
    // we have nowhere to report progress to so just hand the stream back untouched
    let _ = (file_name, total);
    stream
}
