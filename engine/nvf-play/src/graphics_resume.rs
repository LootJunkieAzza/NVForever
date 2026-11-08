//! Pause-session pipeline wait [design: pause.rs, nvforever/pipeline_gate.rs].
//! Rendering continues under the menu; simulation resumes only after fresh, quiet
//! render samples or the loading screen's real-time cap. Streaming is not gated here.

use crate::loading::{PIPELINE_CAP_SECS, SETTLE_FRAMES, SETTLE_SECS};
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Default)]
struct Sample {
    frame: u32,
    pending: u32,
    asked: u32,
}

#[derive(Debug)]
struct Wait {
    since: f64,
    last: Sample,
    quiet: Option<(f64, u32)>,
}

/// Renderer-independent bridge and pause-session latch [design: this module].
#[derive(Resource, Debug, Default)]
pub struct GraphicsResume {
    sample: Sample,
    changed: bool,
    wait: Option<Wait>,
}

#[derive(Debug, PartialEq)]
pub struct Completion {
    pub waited: f64,
    pub pending: u32,
    pub capped: bool,
}

impl GraphicsResume {
    /// Publish a completed render frame, including shadows and total requests.
    pub fn sample(&mut self, frame: u32, pending: u32, asked: u32) {
        self.sample = Sample { frame, pending, asked };
    }

    /// Latch an effective setting change. The settings caller limits this to a pause.
    pub fn changed(&mut self) {
        self.changed = true;
        if let Some(wait) = &mut self.wait {
            wait.last = self.sample;
            wait.quiet = None;
        }
    }

    pub fn pending(&self) -> bool {
        self.wait.is_some()
    }

    /// True means unchanged: release immediately. Repeated requests keep the first cap.
    pub fn request(&mut self, now: f64) -> bool {
        if !self.changed {
            return true;
        }
        self.wait.get_or_insert(Wait { since: now, last: self.sample, quiet: None });
        false
    }

    pub fn poll(&mut self, now: f64) -> Option<Completion> {
        let wait = self.wait.as_mut()?;
        let waited = now - wait.since;
        let capped = waited >= PIPELINE_CAP_SECS;
        let sample = self.sample;
        let mut ready = false;
        if sample.frame != wait.last.frame {
            // A request can arrive and finish between main-world samples. Count it as
            // activity even when the current pending count is zero [design].
            if sample.pending != 0 || sample.asked != wait.last.asked {
                wait.quiet = None;
            } else {
                let (since, frame) = *wait.quiet.get_or_insert((now, sample.frame));
                ready = now - since >= SETTLE_SECS && sample.frame.wrapping_sub(frame) >= SETTLE_FRAMES;
            }
            wait.last = sample;
        }
        if !ready && !capped {
            return None;
        }
        self.reset();
        Some(Completion { waited, pending: sample.pending, capped: capped && !ready })
    }

    /// A load, quit or menu closure ends this pause session, retaining the latest sample.
    pub fn reset(&mut self) {
        self.changed = false;
        self.wait = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_is_immediate_and_repeated_requests_keep_the_cap() {
        let mut resume = GraphicsResume::default();
        assert!(resume.request(1.0));
        resume.changed();
        assert!(!resume.request(2.0));
        assert!(!resume.request(10.0));
        assert!(resume.poll(16.9).is_none());
        assert_eq!(resume.poll(17.0), Some(Completion { waited: 15.0, pending: 0, capped: true }));
        assert!(resume.request(18.0));
    }

    #[test]
    fn stale_zero_and_late_requests_cannot_release_the_menu() {
        let mut resume = GraphicsResume::default();
        resume.sample(10, 0, 20);
        resume.changed();
        resume.request(1.0);
        assert!(resume.poll(2.0).is_none(), "the old zero is not a fresh frame");
        resume.sample(11, 0, 20);
        assert!(resume.poll(2.0).is_none());
        resume.sample(14, 2, 22);
        assert!(resume.poll(2.5).is_none(), "late work resets settling");
        resume.sample(15, 0, 22);
        assert!(resume.poll(2.6).is_none());
        resume.sample(18, 0, 23);
        assert!(resume.poll(3.0).is_none(), "new requests matter even if already ready");
        resume.sample(19, 0, 23);
        assert!(resume.poll(3.1).is_none());
        resume.sample(21, 0, 23);
        assert!(resume.poll(3.5).is_none(), "three render frames are required");
        resume.sample(22, 0, 23);
        assert_eq!(resume.poll(3.6), Some(Completion { waited: 2.6, pending: 0, capped: false }));
        assert!(resume.poll(4.0).is_none(), "only one completion");
    }

    #[test]
    fn a_change_restarts_settling_and_closure_discards_the_session() {
        let mut resume = GraphicsResume::default();
        resume.changed();
        resume.request(1.0);
        resume.sample(1, 0, 0);
        resume.poll(1.1);
        resume.changed();
        resume.sample(4, 0, 0);
        assert!(resume.poll(1.5).is_none());
        resume.sample(7, 0, 0);
        assert!(resume.poll(1.6).is_none(), "quiet time starts after the change");
        resume.reset();
        assert!(!resume.pending());
        assert!(resume.request(2.0));
        resume.changed();
        resume.request(3.0);
        resume.sample(8, 5, 5);
        assert_eq!(resume.poll(18.0), Some(Completion { waited: 15.0, pending: 5, capped: true }));
    }
}
