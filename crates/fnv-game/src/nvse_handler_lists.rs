//! xNVSE's event-handler lists (docs/handoff/xnvse-remaining.md §3): GetEventHandlers,
//! DumpEventHandlers, GetHigher/LowerPriorityEventHandlers, IsEventHandlerFirst/Last.
//! They read the handlers SetEventHandler and SetEventHandlerAlt registered (not JIP's
//! SetOn…EventHandler ones) [documented: GetEventHandlers].
//!
//! A handler array is [handler, StringMap of its filters]. Handlers by priority are a Map
//! from priority to the list of handler arrays; without an event filter the result is a
//! StringMap from event name to that Map; an unknown event gives ar_Null [documented:
//! GetEventHandlers; observed: nvse-sixth, events.all.type StringMap, events.filtered.type
//! Map, events.unknown.size -1].

use crate::game::{Game, Handler};
use fnv_script::vm::{ArrayKind, XKey, XVal};

/// The events xNVSE knows without a script registering them: the built-in events and
/// the session events (docs/spec-notes/ext-events.md §2.1–2.2), lower case. Another name
/// is a user-defined event, which exists once something listens for it: with no handler
/// it gives ar_Null [observed: nvse-sixth, OnActorEquip 0 entries, an unknown name -1].
const KNOWN: [&str; 49] = [
    "onactivate", "onactorequip", "onequip", "onactorunequip", "onunequip", "onadd", "onclose", "onopen",
    "oncombatend", "ondeath", "ondrop", "onhit", "onhitwith", "onload", "onmagiceffecthit", "onmurder",
    "onpackagestart", "onpackagechange", "onpackagedone", "onreset", "onsell", "onstartcombat", "ontrigger",
    "ontriggerenter", "ontriggerleave", "onapplyimod", "onremoveimod", "onlockbroken", "onlockpicksuccess",
    "onlockpickbroken", "onterminalhacked", "onterminalhackfailed", "onrepair", "ondisable", "onenable",
    "loadgame", "postloadgame", "savegame", "newgame", "exitgame", "exittomainmenu", "qqq", "runtimescripterror",
    "deletegame", "renamegame", "renamenewgame", "deletegamename", "renamegamename", "renamenewgamename",
];

/// The StringMap of a handler's filters: "first"/"second" for SetEventHandler, the
/// argument index as text for SetEventHandlerAlt [hypothesis: the keys].
fn filters(g: &mut Game, h: &Handler) -> XVal {
    let entries = h
        .filters
        .iter()
        .map(|(i, v)| {
            let key = if h.alt {
                i.to_string()
            } else if *i == 1 {
                "first".into()
            } else {
                "second".into()
            };
            (XKey::Str(key), v.clone())
        })
        .collect();
    XVal::Array(g.ext.new_array(ArrayKind::StringMap, entries))
}

fn handler_array(g: &mut Game, h: &Handler) -> XVal {
    let f = filters(g, h);
    XVal::Array(g.ext.new_array(ArrayKind::List, vec![(XKey::Num(0.0), h.func.clone()), (XKey::Num(1.0), f)]))
}

/// The list of handler arrays of `hs`, in the order they run (higher priority first,
/// then registration order).
fn list(g: &mut Game, hs: &[Handler]) -> XVal {
    let mut entries = Vec::new();
    for (k, h) in hs.iter().enumerate() {
        let a = handler_array(g, h);
        entries.push((XKey::Num(k as f64), a));
    }
    XVal::Array(g.ext.new_array(ArrayKind::List, entries))
}

/// A Map from priority to the list of handler arrays at that priority.
fn by_priority(g: &mut Game, hs: &[Handler]) -> XVal {
    let mut prios: Vec<i32> = hs.iter().map(|h| h.priority).collect();
    prios.sort_unstable();
    prios.dedup();
    let mut entries = Vec::new();
    for p in prios {
        let at: Vec<Handler> = hs.iter().filter(|h| h.priority == p).cloned().collect();
        let l = list(g, &at);
        entries.push((XKey::Num(f64::from(p)), l));
    }
    XVal::Array(g.ext.new_array(ArrayKind::Map, entries))
}

/// The registered handlers, in running order.
fn sorted(g: &Game) -> Vec<Handler> {
    let mut hs = g.handlers.clone();
    hs.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.seq.cmp(&b.seq)));
    hs
}

fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

/// The handlers matching GetEventHandlers' filters: event name (any case; "" for all),
/// handler (0 for any), priority (0 for any).
fn matching(g: &Game, args: &[XVal]) -> Vec<Handler> {
    let event = text(args, 0).to_ascii_lowercase();
    let func = args.get(1).filter(|v| !matches!(v, XVal::Num(x) if *x == 0.0) && !matches!(v, XVal::Ref(0)));
    let prio = match args.get(2) {
        Some(XVal::Num(x)) => *x as i32,
        _ => 0,
    };
    sorted(g)
        .into_iter()
        .filter(|h| event.is_empty() || h.event == event)
        .filter(|h| func.is_none_or(|f| &h.func == f))
        .filter(|h| prio == 0 || h.priority == prio)
        .collect()
}

/// The handler `args[1]` of event `args[0]` at priority `args[2]`, and the others of that
/// event (`args[3..]`, the scripts and plugins to ignore, are not kept apart here).
fn around(g: &Game, args: &[XVal]) -> Option<(Handler, Vec<Handler>)> {
    let event = text(args, 0).to_ascii_lowercase();
    let func = args.get(1)?;
    let prio = match args.get(2) {
        Some(XVal::Num(x)) => *x as i32,
        _ => return None,
    };
    let hs: Vec<Handler> = sorted(g).into_iter().filter(|h| h.event == event).collect();
    let me = hs.iter().find(|h| &h.func == func && h.priority == prio)?.clone();
    let others = hs.into_iter().filter(|h| h.seq != me.seq).collect();
    Some((me, others))
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, args: &[XVal]) -> Option<Result<XVal, String>> {
    let v = match name {
        "geteventhandlers" => {
            let hs = matching(g, args);
            let event = text(args, 0);
            let prio_filtered = matches!(args.get(2), Some(XVal::Num(x)) if *x != 0.0);
            if !event.is_empty() {
                let lower = event.to_ascii_lowercase();
                if hs.is_empty() && !KNOWN.contains(&lower.as_str()) && !g.handlers.iter().any(|h| h.event == lower) {
                    XVal::Array(0)
                } else if prio_filtered {
                    list(g, &hs)
                } else {
                    by_priority(g, &hs)
                }
            } else {
                // Event names as the scripts wrote them, first spelling kept.
                let mut names: Vec<(String, String)> = Vec::new();
                for h in &hs {
                    if !names.iter().any(|(e, _)| *e == h.event) {
                        names.push((h.event.clone(), h.name.clone()));
                    }
                }
                let mut entries = Vec::new();
                for (e, shown) in names {
                    let of: Vec<Handler> = hs.iter().filter(|h| h.event == e).cloned().collect();
                    let v = if prio_filtered { list(g, &of) } else { by_priority(g, &of) };
                    entries.push((XKey::Str(shown), v));
                }
                XVal::Array(g.ext.new_array(ArrayKind::StringMap, entries))
            }
        }
        // One console line per handler, by event and priority [documented:
        // DumpEventHandlers; the line's wording is NVForever's own].
        "dumpeventhandlers" => {
            let hs = matching(g, args);
            for h in hs {
                g.console_out.push(format!("{} (priority {}): {:?}", h.name, h.priority, h.func));
            }
            XVal::Num(0.0)
        }
        // The others at a higher-or-equal (lower-or-equal) priority, as a priority Map;
        // ar_Null when the handler is not at that priority [documented].
        "gethigherpriorityeventhandlers" | "getlowerpriorityeventhandlers" => match around(g, args) {
            Some((me, others)) => {
                let higher = name.starts_with("gethigher");
                let pick: Vec<Handler> =
                    others.into_iter().filter(|h| if higher { h.priority >= me.priority } else { h.priority <= me.priority }).collect();
                by_priority(g, &pick)
            }
            None => XVal::Array(0),
        },
        // First: none at a lower-or-equal priority; last: none at a higher-or-equal one;
        // 0 when the handler is not at that priority [documented: IsEventHandlerFirst;
        // IsEventHandlerLast, as the pages word them].
        "iseventhandlerfirst" | "iseventhandlerlast" => match around(g, args) {
            Some((me, others)) => {
                let first = name == "iseventhandlerfirst";
                let none = others.iter().all(|h| if first { h.priority > me.priority } else { h.priority < me.priority });
                XVal::Num(f64::from(u8::from(none)))
            }
            None => XVal::Num(0.0),
        },
        _ => return None,
    };
    Some(Ok(v))
}
