//! Bounded, opt-in log-only diagnostics. No game files or directories are scanned.
//! Format coverage is adapted from h9x000 / Claude Fable 5.2's GPL-3.0-or-later
//! 7c1bfa19f9efa3a6f600b5337026dadceb425ce2; this parser streams, separates sessions
//! and withholds all message/path text unless the user explicitly requests detail.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::BufRead;

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_LINE: usize = 64 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_RETAINED: usize = 8 * 1024 * 1024;
const MAX_SESSIONS: usize = 128;

/// Only strip complete ANSI CSI sequences. An unmatched escape must not swallow the
/// rest of a diagnostic (older redirected logs may have a truncated final write).
fn plain(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            let mut sequence = String::from("\u{1b}");
            sequence.push(chars.next().unwrap());
            let mut complete = false;
            for next in chars.by_ref() {
                sequence.push(next);
                if ('\u{40}'..='\u{7e}').contains(&next) {
                    complete = true;
                    break;
                }
            }
            if !complete {
                out.push_str(&sequence);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn parts(line: &str) -> Option<(&str, &str)> {
    let (time, rest) = line.split_once(' ')?;
    if time.len() < 20 || !time.as_bytes()[0].is_ascii_digit() || !time.contains('T') {
        return None;
    }
    let (level, rest) = rest.trim_start().split_once(' ')?;
    if !matches!(level, "INFO" | "WARN" | "ERROR" | "DEBUG" | "TRACE") {
        return None;
    }
    let (_, words) = rest.trim_start().split_once(": ")?;
    Some((level, words))
}

#[derive(Debug, Default)]
pub struct Session {
    aid_seen: bool,
    load_order_seen: bool,
    mod_log: bool,
    exited: bool,
    zero_exit: bool,
    entries: BTreeMap<(&'static str, String), usize>,
    /// Replace a repeated plugin exit row instead of summing the same total twice.
    exit_counts: BTreeMap<String, [u64; 4]>,
}

#[derive(Debug)]
pub struct Digest {
    pub lines: usize,
    pub invalid_utf8_lines: usize,
    pub sessions: Vec<Session>,
}

/// Counts and comparisons describe log records, not proof that a mod ran successfully.
/// A missing exit row is explicit; absence of failure text never becomes a PASS.
impl Digest {
    pub fn read(mut reader: impl BufRead) -> Result<Self, String> {
        use std::io::Read;
        let mut out = Self { lines: 0, invalid_utf8_lines: 0, sessions: vec![Session::default()] };
        let mut bytes = 0;
        let mut entries = 0;
        let mut retained = 0;
        let mut raw = Vec::new();
        loop {
            raw.clear();
            // Take limits even a single unterminated line before allocation can grow.
            let n = Read::by_ref(&mut reader).take((MAX_LINE + 1) as u64).read_until(b'\n', &mut raw).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            bytes += n;
            if n > MAX_LINE || bytes > MAX_BYTES {
                return Err("mod-log input limit exceeded; no partial digest".into());
            }
            out.lines += 1;
            if std::str::from_utf8(&raw).is_err() {
                out.invalid_utf8_lines += 1;
            }
            let line = plain(String::from_utf8_lossy(&raw).trim_end_matches(['\r', '\n']));
            let parsed = parts(&line);
            let aid_start = parsed.is_some_and(|(_, words)| words.starts_with("mods: test aid "));
            let order_start = parsed.is_some_and(|(_, words)| words.starts_with("mods: load order,"));
            let current = out.sessions.last().unwrap();
            // The real logger writes its aid prelude before the load-order row. Those
            // two lines belong to one launch, not two artificial sessions.
            if (aid_start && (current.aid_seen || current.load_order_seen)) || (order_start && current.load_order_seen) {
                if out.sessions.len() == MAX_SESSIONS {
                    return Err("mod-log session limit exceeded; no partial digest".into());
                }
                out.sessions.push(Session::default());
            }
            let session = out.sessions.last_mut().unwrap();
            session.aid_seen |= aid_start;
            session.load_order_seen |= order_start;
            let (kind, words) = match parsed {
                Some((_, words)) if words.starts_with("mods: ") => {
                    session.mod_log = true;
                    let words = &words[6..];
                    if let Some(exit) = words.strip_prefix("at exit: ") {
                        session.exited = true;
                        session.zero_exit |= exit == "no script error, failed call, skipped statement or function not built in the mods' scripts";
                        if let Some((plugin, counts)) = exit.rsplit_once(": ") {
                            let values: Vec<Option<u64>> = counts.split(", ").map(|s| s.split(' ').next().and_then(|n| n.parse().ok())).collect();
                            if let [Some(a), Some(b), Some(c), Some(d)] = values.as_slice() {
                                if !session.exit_counts.contains_key(plugin) {
                                    retained += plugin.len();
                                    entries += 1;
                                }
                                session.exit_counts.insert(plugin.into(), [*a, *b, *c, *d]);
                            }
                        }
                        ("exit", words)
                    } else if words.starts_with("load order,") {
                        ("load_order", words)
                    } else if words.contains(": does not compile:") {
                        ("compile_error", words)
                    } else if words.contains(": runs without ") {
                        ("compile_lines_omitted", words)
                    } else if words.contains("functions NVForever does not have") || words.contains("is not built:") {
                        ("unbuilt", words)
                    } else if words.contains(": skipped:") || words.contains(": skipped at:") || words.contains(" failed:") || words.contains(": stopped early:") {
                        ("script_failure", words)
                    } else if words.contains("not there (first in ") {
                        ("dependency_absent", words)
                    } else if words.starts_with("HUD") {
                        ("hud", words)
                    } else if words.starts_with("MCM") {
                        ("mcm", words)
                    } else {
                        ("mod_note", words)
                    }
                }
                Some(("ERROR", words)) => ("error", words),
                Some((_, words)) if words.starts_with("script runner: ") && words.contains(" skipped (") => ("runner_skipped", words),
                Some((_, words)) if words.starts_with("message box shown: ") => ("message_box", words),
                Some((_, words)) if words.starts_with("movement: ") => ("movement", words),
                Some(("WARN", words)) if words.starts_with("ui: ") => ("ui_warning", words),
                None if line.contains("panicked at") => ("panic", line.as_str()),
                _ => continue,
            };
            let key = (kind, words.to_string());
            if !session.entries.contains_key(&key) {
                entries += 1;
                retained += words.len();
            }
            if entries > MAX_ENTRIES || retained > MAX_RETAINED {
                return Err("mod-log retention limit exceeded; no partial digest".into());
            }
            *session.entries.entry(key).or_default() += 1;
        }
        Ok(out)
    }

    pub fn json(&self, detail: bool) -> Value {
        json!({"lines": self.lines, "invalid_utf8_lines": self.invalid_utf8_lines,
            "sessions": self.sessions.iter().map(|s| s.json(detail)).collect::<Vec<_>>()})
    }

    /// Compare the final session of each explicit input, never combine counts from
    /// several appended launches. The output says which session indexes were chosen.
    pub fn changes(&self, before: &Self, detail: bool) -> Value {
        let now = self.sessions.last().unwrap();
        let old = before.sessions.last().unwrap();
        let counts = |s: &Session| -> BTreeMap<&str, usize> {
            let mut out = BTreeMap::new();
            for ((kind, _), n) in &s.entries {
                *out.entry(*kind).or_default() += n;
            }
            out
        };
        let a = counts(now);
        let b = counts(old);
        let kinds: std::collections::BTreeSet<&str> = a.keys().chain(b.keys()).copied().collect();
        let mut doc = json!({"session": self.sessions.len() - 1, "previous_session": before.sessions.len() - 1,
            "counts": kinds.into_iter().filter_map(|kind| {
                let n = a.get(kind).copied().unwrap_or(0);
                let p = b.get(kind).copied().unwrap_or(0);
                (n != p).then(|| json!({"kind": kind, "previous_log_records": p, "log_records": n}))
            }).collect::<Vec<_>>()});
        if detail {
            let difference = |a: &Session, b: &Session| a.entries.iter().filter(|(k, _)| !b.entries.contains_key(*k)).map(|((kind, words), count)| json!({"kind": kind, "words": words, "log_records": count})).collect::<Vec<_>>();
            doc["new"] = json!(difference(now, old));
            doc["gone"] = json!(difference(old, now));
        }
        doc
    }
}

impl Session {
    fn json(&self, detail: bool) -> Value {
        let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
        for ((kind, _), n) in &self.entries {
            *kinds.entry(*kind).or_default() += n;
        }
        let mut total = [0u64; 4];
        let mut overflow = false;
        for counts in self.exit_counts.values() {
            for i in 0..4 {
                match total[i].checked_add(counts[i]) {
                    Some(next) => total[i] = next,
                    None => overflow = true,
                }
            }
        }
        let totals = if overflow || (self.exit_counts.is_empty() && !self.zero_exit) {
            Value::Null
        } else {
            json!({"script_errors": total[0], "failed_calls": total[1], "skipped_statements": total[2], "unbuilt": total[3]})
        };
        let mut doc = json!({"mod_log": self.mod_log, "load_order_row_present": self.load_order_seen, "exit_rows_present": self.exited, "log_records": kinds,
            "reported_exit_counts": totals, "exit_count_overflow": overflow,
            "scope": "Log evidence only; exit rows can be missing or incomplete. No compatibility verdict."});
        if detail {
            doc["entries"] = json!(self.entries.iter().map(|((kind, words), n)| json!({"kind": kind, "words": words, "log_records": n})).collect::<Vec<_>>());
        }
        doc
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn read(text: &str) -> Digest { Digest::read(Cursor::new(text)).unwrap() }
    fn line(words: &str) -> String { format!("2026-11-15T00:00:00.000000Z INFO nvf_play::mod_log: mods: {words}\n") }

    #[test]
    fn ansi_crlf_and_exit_totals_are_not_failure_record_counts() {
        let text = line("load order, 1 plugin (1 mod): private.esp")
            + &line("private.esp: SCPT 1: skipped: private script words")
            + &line("at exit: private.esp: 0 script errors, 2 failed calls, 73 skipped statements, 0 functions not built");
        let d = read(&text.replace("INFO", "\u{1b}[32mINFO\u{1b}[0m").replace('\n', "\r\n"));
        let safe = d.json(false);
        assert!(!safe.to_string().contains("private"));
        assert_eq!(safe["sessions"][0]["reported_exit_counts"]["skipped_statements"], 73);
        assert_eq!(safe["sessions"][0]["log_records"]["script_failure"], 1);
        assert!(d.json(true).to_string().contains("private script words"));
        assert_eq!(plain("x\u{1b}[31mred\u{1b}[0m tail\u{1b}["), "xred tail\u{1b}[");
    }

    #[test]
    fn appended_sessions_and_missing_exit_are_kept_separate() {
        let d = read(&(line("test aid NVF_PLAY_TEST_MOD_LOG: authored") + &line("load order, old") + &line("at exit: no script error")
            + &line("test aid NVF_PLAY_TEST_MOD_LOG: authored") + &line("load order, new") + &line("new: skipped: first")));
        let j = d.json(false);
        assert_eq!(d.sessions.len(), 2);
        assert_eq!(j["sessions"][0]["exit_rows_present"], true);
        assert_eq!(j["sessions"][1]["exit_rows_present"], false);
        assert!(j["sessions"][1]["reported_exit_counts"].is_null());
        let changes = d.changes(&read(&line("load order, previous")), true);
        assert_eq!(changes["session"], 1);
        assert!(changes["new"].as_array().unwrap().iter().any(|r| r["kind"] == "script_failure"));
    }

    #[test]
    fn exact_line_entry_and_session_limits_fail_closed() {
        assert!(Digest::read(Cursor::new(vec![b'x'; MAX_LINE])).is_ok());
        assert!(Digest::read(Cursor::new(vec![b'x'; MAX_LINE + 1])).unwrap_err().contains("input limit"));
        let entries = (0..MAX_ENTRIES).map(|i| line(&format!("authored {i}"))).collect::<String>();
        assert!(Digest::read(Cursor::new(entries.as_bytes())).is_ok());
        assert!(Digest::read(Cursor::new(entries + &line("one more"))).unwrap_err().contains("retention limit"));
        let sessions = line("load order, same").repeat(MAX_SESSIONS);
        assert!(Digest::read(Cursor::new(sessions.as_bytes())).is_ok());
        assert!(Digest::read(Cursor::new(sessions + &line("load order, extra"))).unwrap_err().contains("session limit"));
    }

    #[test]
    fn unknown_exit_totals_and_overflow_are_not_zero_success() {
        let missing = read(&line("load order, authored"));
        assert!(missing.json(false)["sessions"][0]["reported_exit_counts"].is_null());
        let zero = read(&line("at exit: no script error, failed call, skipped statement or function not built in the mods' scripts"));
        assert_eq!(zero.json(false)["sessions"][0]["reported_exit_counts"]["failed_calls"], 0);
        let overflow = read(&(line(&format!("at exit: A: 0 script errors, {} failed calls, 0 skipped statements, 0 functions not built", u64::MAX))
            + &line("at exit: B: 0 script errors, 1 failed calls, 0 skipped statements, 0 functions not built")));
        assert_eq!(overflow.json(false)["sessions"][0]["exit_count_overflow"], true);
        assert!(overflow.json(false)["sessions"][0]["reported_exit_counts"].is_null());
    }
}
