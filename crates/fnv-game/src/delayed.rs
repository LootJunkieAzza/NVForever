//! xNVSE's delayed calls (task D3's rest; docs/spec-notes/ext-udf.md §6):
//! CallAfterSeconds, CallAfterFrames, CallForSeconds, CallWhile, CallWhen and
//! CallWhilePerSeconds [documented: their pages]. Not saved: "Every queued call is removed
//! when loading the game" [documented], so a load starts with none. Run from
//! `Game::tick`; seconds are game seconds (`cfg.frame_seconds`, which the engine scales
//! with the time multiplier), counting down in MenuMode too when the call is eligible
//! there [documented: CallAfterSeconds, CallForSeconds] and pausing whenever `runWhen`
//! or `flags` holds the call back.

use crate::game::{menus, Game, RunCtx};
use fnv_script::vm::{Value, XVal};
use fnv_sim::FormId;

/// Tolerance for summing `frame_seconds` across frames without IEEE-754 rounding drift.
const TIME_EPS: f64 = 1e-9;

/// What decides when a queued call runs.
#[derive(Debug, Clone, PartialEq)]
enum When {
    /// Once, after this many remaining eligible seconds (0.0 runs next eligible frame).
    AfterSeconds(f64),
    /// Once, after this many remaining eligible frames (0 runs next eligible frame).
    AfterFrames(u64),
    /// Every eligible frame while remaining eligible seconds > 0.
    ForSeconds(f64),
    /// Every eligible frame while the condition gives true; dropped the first time it does not.
    While(XVal),
    /// Once, the first eligible frame the condition gives true.
    Once(XVal),
    /// Every `interval` eligible seconds while the condition gives true.
    WhilePerSeconds { cond: XVal, interval: f64, remaining: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Delayed {
    when: When,
    func: XVal,
    on: Option<FormId>,
    args: Vec<XVal>,
    /// For the timers: runWhen 0 game mode only, 1 also menu mode, 2 also menu mode but
    /// paused in the pause and console menus [documented]. For the conditions: the flags
    /// (1 args to the function, 2 args to the condition, 4 not in menu mode, 8 not in game
    /// mode, 16 not while paused) [documented].
    mode: u32,
    condition_flags: bool,
    /// Kept across a load (SUP's CallFunctionNextFrame with removeOnLoad 0).
    keep_on_load: bool,
}

impl Delayed {
    /// Whether it may run and advance its timer this frame.
    fn runs_now(&self, menu: bool, paused: bool) -> bool {
        if self.condition_flags {
            !(menu && self.mode & 4 != 0) && !(!menu && self.mode & 8 != 0) && !(paused && self.mode & 16 != 0)
        } else {
            match self.mode {
                0 => !menu,
                1 => true,
                _ => !paused,
            }
        }
    }
}

/// The delayed-call functions; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, on: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let on = on.or(ctx.self_ref);
    let num = |k: usize| match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => 0.0,
    };
    let item = |when: When, func: usize, mode: u32, rest: usize, condition_flags: bool| Delayed {
        when,
        func: args.get(func).cloned().unwrap_or(XVal::Num(0.0)),
        on,
        args: args.iter().skip(rest).cloned().collect(),
        mode,
        condition_flags,
        keep_on_load: false,
    };
    let d = match name {
        // `seconds udf [runWhen] args…`; 0 seconds runs next frame [documented].
        "callafterseconds" => item(When::AfterSeconds(num(0).max(0.0)), 1, num(2) as u32, 3, false),
        // [hypothesis: no page in the export; the catalogue gives frames, udf, runWhen,
        // args, so it is CallAfterSeconds counted in frames].
        "callafterframes" => item(When::AfterFrames(num(0).max(0.0) as u64), 1, num(2) as u32, 3, false),
        "callforseconds" => item(When::ForSeconds(num(0).max(0.0)), 1, num(2) as u32, 3, false),
        // `udf condition [flags] args…`.
        "callwhile" => item(When::While(args.get(1).cloned().unwrap_or(XVal::Num(0.0))), 0, num(2) as u32, 3, true),
        "callwhen" => item(When::Once(args.get(1).cloned().unwrap_or(XVal::Num(0.0))), 0, num(2) as u32, 3, true),
        // `interval udf condition [flags] args…`.
        "callwhileperseconds" => {
            let interval = num(0).max(0.0);
            item(When::WhilePerSeconds { cond: args.get(2).cloned().unwrap_or(XVal::Num(0.0)), interval, remaining: interval }, 1, num(3) as u32, 4, true)
        }
        _ => return None,
    };
    // An array argument lives until the calls stop [documented].
    for a in &d.args {
        g.ext.retain(a);
    }
    g.delayed.push(d);
    Some(Ok(XVal::Num(0.0)))
}

/// Call the function `func` with `args` next frame, in a menu too (SUP's dialog boxes
/// answer their handler this way, `crate::nvse_sup5`).
pub(crate) fn call_next_frame(g: &mut Game, func: XVal, on: Option<FormId>, args: Vec<XVal>) {
    for a in &args {
        g.ext.retain(a);
    }
    g.delayed.push(Delayed { when: When::AfterFrames(1), func, on, args, mode: 1, condition_flags: false, keep_on_load: false });
}

/// SUP's CallFunctionNextFrame (`crate::nvse_sup4`): `func` on `on` `frames` frames from
/// now, in menus too [hypothesis: frames pass in menu mode as well]; `keep` outlasts a
/// load (`Game::take_session`).
pub(crate) fn call_in_frames(g: &mut Game, func: XVal, on: Option<FormId>, frames: u64, keep: bool) {
    g.delayed.push(Delayed { when: When::AfterFrames(frames), func, on, args: Vec::new(), mode: 1, condition_flags: false, keep_on_load: keep });
}

impl Game<'_> {
    /// Whether the game is in a main, pause, or console menu for delayed-call pause
    /// flags (`runWhen == 2` and `DontRunWhilePaused` bit 16) [documented: ext-udf.md §6].
    fn delayed_paused(&self) -> bool {
        let is_pause = |c: u16| matches!(c, menus::CONSOLE | menus::TITLE | menus::START);
        self.start_frames || self.menu.is_some_and(is_pause) || self.page_menu.is_some_and(is_pause)
    }

    /// The calls that outlast a load, their remaining frames kept: those of function
    /// scripts only (a lambda is gone with the game it was made in).
    pub(crate) fn take_kept_calls(&mut self) -> Vec<Delayed> {
        self.delayed
            .iter()
            .filter(|d| d.keep_on_load && self.session_function(&d.func) && matches!(d.when, When::AfterFrames(_)))
            .cloned()
            .collect()
    }

    /// The kept calls back after a load.
    pub(crate) fn put_kept_calls(&mut self, calls: Vec<Delayed>) {
        for mut d in calls {
            if let When::AfterFrames(left) = d.when {
                d.when = When::AfterFrames(left.max(1));
            }
            self.delayed.push(d);
        }
    }

    /// The queued calls due this frame.
    pub(crate) fn run_delayed(&mut self) {
        if self.delayed.is_empty() {
            return;
        }
        let menu = self.menu.is_some() || self.page_menu.is_some() || self.dialogue.is_some();
        let paused = self.delayed_paused();
        let dt = if self.gaps.jip3.time_frozen { 0.0 } else { self.cfg.frame_seconds };
        let queued = std::mem::take(&mut self.delayed);
        let mut kept = Vec::new();
        for mut d in queued {
            if !d.runs_now(menu, paused) {
                kept.push(d);
                continue;
            }
            let to_func = !d.condition_flags || d.mode & 1 != 0;
            let to_cond = d.condition_flags && d.mode & 2 != 0;
            let (run, keep) = match d.when.clone() {
                When::AfterSeconds(left) => {
                    let next = left - dt;
                    let due = next <= TIME_EPS;
                    d.when = When::AfterSeconds(next.max(0.0));
                    (due, !due)
                }
                When::AfterFrames(left) => {
                    let next = left.saturating_sub(1);
                    d.when = When::AfterFrames(next);
                    (next == 0, next > 0)
                }
                When::ForSeconds(left) => {
                    let run = left > TIME_EPS;
                    let next = left - dt;
                    let keep = next > TIME_EPS;
                    d.when = When::ForSeconds(next.max(0.0));
                    (run, keep)
                }
                When::While(c) => {
                    let ok = self.delayed_truth(&c, &d, to_cond);
                    (ok, ok)
                }
                When::Once(c) => {
                    let ok = self.delayed_truth(&c, &d, to_cond);
                    (ok, !ok)
                }
                When::WhilePerSeconds { cond, interval, remaining } => {
                    let next = remaining - dt;
                    if next > TIME_EPS {
                        d.when = When::WhilePerSeconds { cond, interval, remaining: next };
                        (false, true)
                    } else {
                        let ok = self.delayed_truth(&cond, &d, to_cond);
                        d.when = When::WhilePerSeconds { cond, interval, remaining: interval };
                        (ok, ok)
                    }
                }
            };
            if run {
                let args = if to_func { d.args.clone() } else { Vec::new() };
                self.delayed_run(&d.func, d.on, args, "delayed call");
            }
            if keep {
                kept.push(d);
            } else {
                for a in &d.args {
                    self.ext.release(a);
                }
            }
        }
        // Calls queued while these ran come after.
        kept.append(&mut self.delayed);
        self.delayed = kept;
    }

    fn delayed_truth(&mut self, cond: &XVal, d: &Delayed, with_args: bool) -> bool {
        let args = if with_args { d.args.clone() } else { Vec::new() };
        self.delayed_run(cond, d.on, args, "delayed call's condition").is_some_and(|v| v.truthy())
    }

    fn delayed_run(&mut self, func: &XVal, on: Option<FormId>, args: Vec<XVal>, what: &str) -> Option<XVal> {
        let ctx = RunCtx { self_ref: on, ..RunCtx::default() };
        match self.run_function(func, on.map(|f| Value::Ref(f.0)), args, None, ctx) {
            Ok(v) => Some(v),
            Err(error) => {
                self.handler_failed(what.into(), error);
                None
            }
        }
    }
}
