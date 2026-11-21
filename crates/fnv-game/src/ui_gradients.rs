//! Traits changed over real time (task T9, docs/design/ui-tile-tree.md §5.2): JIP's
//! SetUIFloatGradual with SUP's readers of it, CacheUI's gradual functions and the string
//! reveals. A script starts a change; the engine steps every change once a frame with the
//! frame's real time ([`Gradients::step`], called after the HUD's writes and the scripts,
//! before the tree is worked out), so the changes are "unaffected by Time Mult"
//! [documented: GECK wiki SetUIFloatGradual].
//!
//! Each change sits on a tile and a trait by name; one whose tile is unloaded is dropped.
//! The table is emptied when the tree is built again (after a load): the tiles it named
//! are gone [hypothesis: the original's tiles go with their menu as well].

use fnv_ui::{TileId, Ui, Value};

/// Which plugin's table a change is in: JIP's (SetUIFloatGradual, read by SUP's
/// GetUIFloatGradual… functions, whose pages read "gradually changing via
/// SetUIFloatGradual") or CacheUI's (its gradual and reveal functions, with their
/// delays and locks) [documented: the GECK wiki pages; design: the two plugins keep
/// their own tables, so one plugin's stop does not stop the other's].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Jip,
    CacheUi,
}

/// What a change writes.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// A number from `start` to `end` in one of the change modes (see [`float_at`]).
    /// `start` is `None` while a CacheUI delay runs: the value is pulled when it ends
    /// [documented: CacheUIGradualDelay, "pulling current value"].
    Float { start: Option<f64>, end: f64, mode: u8 },
    /// A string shown a letter at a time (SetUIStringReveal, CacheUIStringReveal).
    Reveal { text: String, mode: u8, invert: bool },
}

#[derive(Clone, Debug)]
pub struct Gradient {
    pub owner: Owner,
    pub tile: TileId,
    /// The trait's name as the path gave it.
    pub name: String,
    pub kind: Kind,
    pub duration: f64,
    /// Seconds since the change started (after any delay).
    pub elapsed: f64,
    /// Seconds of delay left (CacheUIGradualDelay); the change is "invisible" until then
    /// [documented: CacheUIGradualDelay notes].
    pub delay: f64,
    /// Locked until it finishes: only a stop or another locking delay replaces it
    /// [documented: CacheUIGradualDelay, flag 2 and its notes].
    pub lock: bool,
    /// A trait of the same tile and its value when the delay started: a change of it
    /// during the delay ends this change [documented: CacheUIGradualDelay, 8th argument].
    pub watch: Option<(String, Value)>,
    /// When its delay ends and another change of the trait is running: gives way to it
    /// (CacheUIGradualDelay's flag 0) rather than replacing it (flags 1 and 2)
    /// [documented: CacheUIGradualDelay; hypothesis: the flag is read when the delay ends,
    /// as well as when the call is made]. An undelayed change never yields.
    pub yields: bool,
}

impl Gradient {
    fn on(&self, owner: Owner, tile: TileId, name: &str) -> bool {
        self.owner == owner && self.tile == tile && self.name.eq_ignore_ascii_case(name)
    }

    /// The change mode, as GetUIFloatGradualMode gives it.
    pub fn mode(&self) -> u8 {
        match self.kind {
            Kind::Float { mode, .. } | Kind::Reveal { mode, .. } => mode,
        }
    }

    /// Whether it runs on without end: modes 2 and 3 [documented: SetUIFloatGradual,
    /// "repeated perpetually"].
    pub fn repeats(&self) -> bool {
        matches!(self.kind, Kind::Float { mode: 2 | 3, .. })
    }
}

/// The float change modes built: 0 to 3 [documented: SetUIFloatGradual], and CacheUI's 5
/// (ease in) and 6 (smooth strobe) [documented: CacheUIGradual, "As of Update 1.09"]. Mode
/// 4 (a rumble whose "intensity decays") is not built: the page gives neither the shake
/// nor the decay (menu-xml.md §9).
pub fn mode_built(mode: u8) -> bool {
    matches!(mode, 0..=3 | 5 | 6)
}

/// A duration that gives a change no time to run: none, negative, or not a number.
fn no_time(duration: f64) -> bool {
    !duration.is_finite() || duration <= 0.0
}

fn lerp(a: f64, b: f64, p: f64) -> f64 {
    a + (b - a) * p
}

/// A float change's value `t` seconds in, and whether it has finished [documented:
/// SetUIFloatGradual's modes; hypothesis: the change is linear in time, the page says
/// "gradually"; mode 5's acceleration is taken as the square of the time's share, the
/// page gives no curve; mode 6, "similar to mode 2 but only runs for duration", is one
/// up-and-down that ends at the start].
pub fn float_at(start: f64, end: f64, mode: u8, duration: f64, t: f64) -> (f64, bool) {
    if no_time(duration) {
        // [hypothesis] No time (none, negative, or not a number): the end value at once,
        // for every mode.
        return (end, true);
    }
    let share = (t / duration).clamp(0.0, 1.0);
    match mode {
        // From startVal to endVal in duration/6, endVal for duration*2/3, back in duration/6.
        1 => {
            let sixth = duration / 6.0;
            if t >= duration {
                (start, true)
            } else if t < sixth {
                (lerp(start, end, t / sixth), false)
            } else if t < duration - sixth {
                (end, false)
            } else {
                (lerp(end, start, (t - (duration - sixth)) / sixth), false)
            }
        }
        // There and back in duration, again and again.
        2 => (there_and_back(start, end, (t % duration) / duration), false),
        // There in duration, again and again.
        3 => (lerp(start, end, (t % duration) / duration), false),
        5 => (lerp(start, end, share * share), t >= duration),
        6 if t >= duration => (start, true),
        6 => (there_and_back(start, end, share), false),
        _ => (lerp(start, end, share), t >= duration),
    }
}

fn there_and_back(start: f64, end: f64, share: f64) -> f64 {
    if share < 0.5 {
        lerp(start, end, share * 2.0)
    } else {
        lerp(end, start, (share - 0.5) * 2.0)
    }
}

/// A reveal's string `t` seconds in, and whether it has finished [documented:
/// SetUIStringReveal, "H", "He", "Hel" … from the left, and inverted "Hello World",
/// "ello World", "llo World"; hypothesis: the letters shown are the share of the time
/// passed, rounded down; from the right (modes 1, 3) shows the last letters (inverted:
/// drops the last ones); ease in (modes 2, 3) is the square of the time's share].
pub fn reveal_at(text: &str, mode: u8, invert: bool, duration: f64, t: f64) -> (String, bool) {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut share = if no_time(duration) { 1.0 } else { (t / duration).clamp(0.0, 1.0) };
    if mode >= 2 {
        share *= share;
    }
    let n = ((len as f64) * share).floor() as usize;
    let n = n.min(len);
    let from_right = mode % 2 == 1;
    let s: String = match (invert, from_right) {
        (false, false) => chars[..n].iter().collect(),
        (false, true) => chars[len - n..].iter().collect(),
        (true, false) => chars[n..].iter().collect(),
        (true, true) => chars[..len - n].iter().collect(),
    };
    (s, share >= 1.0)
}

/// The changes running.
#[derive(Default)]
pub struct Gradients {
    pub list: Vec<Gradient>,
}

impl Gradients {
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Forgets every change (the tree was built again).
    pub fn clear(&mut self) {
        self.list.clear();
    }

    /// Stops `owner`'s changes of a trait, delayed or locked ones too [documented:
    /// CacheUIGradualDelay notes, a stop overrides a lock].
    pub fn stop(&mut self, owner: Owner, tile: TileId, name: &str) {
        self.list.retain(|g| !g.on(owner, tile, name));
    }

    /// The running (not delayed) change of a trait in `owner`'s table.
    pub fn active(&self, owner: Owner, tile: TileId, name: &str) -> Option<&Gradient> {
        self.list.iter().find(|g| g.on(owner, tile, name) && g.delay <= 0.0)
    }

    /// Whether a change of `owner`'s on the trait is locked.
    pub fn locked(&self, owner: Owner, tile: TileId, name: &str) -> bool {
        self.list.iter().any(|g| g.on(owner, tile, name) && g.lock)
    }

    /// Any change of `owner`'s on a trait of the tile.
    pub fn on_tile(&self, owner: Owner, tile: TileId) -> bool {
        self.list.iter().any(|g| g.owner == owner && g.tile == tile && g.delay <= 0.0)
    }

    /// Starts a change; the first value is written now [documented: SetUIFloatGradual,
    /// "first setting it to startValue"]. It replaces the owner's running change of the
    /// trait; a delayed change replaces delayed ones too, an undelayed one leaves them, as
    /// a delay "is invisible" to CacheUIGradual until it has run [documented:
    /// CacheUIGradualDelay notes]. A change with no time to run is finished as written
    /// and not kept (so a zero-length locked change locks nothing) [design].
    pub fn start(&mut self, ui: &mut Ui, mut g: Gradient) {
        let delayed = g.delay > 0.0;
        self.list.retain(|x| !(x.on(g.owner, g.tile, &g.name) && (delayed || x.delay <= 0.0)));
        if !delayed {
            if let Kind::Float { start: start @ None, .. } = &mut g.kind {
                *start = Some(ui.tile(g.tile).get(&g.name).map_or(0.0, Value::num));
            }
            if write(ui, &g, 0.0) && !g.repeats() {
                return;
            }
        }
        self.list.push(g);
    }

    /// One frame: every change moves on by `dt` seconds of real time, writing its trait;
    /// finished ones, and ones whose tile is gone, are dropped. Nothing to do (and no tree
    /// work) when no change runs.
    pub fn step(&mut self, ui: &mut Ui, dt: f64) {
        if self.list.is_empty() {
            return;
        }
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let mut i = 0;
        while i < self.list.len() {
            let was_delayed = self.list[i].delay > 0.0;
            if !step_one(ui, &mut self.list[i], dt) {
                self.list.remove(i);
                continue;
            }
            // Its delay ended this frame: it meets the trait's running change now
            // (`Gradient::yields`): it gives way, or that change goes.
            if was_delayed && self.list[i].delay <= 0.0 {
                let (owner, tile, name, yields) = (self.list[i].owner, self.list[i].tile, self.list[i].name.clone(), self.list[i].yields);
                let other = |x: &Gradient| x.on(owner, tile, &name) && x.delay <= 0.0;
                if yields && self.list.iter().enumerate().any(|(k, x)| k != i && other(x)) {
                    self.list.remove(i);
                    continue;
                }
                let mut k = 0;
                while k < self.list.len() {
                    if k != i && other(&self.list[k]) {
                        self.list.remove(k);
                        if k < i {
                            i -= 1;
                        }
                    } else {
                        k += 1;
                    }
                }
            }
            i += 1;
        }
    }
}

fn step_one(ui: &mut Ui, g: &mut Gradient, dt: f64) -> bool {
    if !ui.tile(g.tile).live {
        return false;
    }
    let mut dt = dt;
    if g.delay > 0.0 {
        if let Some((name, was)) = &g.watch {
            let now = ui.tile(g.tile).get(name).cloned().unwrap_or_default();
            if !now.same(was) {
                return false;
            }
        }
        g.delay -= dt;
        if g.delay > 0.0 {
            return true;
        }
        // The delay's overshoot runs as change time.
        dt = -g.delay;
        g.delay = 0.0;
        if let Kind::Float { start, .. } = &mut g.kind {
            let now = ui.tile(g.tile).get(&g.name).map_or(0.0, Value::num);
            *start = Some(now);
        }
    }
    g.elapsed += dt;
    !write(ui, g, g.elapsed)
}

/// Writes a change's value at `t`; true when it has finished.
fn write(ui: &mut Ui, g: &Gradient, t: f64) -> bool {
    let (v, done) = match &g.kind {
        Kind::Float { start, end, mode } => {
            let start = start.unwrap_or_else(|| ui.tile(g.tile).get(&g.name).map_or(0.0, Value::num));
            let (v, done) = float_at(start, *end, *mode, g.duration, t);
            (Value::Num(v), done)
        }
        Kind::Reveal { text, mode, invert } => {
            let (s, done) = reveal_at(text, *mode, *invert, g.duration, t);
            (Value::Str(s), done)
        }
    };
    ui.set_trait(g.tile, &g.name, v);
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_modes_over_time() {
        // Mode 0: 200 to 50 over 4 s [documented: the page's first example].
        assert_eq!(float_at(200.0, 50.0, 0, 4.0, 2.0), (125.0, false));
        assert_eq!(float_at(200.0, 50.0, 0, 4.0, 4.0), (50.0, true));
        // Mode 1 over 6 s: up in 1 s, held 4 s, down in 1 s [documented: second example].
        assert_eq!(float_at(0.0, 255.0, 1, 6.0, 0.5).0, 127.5);
        assert_eq!(float_at(0.0, 255.0, 1, 6.0, 3.0), (255.0, false));
        assert_eq!(float_at(0.0, 255.0, 1, 6.0, 5.5).0, 127.5);
        assert_eq!(float_at(0.0, 255.0, 1, 6.0, 6.0), (0.0, true));
        // Mode 2: up and back within 4 s, repeated.
        assert_eq!(float_at(0.0, 100.0, 2, 4.0, 2.0), (100.0, false));
        assert_eq!(float_at(0.0, 100.0, 2, 4.0, 5.0), (50.0, false));
        // Mode 3: up in 5 s, repeated.
        assert_eq!(float_at(0.0, 10.0, 3, 5.0, 7.5), (5.0, false));
        // CacheUI's ease in and strobe.
        assert_eq!(float_at(0.0, 100.0, 5, 2.0, 1.0), (25.0, false));
        assert_eq!(float_at(0.0, 100.0, 6, 2.0, 2.0), (0.0, true));
    }

    #[test]
    fn reveals_letter_by_letter() {
        assert_eq!(reveal_at("Hello", 0, false, 5.0, 1.0).0, "H");
        assert_eq!(reveal_at("Hello", 0, false, 5.0, 2.0).0, "He");
        assert_eq!(reveal_at("Hello", 0, true, 5.0, 1.0).0, "ello");
        assert_eq!(reveal_at("Hello", 1, false, 5.0, 2.0).0, "lo");
        assert_eq!(reveal_at("Hello", 0, false, 5.0, 5.0), ("Hello".to_string(), true));
    }
}
