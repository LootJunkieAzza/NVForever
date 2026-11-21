//! Bounded summaries of actual headless trace events, not inferred script success.
//! Inspired by h9x000 / Claude Fable 5.2, ffc6b6fb62484b1089f08dddf328681b61aab165.

use fnv_game::trace::{Event, EventKind};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MAX_ENTRIES: usize = 4096;
const MAX_WORD_BYTES: usize = 8 * 1024 * 1024;

/// Borrow event payloads while deciding whether a distinct row fits. Formatting a
/// rejected payload would allocate before admission; default redacted output needs
/// no copies of the words at all.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Words<'a> {
    Text(&'a str),
    Call { name: &'a str, error: &'a str },
    Unread { name: &'a str, param: usize, got: &'a str, wanted: &'a str },
}

impl Words<'_> {
    fn byte_len(self) -> Option<usize> {
        match self {
            Self::Text(text) => Some(text.len()),
            Self::Call { name, error } => name.len().checked_add(": ".len())?.checked_add(error.len()),
            Self::Unread { name, param, got, wanted } => {
                let digits = if param == 0 { 1 } else { param.ilog10() as usize + 1 };
                [name.len(), " parameter ".len(), digits, ": ".len(), got.len(), ", wanted ".len(), wanted.len()]
                    .iter().try_fold(0usize, |sum, n| sum.checked_add(*n))
            }
        }
    }

    fn render(self) -> String {
        match self {
            Self::Text(text) => text.to_string(),
            Self::Call { name, error } => format!("{name}: {error}"),
            Self::Unread { name, param, got, wanted } => format!("{name} parameter {param}: {got}, wanted {wanted}"),
        }
    }
}

pub fn summarize(events: &[Event], detail: bool) -> Result<Value, String> {
    let mut rows: BTreeMap<(&str, &str, Words<'_>), usize> = BTreeMap::new();
    let mut retained = 0usize;
    for e in events {
        let (kind, unit, words) = match &e.kind {
            EventKind::CallFailed { name, unit, error, .. } => ("call_failed", unit, Words::Call { name, error }),
            EventKind::ScriptError { unit, error } => ("script_error", unit, Words::Text(error)),
            EventKind::Skipped { unit, what } => ("skipped", unit, Words::Text(what)),
            EventKind::Unhandled { name, unit, .. } => ("unhandled", unit, Words::Text(name)),
            EventKind::ArgUnread { name, unit, param, wanted, got, .. } => ("argument_unread", unit, Words::Unread { name, param: *param, got, wanted }),
            EventKind::ScriptPrint { unit, text } => ("printed", unit, Words::Text(text)),
            _ => continue,
        };
        let Some(cost) = words.byte_len().and_then(|n| n.checked_add(unit.len())).filter(|n| *n <= MAX_WORD_BYTES) else {
            return Err("runner diagnostic retention limit exceeded; no partial summary".into());
        };
        let key = (kind, unit.as_str(), words);
        if let Some(count) = rows.get_mut(&key) {
            *count += 1;
            continue;
        }
        let next = retained.checked_add(cost);
        let Some(next) = next.filter(|n| *n <= MAX_WORD_BYTES) else {
            return Err("runner diagnostic retention limit exceeded; no partial summary".into());
        };
        if rows.len() == MAX_ENTRIES {
            return Err("runner diagnostic retention limit exceeded; no partial summary".into());
        }
        retained = next;
        rows.insert(key, 1);
    }
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let entries: Vec<Value> = rows.into_iter().enumerate().map(|(id, ((kind, unit, words), count))| {
        *counts.entry(kind).or_default() += count;
        let mut row = json!({"id": id, "kind": kind, "trace_events": count});
        if detail {
            row["unit"] = json!(unit);
            row["words"] = json!(words.render());
        }
        row
    }).collect();
    Ok(json!({"schema": "fnv.ext.runner_diagnostics/1", "detail": detail, "counts": counts, "entries": entries,
        "scope": "Recorded trace events only; some runtime diagnostics are deduplicated before tracing. No rendered/native/mod acceptance."}))
}

/// Optional diagnostics must not turn a previously successful Script Runner command
/// into an error or discard its existing compile/run rows. No partial counts escape.
pub fn document(events: &[Event], detail: bool) -> Value {
    match summarize(events, detail) {
        Ok(mut doc) => { doc["available"] = json!(true); doc }
        Err(_) => json!({"schema": "fnv.ext.runner_diagnostics/1", "detail": detail,
            "available": false, "reason": "retention_limit", "scope": "Diagnostics unavailable; existing command results are unchanged. No partial counts."}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_words_and_units_require_detail_and_counts_are_events() {
        let event = Event { frame: 0, kind: EventKind::CallFailed { opcode: "x".into(), name: "F".into(), unit: "private/unit".into(), error: "private words".into() } };
        let events = [event.clone(), event];
        let safe = summarize(&events, false).unwrap();
        assert_eq!(safe["counts"]["call_failed"], 2);
        assert!(!safe.to_string().contains("private"));
        let raw = summarize(&events, true).unwrap();
        assert_eq!(raw["entries"][0]["unit"], "private/unit");
        assert_eq!(raw["entries"][0]["words"], "F: private words");
    }

    #[test]
    fn retention_boundary_refuses_instead_of_hiding_new_failures() {
        let events: Vec<Event> = (0..=MAX_ENTRIES).map(|i| Event { frame: 0, kind: EventKind::ScriptPrint { unit: "authored".into(), text: i.to_string() } }).collect();
        assert!(summarize(&events[..MAX_ENTRIES], false).is_ok());
        assert!(summarize(&events, false).unwrap_err().contains("retention limit"));
    }

    #[test]
    fn retained_word_bytes_are_capped_even_for_one_entry() {
        let mut event = Event { frame: 0, kind: EventKind::ScriptPrint { unit: String::new(), text: "x".repeat(MAX_WORD_BYTES) } };
        assert!(summarize(std::slice::from_ref(&event), false).is_ok());
        if let EventKind::ScriptPrint { text, .. } = &mut event.kind { text.push('x'); }
        assert!(summarize(&[event], false).unwrap_err().contains("retention limit"));
    }

    #[test]
    fn unavailable_diagnostics_are_explicit_and_never_partial() {
        let event = Event { frame: 0, kind: EventKind::CallFailed {
            opcode: "authored".into(), name: "F".into(), unit: "u".into(), error: "x".repeat(MAX_WORD_BYTES),
        } };
        let doc = document(&[event], true);
        assert_eq!(doc["available"], false);
        assert_eq!(doc["reason"], "retention_limit");
        assert!(doc.get("counts").is_none());
        assert!(doc.get("entries").is_none());
        assert_eq!(document(&[], false)["available"], true);
    }
}
