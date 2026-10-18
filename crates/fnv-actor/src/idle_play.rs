//! Playing one idle animation over time: its loops, where it stands in its clip, and its
//! fade in and out. Headless: the clip is known here only by its length and text keys;
//! whoever poses the skeleton samples the clip at [`IdlePlay::time`] with
//! [`IdlePlay::weight`]. Which idle plays and why is `fnv_game::idles`.
//!
//! Facts and readings: docs/spec-notes/idles.md §3.

/// Frames per second of the numbers in `Blend:`, `BlendIn:` and `BlendOut:` text keys
/// [hypothesis, as for movement clips: animation.md §9.3].
const KEY_FRAMES_PER_SECOND: f32 = 30.0;

/// Fade time of a clip without a key for it, seconds [engineering choice, as the
/// movement clips' default].
pub const DEFAULT_FADE: f32 = 0.2;

/// What an idle clip's text keys say about playing it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IdleKeys {
    /// The clip's length in seconds.
    pub length: f32,
    /// The stretch that repeats (seconds from the clip's start), from its `StartLoop`
    /// and `EndLoop` keys [observed: the opening's lying-awake clip has them 0.03 s and
    /// 5.97 s into its 6 s, its sitting-on-the-bed clip at 2.1 s and at its end;
    /// hypothesis: a looping idle plays up to `EndLoop` and goes back to `StartLoop`].
    pub loop_range: Option<(f32, f32)>,
    /// Seconds to fade in and out, from `Blend:N` / `BlendIn:N` and `BlendOut:N`
    /// [hypothesis: the number counts frames, and the key's own time means nothing: the
    /// sitting-on-the-bed clip has `BlendOut:10` before its loop starts].
    pub blend_in: f32,
    pub blend_out: f32,
}

impl IdleKeys {
    /// From a clip's start and stop times and its text keys (time, text; a key's text
    /// may hold several lines).
    pub fn read(start: f32, stop: f32, text_keys: &[(f32, String)]) -> IdleKeys {
        let length = (stop - start).max(0.0);
        let (mut loop_start, mut loop_end) = (None, None);
        let (mut blend_in, mut blend_out) = (None, None);
        let frames = |rest: &str| rest.trim().parse::<f32>().ok().filter(|n| *n >= 0.0).map(|n| n / KEY_FRAMES_PER_SECOND);
        for (time, text) in text_keys {
            for line in text.lines() {
                let l: String = line.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_lowercase();
                let at = (time - start).clamp(0.0, length);
                if l == "startloop" {
                    loop_start = Some(at);
                } else if l == "endloop" {
                    loop_end = Some(at);
                } else if let Some(rest) = l.strip_prefix("blendout:") {
                    blend_out = frames(rest).or(blend_out);
                } else if let Some(rest) = ["blendin:", "blenin:", "blend:"].iter().find_map(|p| l.strip_prefix(p)) {
                    // `BlenIn` is how one of the game's clips spells it [observed].
                    blend_in = frames(rest).or(blend_in);
                }
            }
        }
        let loop_range = match (loop_start, loop_end) {
            (Some(a), Some(b)) if b - a > 1e-3 => Some((a, b)),
            _ => None,
        };
        IdleKeys { length, loop_range, blend_in: blend_in.unwrap_or(DEFAULT_FADE), blend_out: blend_out.unwrap_or(DEFAULT_FADE) }
    }

    /// The stretch a loop repeats: the keys', else the whole clip.
    fn looped(&self) -> (f32, f32) {
        self.loop_range.unwrap_or((0.0, self.length))
    }
}

/// One idle playing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IdlePlay {
    pub keys: IdleKeys,
    /// Seconds into the clip.
    t: f32,
    /// Seconds since the idle started.
    age: f32,
    /// Passes of the loop still to play, this one included (`None`: until told to
    /// leave).
    passes: Option<u32>,
    /// Seconds since the idle ended and began to fade out.
    out: Option<f32>,
    /// Seconds the last pose is held at the end of the clip before the idle ends
    /// ([`IdlePlay::again`]).
    hold: f32,
}

impl IdlePlay {
    /// Start an idle that plays its loop `passes` times (`None`: until
    /// [`IdlePlay::leave_loop`] or [`IdlePlay::stop`]).
    pub fn new(keys: IdleKeys, passes: Option<u32>) -> IdlePlay {
        IdlePlay { keys, t: 0.0, age: 0.0, passes: passes.map(|n| n.max(1)), out: None, hold: 0.0 }
    }

    /// The same idle asked for again while this one still shows: it is not started over.
    /// One under way plays on; one that ended (holding its last pose as it fades out)
    /// shows in full again and holds that pose for another clip's length, then ends
    /// [hypothesis: the opening asks Doc Mitchell for `VCG01DocGiveMirror` twice, 3 s
    /// apart (stage 30's `PlayIdle`, then the next line's speaker idle), and he hands
    /// the mirror over once, holding it out at "How'd I do?" (owner, 2026-10-18);
    /// idles.md §3].
    pub fn again(&mut self) {
        if self.out.take().is_some() && self.t >= self.keys.length {
            // Ends once `t` (past the clip's end while held) has gone one length further.
            self.hold = self.t;
        }
    }

    /// Advance by `dt` seconds; true in the step the idle ends (it reached the end of
    /// its clip on its last pass): from then on it holds its last pose and fades out,
    /// and whatever follows may start [design: so two idles in a row cross-fade].
    pub fn advance(&mut self, dt: f32) -> bool {
        let dt = dt.max(0.0);
        self.age += dt;
        if let Some(o) = self.out.as_mut() {
            *o += dt;
            return false;
        }
        self.t += dt;
        let (from, to) = self.keys.looped();
        let span = to - from;
        if span <= 1e-3 {
            // Nothing to repeat.
            self.passes = Some(1);
        }
        while self.t >= to && self.passes != Some(1) {
            self.t -= span;
            self.passes = self.passes.map(|n| n - 1);
        }
        if self.passes == Some(1) && self.t >= self.keys.length + self.hold {
            self.t = self.keys.length;
            self.out = Some(0.0);
            return true;
        }
        false
    }

    /// Play the pass under way to the end of the clip, then end (a looping idle whose
    /// package is over) [design].
    pub fn leave_loop(&mut self) {
        self.passes = Some(1);
    }

    /// End now: the idle fades out from where it is.
    pub fn stop(&mut self) {
        self.out.get_or_insert(0.0);
    }

    /// Seconds into the clip to sample.
    pub fn time(&self) -> f32 {
        self.t.clamp(0.0, self.keys.length)
    }

    /// How much of the idle shows, 0 to 1: fading in from its start, out from its end.
    pub fn weight(&self) -> f32 {
        let ramp = |t: f32, over: f32| if over <= 1e-4 { 1.0 } else { (t / over).clamp(0.0, 1.0) };
        ramp(self.age, self.keys.blend_in) * (1.0 - self.out.map_or(0.0, |o| ramp(o, self.keys.blend_out)))
    }

    /// Ended (fading out, or gone).
    pub fn ended(&self) -> bool {
        self.out.is_some()
    }

    /// Faded out: nothing of it shows any more.
    pub fn gone(&self) -> bool {
        self.out.is_some_and(|o| o >= self.keys.blend_out)
    }

    /// Loops until told to leave.
    pub fn endless(&self) -> bool {
        self.passes.is_none() && self.out.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(list: &[(f32, &str)]) -> Vec<(f32, String)> {
        list.iter().map(|(t, s)| (*t, s.to_string())).collect()
    }

    #[test]
    fn text_keys_give_the_loop_and_the_fades() {
        // The shape of the opening's lying-awake clip.
        let k = IdleKeys::read(0.0, 6.0, &keys(&[(0.0, "start"), (0.0333, "StartLoop"), (0.1333, "Sound: Roll"), (5.8333, "BlendOut:10"), (5.9667, "EndLoop"), (6.0, "end")]));
        assert_eq!(k.length, 6.0);
        let (a, b) = k.loop_range.unwrap();
        assert!((a - 0.0333).abs() < 1e-4 && (b - 5.9667).abs() < 1e-4);
        assert!((k.blend_out - 10.0 / 30.0).abs() < 1e-5 && k.blend_in == DEFAULT_FADE);
        // Spellings and several lines in one key; times count from the clip's start.
        let k = IdleKeys::read(1.0, 9.0, &keys(&[(1.9, "BlenIn:30"), (2.4, "Blend Out: 15\r\nEnum: Left"), (3.1, "startloop"), (9.0, "ENDLOOP\nend")]));
        assert_eq!((k.blend_in, k.blend_out), (1.0, 0.5));
        let (a, b) = k.loop_range.unwrap();
        assert!((a - 2.1).abs() < 1e-4 && (b - 8.0).abs() < 1e-4);
        // `Blend:N` is the fade in; no loop keys, no loop stretch.
        let k = IdleKeys::read(0.0, 2.0, &keys(&[(0.1, "Blend:6"), (0.5, "EndLoop")]));
        assert_eq!((k.blend_in, k.blend_out, k.loop_range), (0.2, DEFAULT_FADE, None));
    }

    fn run(p: &mut IdlePlay, seconds: f32) -> Option<f32> {
        let dt = 1.0 / 64.0;
        let mut t = 0.0;
        while t < seconds {
            t += dt;
            if p.advance(dt) {
                return Some(t);
            }
        }
        None
    }

    const PLAIN: IdleKeys = IdleKeys { length: 2.0, loop_range: None, blend_in: 0.5, blend_out: 0.25 };
    const LOOPED: IdleKeys = IdleKeys { length: 6.0, loop_range: Some((1.0, 5.0)), blend_in: 0.0, blend_out: 0.5 };

    #[test]
    fn a_one_shot_idle_plays_to_its_end_and_fades_out() {
        let mut p = IdlePlay::new(PLAIN, Some(1));
        assert_eq!(p.weight(), 0.0);
        p.advance(0.25);
        assert_eq!((p.time(), p.weight(), p.ended()), (0.25, 0.5, false));
        let ended = run(&mut p, 10.0).unwrap();
        assert!((ended - 1.75).abs() < 0.02, "{ended}");
        assert_eq!((p.time(), p.weight(), p.ended(), p.gone()), (2.0, 1.0, true, false));
        // Holding its last pose while it fades.
        p.advance(0.125);
        assert_eq!((p.time(), p.weight()), (2.0, 0.5));
        assert!(!p.advance(1.0) && p.gone() && p.weight() == 0.0);
    }

    #[test]
    fn loops_repeat_the_stretch_between_the_keys() {
        // Three passes: in to 5, back to 1 twice, then out to the end: 5 + 4 + 4 + 1.
        let mut p = IdlePlay::new(LOOPED, Some(3));
        let ended = run(&mut p, 60.0).unwrap();
        assert!((ended - 14.0).abs() < 0.02, "{ended}");
        // Without keys the whole clip repeats.
        let mut p = IdlePlay::new(PLAIN, Some(2));
        let ended = run(&mut p, 60.0).unwrap();
        assert!((ended - 4.0).abs() < 0.02, "{ended}");
    }

    #[test]
    fn an_endless_idle_stays_in_its_loop_until_told_to_leave() {
        let mut p = IdlePlay::new(LOOPED, None);
        assert_eq!(run(&mut p, 50.0), None);
        assert!(p.endless() && (1.0..5.0).contains(&p.time()) && p.weight() == 1.0);
        // Leaving: the pass under way plays out, then the rest of the clip.
        let before = p.time();
        p.leave_loop();
        let ended = run(&mut p, 10.0).unwrap();
        assert!((ended - (6.0 - before)).abs() < 0.02, "{ended} from {before}");
        // Stopping fades out from where the idle is.
        let mut p = IdlePlay::new(LOOPED, None);
        run(&mut p, 2.0);
        p.stop();
        assert!(p.ended() && !p.gone());
        let at = p.time();
        p.advance(0.25);
        assert_eq!((p.time(), p.weight()), (at, 0.5));
    }

    #[test]
    fn asked_again_an_idle_is_not_started_over() {
        // Under way: it plays on.
        let mut p = IdlePlay::new(PLAIN, Some(1));
        p.advance(1.0);
        p.again();
        assert_eq!((p.time(), p.ended()), (1.0, false));
        // Ended and fading: it shows in full again, holds its last pose for another
        // length, then ends.
        run(&mut p, 10.0).unwrap();
        p.advance(0.125);
        p.again();
        assert_eq!((p.time(), p.weight(), p.ended()), (2.0, 1.0, false));
        let ended = run(&mut p, 10.0).unwrap();
        assert!((ended - 2.0).abs() < 0.02, "{ended}");
        assert_eq!(p.time(), 2.0);
    }

    #[test]
    fn a_big_step_does_not_lose_the_loop() {
        let mut p = IdlePlay::new(LOOPED, None);
        assert!(!p.advance(1000.0));
        assert!((1.0..5.0).contains(&p.time()));
        // A clip of no length ends at once instead of looping for ever.
        let mut p = IdlePlay::new(IdleKeys { length: 0.0, loop_range: None, blend_in: 0.0, blend_out: 0.0 }, Some(4));
        assert!(run(&mut p, 1.0).is_some());
    }
}
