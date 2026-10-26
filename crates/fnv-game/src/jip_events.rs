//! JIP LN's own event handlers (task J6; docs/spec-notes/jip-events.md): the
//! `SetOn…EventHandler` functions and SetGameMainLoopCallback. Beside the extender's
//! SetEventHandler (`Game::dispatch_event`), with JIP's own rules:
//! - several handlers for one event run **newest first** [observed: ev.order];
//! - a handler that declares fewer parameters than the event passes never runs
//!   [observed: ev.noparams, ev.oneparam]; NVForever logs why [decision];
//! - registrations are not saved: a load starts with none [documented for the ragdoll
//!   event; hypothesis for the others, jip-events.md §1].

use crate::game::{Game, RunCtx};
use crate::trace::EventKind;
use fnv_script::vm::{Value, XVal};
use fnv_sim::FormId;

/// XMarker: the "no filter" form some events take, since 0 is refused [documented:
/// SetOnCriticalHitEventHandler].
const XMARKER: u32 = 0x0000_003B;

/// The JIP registrations of a session.
#[derive(Debug, Clone, Default)]
pub struct JipEvents {
    pub handlers: Vec<JipHandler>,
    pub main_loop: Vec<MainLoop>,
    seq: u64,
    /// Each actor's hit in progress, or its last one (task J7, jip-hit-info.md §5).
    pub hits: std::collections::HashMap<FormId, JipHit>,
}

/// What JIP's GetHit… functions read [documented: jip-hit-info.md §1].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct JipHit {
    pub attacker: Option<FormId>,
    pub weapon: Option<FormId>,
    pub health_damage: f64,
    pub limb_damage: f64,
    pub location: i32,
    /// GetHitExtendedFlag's bits: 1 critical, 2 fatal, 5 limb crippled (bit n = flag n).
    pub flags: u16,
}

/// One `SetOn<Event>EventHandler handler 1 filters…`.
#[derive(Debug, Clone, PartialEq)]
pub struct JipHandler {
    /// The name between `SetOn` and `EventHandler`, lower case (`healthdamage`).
    pub event: String,
    pub func: XVal,
    pub filters: Vec<XVal>,
    pub seq: u64,
}

/// One SetGameMainLoopCallback registration.
#[derive(Debug, Clone, PartialEq)]
pub struct MainLoop {
    pub func: XVal,
    /// The reference it was called on; the player when none [documented].
    pub on: Option<FormId>,
    /// Every this many frames [documented: frames, not time].
    pub delay: u64,
    /// 1 game mode, 2 menu mode, 8 removed on load [documented].
    pub flags: u32,
    pub due: u64,
}

/// What a filter at each position tests: the calling reference or an argument
/// [documented: the table of jip-events.md §2].
#[derive(Clone, Copy)]
enum Slot {
    On,
    Arg(usize),
}

fn slots(event: &str) -> &'static [Slot] {
    match event {
        "hit" | "healthdamage" | "crippledlimb" | "fireweapon" | "projectileimpact" => &[Slot::On],
        "animaction" | "playgroup" => &[Slot::On, Slot::Arg(0)],
        "criticalhit" => &[Slot::On, Slot::Arg(0), Slot::Arg(1)],
        "queststage" => &[Slot::Arg(0), Slot::Arg(1)],
        "useaiditem" | "dialogtopic" | "keydown" | "keyup" | "controldown" | "controlup" | "triggerdown" | "triggerup" | "menuopen" | "menuclose" | "mouseoverchange" => &[Slot::Arg(0)],
        _ => &[],
    }
}

impl Game<'_> {
    /// `SetOn<event>EventHandler func add filters…`; a second identical registration is
    /// kept once [decision, jip-events.md §2]. Returns whether anything changed.
    pub(crate) fn jip_set(&mut self, event: String, func: XVal, add: bool, filters: Vec<XVal>) -> bool {
        let same = |h: &JipHandler| h.event == event && h.func == func && h.filters == filters;
        if add {
            if self.jip.handlers.iter().any(same) {
                return true;
            }
            self.jip.seq += 1;
            let seq = self.jip.seq;
            self.jip.handlers.push(JipHandler { event, func, filters, seq });
            true
        } else {
            let before = self.jip.handlers.len();
            self.jip.handlers.retain(|h| !same(h));
            self.jip.handlers.len() < before
        }
    }

    /// SetGameMainLoopCallback func add [delay] [flags].
    pub(crate) fn jip_main_loop(&mut self, func: XVal, add: bool, delay: u64, flags: u32, on: Option<FormId>) {
        self.jip.main_loop.retain(|m| !(m.func == func && m.on == on));
        if add {
            let delay = delay.max(1);
            self.jip.main_loop.push(MainLoop { func, on, delay, flags, due: self.frame + delay });
        }
    }

    /// Run the handlers of a JIP event, newest first; returns how many ran.
    pub fn jip_event(&mut self, event: &str, on: Option<FormId>, args: Vec<XVal>) -> usize {
        if self.jip.handlers.is_empty() {
            return 0;
        }
        let mut hs: Vec<JipHandler> = self.jip.handlers.iter().filter(|h| h.event == event && self.jip_matches(h, on, &args)).cloned().collect();
        hs.sort_by_key(|h| std::cmp::Reverse(h.seq));
        let ctx = RunCtx { self_ref: on, ..RunCtx::default() };
        let mut ran = 0;
        for h in &hs {
            if self.function_arity(&h.func).is_some_and(|n| n < args.len()) {
                tracing::info!("jip: a {event} handler takes fewer than the event's {} values and does not run", args.len());
                continue;
            }
            match self.run_function(&h.func, on.map(|f| Value::Ref(f.0)), args.clone(), None, ctx) {
                Ok(_) => ran += 1,
                Err(error) => self.emit(EventKind::ScriptError { unit: format!("JIP {event} handler"), error }),
            }
        }
        ran
    }

    fn jip_matches(&self, h: &JipHandler, on: Option<FormId>, args: &[XVal]) -> bool {
        slots(&h.event).iter().zip(&h.filters).all(|(s, f)| {
            if self.jip_wildcard(f) {
                return true;
            }
            let v = match s {
                Slot::On => on.map(|r| XVal::Ref(r.0)),
                Slot::Arg(i) => args.get(*i).cloned(),
            };
            v.is_some_and(|v| self.filter_matches(f, &v, false))
        })
    }

    /// A filter that lets everything through: none (0), XMarker, or an empty form list
    /// [documented: SetOnAnimActionEventHandler, an empty list means any actor].
    fn jip_wildcard(&self, f: &XVal) -> bool {
        match f {
            XVal::Num(x) => *x == 0.0,
            XVal::Ref(0) | XVal::Ref(XMARKER) => true,
            XVal::Ref(r) => self.data.sig(FormId(*r)) == Some(*b"FLST") && crate::functions::list_members(self, FormId(*r)).is_empty(),
            _ => false,
        }
    }

    /// Whether a quest-stage registration that matches asked to skip the stage's result
    /// script (skipScript, the third filter) [documented: SetOnQuestStageEventHandler].
    pub(crate) fn jip_skips_stage(&self, quest: FormId, stage: i32) -> bool {
        let args = [XVal::Ref(quest.0), XVal::Num(f64::from(stage))];
        self.jip.handlers.iter().any(|h| h.event == "queststage" && h.filters.get(2).is_some_and(XVal::truthy) && self.jip_matches(h, None, &args))
    }

    /// A GetHit… value of `actor`'s current or last hit; `None` when it had none.
    pub(crate) fn hit_value(&self, actor: FormId, what: &str, k: i64) -> Option<XVal> {
        let h = self.jip.hits.get(&actor)?;
        Some(match what {
            "attacker" => XVal::Ref(h.attacker.map_or(0, |a| a.0)),
            "weapon" => XVal::Ref(h.weapon.map_or(0, |w| w.0)),
            "health" => XVal::Num(h.health_damage),
            "limb" => XVal::Num(h.limb_damage),
            "flag" => XVal::Num(f64::from(u8::from((0..16).contains(&k) && h.flags & (1 << k) != 0))),
            _ => return None,
        })
    }

    /// The main-loop callbacks due this frame (`Game::tick`).
    pub(crate) fn run_main_loop(&mut self) {
        if self.jip.main_loop.is_empty() {
            return;
        }
        let menu = self.menu.is_some() || self.dialogue.is_some();
        let want = if menu { 2 } else { 1 };
        let frame = self.frame;
        let due: Vec<MainLoop> = self.jip.main_loop.iter().filter(|m| m.due <= frame && m.flags & want != 0).cloned().collect();
        for m in due {
            for x in self.jip.main_loop.iter_mut().filter(|x| x.func == m.func && x.on == m.on) {
                x.due = frame + x.delay;
            }
            let on = m.on.unwrap_or(FormId::PLAYER_REF);
            let ctx = RunCtx { self_ref: Some(on), ..RunCtx::default() };
            if let Err(error) = self.run_function(&m.func, Some(Value::Ref(on.0)), Vec::new(), None, ctx) {
                self.emit(EventKind::ScriptError { unit: "JIP main-loop callback".into(), error });
            }
        }
    }
}
