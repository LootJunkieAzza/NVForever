//! Nonblocking admission and the ordered writer (docs/design/frame-telemetry.md §4).

use super::records::{AttemptCounts, Diagnostic, Frame, Header, Marker, Payload, Phase, Record, StopReason, Trailer};
use super::stats::PhaseStats;
use super::{Config, DIAGNOSTIC_SLOTS, FRAME_SLOTS, MARKERS_PER_FRAME, MAX_BYTES, MAX_FRAMES, TRAILER_RESERVE};
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

#[derive(Clone, Copy)]
struct Stop {
    reason: StopReason,
    t_ns: u64,
}

#[derive(Default)]
struct Shared {
    stop: OnceLock<Stop>,
    byte_cut: OnceLock<Stop>,
    writer_error: OnceLock<Stop>,
    timeout: OnceLock<Stop>,
    deadline: OnceLock<Instant>,
    frozen: AtomicBool,
    closing: AtomicBool,
    active: AtomicBool,
    next_seq: AtomicU64,
    frames_attempted: AtomicU64,
    diagnostics_attempted: AtomicU64,
    markers_attempted: AtomicU64,
    last_t_ns: AtomicU64,
}

impl Shared {
    fn stop(&self, reason: StopReason, t_ns: u64) {
        let stop = Stop {
            reason,
            t_ns: t_ns.max(self.last_t_ns.load(Ordering::Acquire)),
        };
        match reason {
            StopReason::WriterError => {
                let _ = self.writer_error.set(stop);
            }
            StopReason::ShutdownTimeout => {
                // Design note: exceeding the drain grace truncates an earlier normal or
                // limit stop; an I/O error still takes precedence over the timeout.
                let _ = self.timeout.set(stop);
            }
            _ => {
                let _ = self.stop.set(stop);
            }
        }
        self.frozen.store(true, Ordering::SeqCst);
    }

    fn stopped(&self) -> Option<Stop> {
        self.writer_error
            .get()
            .or_else(|| self.timeout.get())
            .or_else(|| self.byte_cut.get())
            .or_else(|| self.stop.get())
            .copied()
    }

    fn cut_at(&self, t_ns: u64) {
        // Queued data can exceed the budget after producers have already stopped.
        // The writer shortens that interval, while errors and timeouts still override it.
        let _ = self.byte_cut.set(Stop {
            reason: StopReason::ByteLimit,
            t_ns,
        });
        self.frozen.store(true, Ordering::SeqCst);
    }
}

/// One producer owns sequence assignment; engine and render data enter through it.
/// Only `finish`/Drop waits for the writer. Queue admission uses `try_send` exclusively.
pub struct Capture {
    config: Config,
    epoch: Instant,
    shared: Arc<Shared>,
    frames: Option<SyncSender<Record>>,
    diagnostics: Option<SyncSender<Record>>,
    markers: Vec<Marker>,
    pending_marker_attempts: u64,
    framed_marker_attempts: u64,
    next_seq: u64,
    limits: Limits,
    // Only shutdown accesses the receiver; this also keeps Capture Send + Sync.
    done: Mutex<Receiver<WriterReport>>,
    writer: Option<JoinHandle<()>>,
}

#[derive(Clone, Copy)]
struct Limits {
    frames: u64,
    bytes: u64,
    frame_slots: usize,
    diagnostic_slots: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            frames: MAX_FRAMES,
            bytes: MAX_BYTES,
            frame_slots: FRAME_SLOTS,
            diagnostic_slots: DIAGNOSTIC_SLOTS,
        }
    }
}

#[derive(Debug)]
pub struct WriterReport {
    /// None if the trailer could not be wholly written and flushed.
    pub trailer: Option<Trailer>,
    pub error: Option<String>,
    pub bytes_written: u64,
}

#[derive(Debug)]
pub enum Shutdown {
    Finished(Box<WriterReport>),
    TimedOut,
}

struct Production<'a>(&'a Shared);

impl<'a> Production<'a> {
    fn begin(shared: &'a Shared) -> Self {
        // This gate and freeze use one atomic order: either the writer waits for the
        // active producer, or that producer observes the freeze before changing counts.
        shared.active.store(true, Ordering::SeqCst);
        Self(shared)
    }
}

impl Drop for Production<'_> {
    fn drop(&mut self) {
        self.0.active.store(false, Ordering::SeqCst);
    }
}

impl Capture {
    /// The caller must guard this final path before calling. Never replaces an existing file.
    pub fn create(path: &Path, header: Header, epoch: Instant) -> io::Result<Self> {
        // Validate before creating a file, including the reserved trailer capacity.
        let header_line = header.line()?;
        let sink = OpenOptions::new().write(true).create_new(true).open(path)?;
        Self::start(sink, header.config, header_line, epoch, Limits::default())
    }

    fn start<W: Write + Send + 'static>(sink: W, config: Config, header_line: Vec<u8>, epoch: Instant, limits: Limits) -> io::Result<Self> {
        if header_line.len() as u64 + TRAILER_RESERVE > limits.bytes {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "header leaves no trailer reserve"));
        }
        let shared = Arc::new(Shared::default());
        shared.next_seq.store(1, Ordering::Relaxed);
        let (frames_tx, frames_rx) = mpsc::sync_channel(limits.frame_slots);
        let (diagnostics_tx, diagnostics_rx) = mpsc::sync_channel(limits.diagnostic_slots);
        let (done_tx, done) = mpsc::sync_channel(1);
        let writer_shared = Arc::clone(&shared);
        let writer = thread::Builder::new().name("frame-telemetry".to_owned()).spawn(move || {
            let report = write_capture(sink, &header_line, frames_rx, diagnostics_rx, &writer_shared, epoch, limits.bytes);
            let _ = done_tx.send(report);
        })?;
        Ok(Self {
            config,
            epoch,
            shared,
            frames: Some(frames_tx),
            diagnostics: Some(diagnostics_tx),
            markers: Vec::with_capacity(MARKERS_PER_FRAME),
            pending_marker_attempts: 0,
            framed_marker_attempts: 0,
            next_seq: 1,
            limits,
            done: Mutex::new(done),
            writer: Some(writer),
        })
    }

    pub fn elapsed_ns(&self) -> u64 {
        elapsed(self.epoch)
    }

    pub fn stop_reason(&self) -> Option<StopReason> {
        self.shared.stopped().map(|s| s.reason)
    }

    fn check_limits(&self, t_ns: u64) {
        if t_ns >= self.config.duration.as_nanos() as u64 {
            self.shared.stop(StopReason::DurationLimit, t_ns);
        } else if self.shared.frames_attempted.load(Ordering::Acquire) >= self.limits.frames {
            self.shared.stop(StopReason::FrameLimit, t_ns);
        }
    }

    /// Check duration before another stop request; a previously selected stop still wins.
    pub fn stop_at(&self, reason: StopReason, t_ns: u64) {
        self.check_limits(t_ns);
        self.shared.stop(reason, t_ns);
    }

    pub fn marker(&mut self, marker: Marker) -> bool {
        let _active = Production::begin(&self.shared);
        self.check_limits(marker.t_ns);
        if self.shared.frozen.load(Ordering::SeqCst) {
            return false;
        }
        self.shared.markers_attempted.fetch_add(1, Ordering::Relaxed);
        self.pending_marker_attempts += 1;
        if self.markers.len() == MARKERS_PER_FRAME {
            return false;
        }
        self.markers.push(marker);
        true
    }

    /// First attempted frame has null duration. A later zero is a real sample.
    pub fn frame_at(&mut self, t_ns: u64, main_frame: u32, duration_ns: u64, phase: Phase) -> bool {
        if self.shared.frozen.load(Ordering::SeqCst) {
            return false;
        }
        let duration_ns = (self.shared.frames_attempted.load(Ordering::Acquire) > 0).then_some(duration_ns);
        let markers = if self.markers.is_empty() {
            Vec::new()
        } else {
            std::mem::replace(&mut self.markers, Vec::with_capacity(MARKERS_PER_FRAME))
        };
        let marker_attempts = std::mem::take(&mut self.pending_marker_attempts);
        self.submit(
            t_ns,
            Payload::Frame(Frame {
                main_frame,
                duration_ns,
                phase,
                markers,
            }),
            marker_attempts,
            false,
        )
    }

    pub fn diagnostic_at(&mut self, t_ns: u64, diagnostic: Diagnostic) -> bool {
        if !self.config.gpu {
            return false;
        }
        self.submit(t_ns, Payload::Diagnostic(diagnostic), 0, false)
    }

    fn submit(&mut self, t_ns: u64, payload: Payload, marker_attempts: u64, terminal: bool) -> bool {
        let _active = Production::begin(&self.shared);
        let t_ns = t_ns.max(self.shared.last_t_ns.load(Ordering::Acquire));
        if !terminal {
            self.check_limits(t_ns);
            if self.shared.frozen.load(Ordering::SeqCst) {
                return false;
            }
        } else if self.shared.writer_error.get().is_some() || self.shared.timeout.get().is_some() || self.shared.byte_cut.get().is_some() {
            return false;
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        let frame = matches!(payload, Payload::Frame(_));
        if frame {
            self.shared.frames_attempted.fetch_add(1, Ordering::Relaxed);
            self.framed_marker_attempts += marker_attempts;
        } else {
            self.shared.diagnostics_attempted.fetch_add(1, Ordering::Relaxed);
        }
        let record = Record {
            seq,
            t_ns,
            payload,
            attempts: AttemptCounts {
                frames: self.shared.frames_attempted.load(Ordering::Relaxed),
                diagnostics: self.shared.diagnostics_attempted.load(Ordering::Relaxed),
                markers: self.framed_marker_attempts,
            },
        };
        self.shared.last_t_ns.store(t_ns, Ordering::Relaxed);
        let queue = if frame { &self.frames } else { &self.diagnostics };
        let accepted = record.payload.within_bounds() && queue.as_ref().is_some_and(|queue| queue.try_send(record).is_ok());
        // Publish only after try_send/drop completes. The writer may then skip seq gaps.
        self.shared.next_seq.store(self.next_seq, Ordering::Release);
        if !terminal {
            self.check_limits(t_ns);
        }
        accepted
    }

    /// A terminal GPU sentinel bypasses duration/freeze checks, but still needs byte/queue capacity.
    /// Stage 2 supplies it only when no GPU sample was observed, after checking capability.
    pub fn finish_at(mut self, t_ns: u64, reason: StopReason, sentinel: Option<Diagnostic>) -> Shutdown {
        self.shutdown(t_ns, reason, sentinel, SHUTDOWN_GRACE)
    }

    fn shutdown(&mut self, t_ns: u64, reason: StopReason, sentinel: Option<Diagnostic>, grace: Duration) -> Shutdown {
        let deadline = Instant::now() + grace;
        let _ = self.shared.deadline.set(deadline);
        self.stop_at(reason, t_ns);
        if self.config.gpu {
            if let Some(sentinel) = sentinel {
                let stop_t = self.shared.stopped().map_or(t_ns, |s| s.t_ns);
                self.submit(stop_t, Payload::Diagnostic(sentinel), 0, true);
            }
        }
        self.frames.take();
        self.diagnostics.take();
        self.shared.closing.store(true, Ordering::Release);
        match self
            .done
            .get_mut()
            .expect("completion receiver is never locked")
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            Ok(report) => {
                if let Some(writer) = self.writer.take() {
                    let _ = writer.join();
                }
                Shutdown::Finished(Box::new(report))
            }
            Err(_) => {
                self.shared.stop(StopReason::ShutdownTimeout, t_ns);
                self.writer.take(); // A blocked OS write cannot be cancelled; detach within the grace.
                Shutdown::TimedOut
            }
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        if self.writer.is_some() {
            let _ = self.shutdown(self.elapsed_ns(), StopReason::NormalExit, None, SHUTDOWN_GRACE);
        }
    }
}

fn elapsed(epoch: Instant) -> u64 {
    epoch.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}

struct Output<W> {
    sink: W,
    bytes: u64,
    max_bytes: u64,
    partial_line: bool,
    error: Option<String>,
}

impl<W: Write> Output<W> {
    fn line(&mut self, line: &[u8], reserve: u64) -> io::Result<bool> {
        if self.bytes + line.len() as u64 > self.max_bytes - reserve {
            return Ok(false);
        }
        self.partial_line = false;
        let mut remaining = line;
        while !remaining.is_empty() {
            match self.sink.write(remaining) {
                Ok(0) => return Err(io::Error::new(io::ErrorKind::WriteZero, "telemetry write returned zero")),
                Ok(n) => {
                    self.bytes += n as u64;
                    remaining = &remaining[n..];
                    self.partial_line = !remaining.is_empty();
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(true)
    }

    fn fail(&mut self, error: io::Error, shared: &Shared, epoch: Instant) {
        shared.stop(StopReason::WriterError, elapsed(epoch));
        if self.error.is_none() {
            let message = error.to_string();
            eprintln!("frame telemetry: {message}");
            self.error = Some(message);
        }
    }
}

fn write_capture<W: Write>(
    sink: W,
    header: &[u8],
    frames: Receiver<Record>,
    diagnostics: Receiver<Record>,
    shared: &Shared,
    epoch: Instant,
    max_bytes: u64,
) -> WriterReport {
    let mut output = Output {
        sink,
        bytes: 0,
        max_bytes,
        partial_line: false,
        error: None,
    };
    let mut phases: [PhaseStats; 3] = Default::default();
    let (mut frames_written, mut diagnostics_written, mut markers_written, mut gpu_written) = (0, 0, 0, 0);
    let mut last_written = (0, 0, AttemptCounts::default());
    let mut cut = None;
    if let Err(e) = output.line(header, TRAILER_RESERVE) {
        output.fail(e, shared, epoch);
    }
    let (mut frame, mut diagnostic): (Option<Record>, Option<Record>) = (None, None);
    loop {
        if shared.deadline.get().is_some_and(|deadline| Instant::now() >= *deadline) {
            shared.stop(StopReason::ShutdownTimeout, elapsed(epoch));
        }
        if output.error.is_some() || shared.timeout.get().is_some() {
            if !shared.active.load(Ordering::SeqCst) {
                break;
            }
            thread::yield_now();
            continue;
        }
        // Snapshot BEFORE polling either queue: every smaller seq has already been sent
        // or dropped, even if a producer is between assignment and send of a later seq.
        let watermark = shared.next_seq.load(Ordering::Acquire);
        if frame.is_none() {
            frame = frames.try_recv().ok();
        }
        if diagnostic.is_none() {
            diagnostic = diagnostics.try_recv().ok();
        }
        let next = match (&frame, &diagnostic) {
            (Some(f), Some(d)) if f.seq < d.seq => &mut frame,
            (Some(_), Some(_)) => &mut diagnostic,
            (Some(_), None) => &mut frame,
            (None, Some(_)) => &mut diagnostic,
            (None, None) => {
                if shared.closing.load(Ordering::Acquire) && !shared.active.load(Ordering::SeqCst) {
                    // Re-poll after observing close, so a send racing the earlier polls
                    // cannot be mistaken for a completely drained pair of queues.
                    frame = frames.try_recv().ok();
                    diagnostic = diagnostics.try_recv().ok();
                    if frame.is_none() && diagnostic.is_none() {
                        break;
                    }
                    continue;
                }
                thread::park_timeout(Duration::from_millis(1));
                continue;
            }
        };
        if next.as_ref().is_some_and(|record| record.seq >= watermark) {
            thread::yield_now();
            continue;
        }
        let record = next.take().expect("selected a populated queue head");
        // Bounds are checked cheaply by producers. A serialization size failure is
        // still an exporter drop, rather than a file I/O error that stops the writer.
        let Ok(line) = record.line() else {
            continue;
        };
        let result = output.line(&line, TRAILER_RESERVE);
        match result {
            Ok(true) => {
                last_written = (record.seq, record.t_ns, record.attempts);
                match record.payload {
                    Payload::Frame(f) => {
                        frames_written += 1;
                        markers_written += f.markers.len() as u64;
                        phases[f.phase.index()].record(f.duration_ns);
                    }
                    Payload::Diagnostic(d) => {
                        diagnostics_written += 1;
                        gpu_written += u64::from(d.available_gpu());
                    }
                }
            }
            Ok(false) => {
                cut = Some(last_written);
                shared.cut_at(last_written.1);
                break;
            }
            Err(e) => output.fail(e, shared, epoch),
        }
    }
    // A byte cut may race a producer already in try_send. Freeze that producer before
    // taking the trailer seq; its later attempts are outside the saved interval.
    while shared.active.load(Ordering::SeqCst) {
        thread::yield_now();
    }
    if output.error.is_none() {
        if let Err(e) = output.sink.flush() {
            output.fail(e, shared, epoch);
        }
    }
    if shared.deadline.get().is_some_and(|deadline| Instant::now() >= *deadline) {
        shared.stop(StopReason::ShutdownTimeout, elapsed(epoch));
    }
    let make_trailer = |phases: &mut [PhaseStats; 3]| {
        let stop = shared.stopped().unwrap_or(Stop {
            reason: StopReason::NormalExit,
            t_ns: elapsed(epoch),
        });
        let attempts = cut.map_or_else(
            || AttemptCounts {
                frames: shared.frames_attempted.load(Ordering::Acquire),
                diagnostics: shared.diagnostics_attempted.load(Ordering::Acquire),
                markers: shared.markers_attempted.load(Ordering::Acquire),
            },
            |(_, _, attempts)| attempts,
        );
        Trailer {
            seq: shared.next_seq.load(Ordering::Acquire),
            t_ns: stop.t_ns,
            stop_reason: stop.reason,
            cut_seq: cut.map(|(seq, _, _)| seq),
            frames_attempted: attempts.frames,
            frames_written,
            dropped_frames: attempts.frames - frames_written,
            dropped_diagnostics: attempts.diagnostics - diagnostics_written,
            dropped_markers: attempts.markers - markers_written,
            gpu_observations_written: gpu_written,
            phases: phases.each_mut().map(PhaseStats::summary),
        }
    };
    let mut trailer = make_trailer(&mut phases);
    let recoverable = if output.error.is_some() {
        // A successful flush is the only useful writability check after a write error.
        // A partial record remains invalid even if the following trailer can be written.
        output.sink.flush().is_ok() && (!output.partial_line || output.line(b"\n", 0).unwrap_or(false))
    } else {
        true
    };
    let mut trailer_written = false;
    if recoverable {
        match trailer.line().and_then(|line| output.line(&line, 0)) {
            Ok(written) => trailer_written = written,
            Err(e) => output.fail(e, shared, epoch),
        }
    }
    if let Err(e) = output.sink.flush() {
        output.fail(e, shared, epoch);
        trailer_written = false;
    }
    // If the trailer/flush itself failed, never return an earlier 'complete' trailer.
    if shared.writer_error.get().is_some() && trailer.stop_reason != StopReason::WriterError {
        trailer = make_trailer(&mut phases);
        trailer_written = false;
    }
    WriterReport {
        trailer: trailer_written.then_some(trailer),
        error: output.error,
        bytes_written: output.bytes,
    }
}

#[cfg(test)]
pub(super) mod test_support {
    use super::*;

    pub fn start<W: Write + Send + 'static>(
        sink: W,
        header: Header,
        frames: u64,
        bytes: u64,
        frame_slots: usize,
        diagnostic_slots: usize,
    ) -> io::Result<Capture> {
        let line = header.line()?;
        Capture::start(
            sink,
            header.config,
            line,
            Instant::now(),
            Limits {
                frames,
                bytes,
                frame_slots,
                diagnostic_slots,
            },
        )
    }

    pub fn shutdown(capture: &mut Capture, grace: Duration) -> Shutdown {
        capture.shutdown(1, StopReason::NormalExit, None, grace)
    }

    pub fn write_records<W: Write>(sink: W, header: Header, records: Vec<Record>) -> WriterReport {
        let shared = Shared::default();
        let (frames_tx, frames) = mpsc::sync_channel(records.len());
        let (diagnostics_tx, diagnostics) = mpsc::sync_channel(records.len());
        let last = records.last().unwrap();
        shared.next_seq.store(last.seq + 1, Ordering::Release);
        shared.frames_attempted.store(last.attempts.frames, Ordering::Relaxed);
        shared.diagnostics_attempted.store(last.attempts.diagnostics, Ordering::Relaxed);
        shared.markers_attempted.store(last.attempts.markers, Ordering::Relaxed);
        shared.stop(StopReason::NormalExit, last.t_ns);
        shared.closing.store(true, Ordering::Release);
        for record in records {
            let queue = if matches!(record.payload, Payload::Frame(_)) {
                &frames_tx
            } else {
                &diagnostics_tx
            };
            queue.try_send(record).unwrap();
        }
        write_capture(sink, &header.line().unwrap(), frames, diagnostics, &shared, Instant::now(), MAX_BYTES)
    }
}
