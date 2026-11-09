//! Frame generation's presenter (2026-11-08): a thread of its own that takes the window's
//! images, copies the generated and the real frame into them and presents them, the second
//! present half a real frame after the first. The render thread hands each frame over and goes
//! on with the next (frame_gen.rs `hand_to_presenter`).
//!
//! Why: presented one after the other from the render thread, the real frame's present waited
//! inside the driver until the generated frame was shown, about one refresh, and the render
//! thread recorded nothing meanwhile. A frame's own work plus that wait came to more than two
//! refreshes once the work passed about 7 ms, and the real frame rate fell from 80 to 50-58 at
//! 160 Hz [observed 2026-11-08: the generated frame's present 0.13-0.21 ms, the real one's
//! 4.4-6.2 ms; issued 6 ms after the first, it returned in 0.4-0.75 ms; h9x000/NVForever#1,
//! docs/design/frame-generation.md "Presentation and pacing"].
//!
//! Pacing follows the design of AMD's `FrameInterpolationSwapchainVK` (FidelityFX SDK 1.1.4,
//! MIT; read as reference, no code taken): wait for the frame's GPU work, then space the
//! presents by half the mean real frame time over the last 10 frames, less 0.1 x its variance
//! and a 0.1 ms margin, the average started again after a gap of more than 100 ms; the wait
//! sleeps, then spins its last millisecond. The spacing is never less than one refresh, and the
//! real frame rate is capped slightly below half the refresh rate (`frame_gen::real_frame_cap`),
//! as AMD advises, so two presents never fall within one refresh. A frame that reaches the
//! presenter when the next is already waiting is shown without its generated frame, so the
//! presenter catches up at once instead of keeping a frame of delay.
//!
//! wgpu 30 lets one acquired image exist per surface and holds a device-wide lock across each
//! present (`command_indices`, written), which stops submits from every thread while a present
//! blocks. So only this thread takes and presents the window's images while it runs
//! (bevy_render `DeferredSwapchain`), and its presents are timed to fall away from the render
//! thread's submit: the real frame's goes out about 1 ms after the next frame is handed over
//! [observed 2026-11-08, 4K at 160 Hz, 78 real fps: each present held 1-1.6 ms in the driver,
//! the render thread kept the cap].
//!
//! This thread never configures the surface: wgpu-core 30's `configure_surface` waits for the
//! GPU and fails with a validation error when another thread submits meanwhile
//! [documented: `wgpu-core-30.0.1/src/device/resource.rs` 5338-5351], and the render thread
//! keeps submitting. An outdated surface's images are skipped and the surface is marked for
//! the render thread, which reconfigures it before its next frame (bevy_render
//! `SurfaceGuard::acquire`, `create_surfaces`).
//!
//! Measured against presenting from the render thread (h9x000/NVForever#1, 4K, the owner's
//! settings): 77.7-78.0 real fps at the cap of 78 with a frame of about 8 ms and with 4 ms more
//! (55.6 before), the 95th percentile between presents 7.0 ms (13.5 before); the generated
//! frame is presented when the GPU has done the frame (4.4-4.8 ms after it is handed over,
//! polled about every 0.5 ms), as the driver showed it before.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bevy::render::view::{SharedSurface, STAND_INS};

use crate::frame_gen::Blit;

/// The presenter of one window.
pub(crate) struct Presenter {
    jobs: Option<mpsc::Sender<Job>>,
    thread: Option<JoinHandle<()>>,
    shared: Arc<Shared>,
    sent: u64,
}

/// One real frame's images: the generated frame then the real one, or the only one.
struct Job {
    index: u64,
    frames: Vec<wgpu::TextureView>,
    /// When the render thread handed it over, and how long it waited for the presenter first.
    sent: Instant,
    held: Duration,
    /// One refresh of the window's monitor (zero when not known): the least spacing.
    refresh: Duration,
}

pub(crate) struct Shared {
    /// The last frame whose copies are all submitted.
    copied: Mutex<u64>,
    copied_changed: Condvar,
    /// Spent waiting for the presents' times, and frames that could not be presented (an
    /// occluded or outdated window), since last read.
    pub(crate) paced_nanos: AtomicU64,
    pub(crate) skipped: AtomicU64,
    /// Generated frames left out to catch up (the presenter a whole frame behind), since last
    /// read.
    pub(crate) dropped: AtomicU64,
    /// From each frame's handing over to its first present (the GPU wait, the pacing and any
    /// frames ahead of it: what the presenter adds to the latency), since last read.
    pub(crate) queued_nanos: AtomicU64,
    /// Of that, waiting for the GPU, and the polls it took (each up to about 0.2 ms after the
    /// GPU was done), since last read.
    pub(crate) gpu_wait_nanos: AtomicU64,
    pub(crate) gpu_polls: AtomicU64,
    /// When each present was issued (frame generation's log line).
    presents: Arc<Mutex<Vec<Instant>>>,
    /// `frame_gen::GeneratedFrames`: counted when generated, taken back when left out.
    generated_shown: Arc<AtomicU64>,
}

impl Shared {
    fn copied(&self, index: u64) {
        let mut copied = self.copied.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if *copied < index {
            *copied = index;
        }
        self.copied_changed.notify_all();
    }
}

impl Presenter {
    pub(crate) fn start(surface: SharedSurface, device: wgpu::Device, queue: wgpu::Queue, presents: Arc<Mutex<Vec<Instant>>>, generated_shown: Arc<AtomicU64>) -> Self {
        let shared = Arc::new(Shared {
            copied: Mutex::new(0),
            copied_changed: Condvar::new(),
            paced_nanos: AtomicU64::new(0),
            skipped: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            queued_nanos: AtomicU64::new(0),
            gpu_wait_nanos: AtomicU64::new(0),
            gpu_polls: AtomicU64::new(0),
            presents,
            generated_shown,
        });
        let (jobs, received) = mpsc::channel();
        let thread_shared = shared.clone();
        let thread = std::thread::Builder::new()
            .name("nvf-frame-gen-present".into())
            .spawn(move || run(&received, &surface, &device, &queue, &thread_shared))
            .map_err(|e| bevy::log::error!("frame generation: no presenter thread: {e}"))
            .ok();
        bevy::log::info!("frame generation: presenting on a thread of its own, paced half a real frame apart");
        Self { jobs: thread.is_some().then_some(jobs), thread, shared, sent: 0 }
    }

    pub(crate) fn shared(&self) -> &Shared {
        &self.shared
    }

    /// Hand over this frame's images (each a view of a texture it may read until the frame
    /// after next), with one refresh of the monitor. Waits while the presenter is more than
    /// `STAND_INS - 1` frames behind, so nothing it has still to copy is drawn over.
    pub(crate) fn send(&mut self, frames: Vec<wgpu::TextureView>, refresh: Duration) {
        let Some(jobs) = &self.jobs else { return };
        self.sent += 1;
        let needed = self.sent.saturating_sub(STAND_INS as u64 - 1);
        let started = Instant::now();
        let deadline = started + Duration::from_millis(250);
        let mut copied = self.shared.copied.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        while *copied < needed {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else { break };
            copied = self.shared.copied_changed.wait_timeout(copied, left).unwrap_or_else(std::sync::PoisonError::into_inner).0;
        }
        drop(copied);
        let sent = Instant::now();
        let _ = jobs.send(Job { index: self.sent, frames, sent, held: sent - started, refresh });
    }
}

impl Drop for Presenter {
    /// The frames handed over are presented, then the thread ends, before the window is
    /// Bevy's again.
    fn drop(&mut self) {
        self.jobs.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(received: &mpsc::Receiver<Job>, surface: &SharedSurface, device: &wgpu::Device, queue: &wgpu::Queue, shared: &Shared) {
    let mut blit = Blit::default();
    let mut pacer = Pacer::default();
    let mut last_present: Option<Instant> = None;
    let mut next: Option<Job> = None;
    loop {
        let job = match next.take() {
            Some(job) => job,
            None => match received.recv() {
                Ok(job) => job,
                Err(_) => break,
            },
        };
        // A whole frame behind (the next frame is already here): show this frame's real image
        // only, a refresh after the last present, and leave its generated frame out. At the cap
        // two paced presents take nearly a whole real frame, so a backlog drained by about
        // 0.2 ms a frame and the frames waited here for up to another frame [observed
        // 2026-11-09, 4K at 160 Hz, 78 real fps: the first present 13-21 ms after the frame
        // was handed over in some 10 s periods, 7.4 in others].
        let behind = match received.try_recv() {
            Ok(newer) => {
                next = Some(newer);
                true
            }
            Err(_) => false,
        };
        pacer.add(job.sent, job.held);
        let frames = if behind && job.frames.len() == 2 {
            shared.dropped.fetch_add(1, Ordering::Relaxed);
            // Not shown after all: the frame-rate counter counts generated frames shown.
            shared.generated_shown.fetch_sub(1, Ordering::Relaxed);
            &job.frames[1..]
        } else {
            &job.frames[..]
        };
        // Never closer than a refresh: two presents within one queue up in the driver, and the
        // second waits there for the first to be shown [observed 2026-11-08: spaced 5.4-6.15 ms
        // at 160 Hz, each present took about 2 ms].
        let paired = frames.len() == 2;
        let spacing = if paired { pacer.spacing().max(job.refresh) } else { job.refresh };
        if paired {
            let (waited, polls) = wait_for_gpu(device, queue);
            shared.gpu_wait_nanos.fetch_add(waited.as_nanos() as u64, Ordering::Relaxed);
            shared.gpu_polls.fetch_add(polls, Ordering::Relaxed);
        }
        let guard = surface.lock();
        let (_, view_format) = guard.formats();
        let count = frames.len();
        for (i, source) in frames.iter().enumerate() {
            let Some(image) = guard.acquire() else {
                shared.skipped.fetch_add(1, Ordering::Relaxed);
                continue;
            };
            let target = image.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(view_format), ..Default::default() });
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame_gen_present_copy") });
            blit.draw(device, &mut encoder, source, &target, view_format, "frame_gen_present_copy");
            queue.submit([encoder.finish()]);
            if i + 1 == count {
                shared.copied(job.index);
            }
            // Paired frames are spaced; a frame shown alone while catching up is held a refresh
            // after the last present; frames without a generated one (menus, cuts) go at once.
            if let (true, Some(last)) = (paired || behind, last_present) {
                let waited = wait_until(last + spacing);
                shared.paced_nanos.fetch_add(waited.as_nanos() as u64, Ordering::Relaxed);
            }
            // Spaced from when the last present was issued, not from when it returned: from its
            // return, the time each present takes in the driver was added to every spacing, and
            // the presenter fell behind the real frame cap [observed 2026-11-08: 0.26-0.47 ms
            // per present, presents 6.5-6.7 ms apart where 6.3 was due, 74-77 real fps of 78].
            let issued = Instant::now();
            last_present = Some(issued);
            if i == 0 {
                shared.queued_nanos.fetch_add(issued.saturating_duration_since(job.sent).as_nanos() as u64, Ordering::Relaxed);
            }
            guard.present(queue, image);
            shared.presents.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(issued);
        }
        shared.copied(job.index);
    }
}

/// Until the GPU has done the work submitted so far (the frame and its generated frame), so the
/// presents are timed from when the frame can be shown (at most 100 ms); how long it waited and
/// how often it polled. Polled, not waited on: wgpu's blocking wait holds a device lock
/// (`snatchable_lock`, read) for as long as it waits, and a thread that asks for it to write
/// (a resource destroyed) would stop every other thread's submits behind it until then.
fn wait_for_gpu(device: &wgpu::Device, queue: &wgpu::Queue) -> (Duration, u64) {
    let done = Arc::new(AtomicBool::new(false));
    let flag = done.clone();
    queue.on_submitted_work_done(move || flag.store(true, Ordering::Release));
    let started = Instant::now();
    let mut polls = 0;
    while !done.load(Ordering::Acquire) && started.elapsed() < Duration::from_millis(100) {
        let _ = device.poll(wgpu::PollType::Poll);
        polls += 1;
        if !done.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_micros(200));
        }
    }
    (started.elapsed(), polls)
}

/// Sleep until a millisecond before `due`, then spin; how long it waited.
fn wait_until(due: Instant) -> Duration {
    let started = Instant::now();
    if let Some(sleep) = due.checked_duration_since(started).and_then(|d| d.checked_sub(Duration::from_millis(1))) {
        std::thread::sleep(sleep);
    }
    while Instant::now() < due {
        std::hint::spin_loop();
    }
    started.elapsed()
}

/// The real frames' intervals, for the presents' spacing: between the times the render thread
/// handed them over, less what it waited for the presenter before each (so a presenter that
/// falls behind does not slow its own pacing to match, which locked the frame rate low: at a
/// start, slow frames gave a long spacing, the long spacing held the render thread back, and
/// the held frames kept the spacing long [observed 2026-11-08: 19, 36, 43 real fps in the first
/// 30 s at 1280x720, where 80 was due]).
#[derive(Default)]
struct Pacer {
    last: Option<Instant>,
    intervals: VecDeque<f64>,
}

/// Intervals the mean is taken over.
const PACER_FRAMES: usize = 10;

impl Pacer {
    fn add(&mut self, now: Instant, held: Duration) {
        if let Some(last) = self.last {
            let interval = now.duration_since(last).saturating_sub(held).as_secs_f64();
            if interval > 0.1 {
                self.intervals.clear();
            } else {
                if self.intervals.len() == PACER_FRAMES {
                    self.intervals.pop_front();
                }
                self.intervals.push_back(interval);
            }
        }
        self.last = Some(now);
    }

    /// Half the mean interval, less 0.1 x its variance and 0.1 ms, never below 0.
    fn spacing(&self) -> Duration {
        spacing(self.intervals.iter().copied())
    }
}

fn spacing(intervals: impl Iterator<Item = f64> + Clone) -> Duration {
    let n = intervals.clone().count();
    if n == 0 {
        return Duration::ZERO;
    }
    let mean = intervals.clone().sum::<f64>() / n as f64;
    let variance = intervals.map(|i| (i - mean) * (i - mean)).sum::<f64>() / n as f64;
    Duration::from_secs_f64((mean * 0.5 - variance * 0.1 - 0.0001).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presents_are_spaced_half_a_real_frame_less_a_margin() {
        let frames = [0.0125; 10];
        let s = spacing(frames.iter().copied()).as_secs_f64();
        assert!((s - (0.00625 - 0.0001)).abs() < 1e-9, "{s}");
        assert_eq!(spacing([].iter().copied()), Duration::ZERO);
        // Uneven frames: the variance takes a little more off.
        let uneven = spacing([0.010, 0.015].iter().copied()).as_secs_f64();
        assert!(uneven < 0.00625 - 0.0001 && uneven > 0.006, "{uneven}");
    }

    #[test]
    fn a_long_gap_starts_the_average_again() {
        let mut p = Pacer::default();
        let t = Instant::now();
        p.add(t, Duration::ZERO);
        p.add(t + Duration::from_millis(12), Duration::ZERO);
        p.add(t + Duration::from_millis(500), Duration::ZERO);
        assert!(p.intervals.is_empty());
        p.add(t + Duration::from_millis(512), Duration::ZERO);
        assert_eq!(p.intervals.len(), 1);
    }

    #[test]
    fn time_held_back_by_the_presenter_does_not_count() {
        let mut p = Pacer::default();
        let t = Instant::now();
        p.add(t, Duration::ZERO);
        // 25 ms apart, 12 of them waiting for the presenter: the frame took 13.
        p.add(t + Duration::from_millis(25), Duration::from_millis(12));
        assert!((p.intervals[0] - 0.013).abs() < 1e-9, "{:?}", p.intervals);
    }
}
