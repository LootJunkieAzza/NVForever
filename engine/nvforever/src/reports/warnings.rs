//! Warning and error lines, counted for the session summary (player-reports.md §3.1
//! `warnings`): grouped by their text with numbers stripped (summary::warning_group), each
//! group with its count and one example line. A tracing layer beside the run log's
//! (crash.rs `file_layer`); at most [`MAX_GROUPS`] groups, later new ones only counted.

use bevy::log::tracing;
use bevy::log::tracing_subscriber::{layer::Context, Layer};
use std::collections::HashMap;
use std::sync::Mutex;

const MAX_GROUPS: usize = 500;

#[derive(Default)]
struct Groups {
    map: HashMap<String, (u64, String)>,
    dropped: u64,
}

static GROUPS: Mutex<Option<Groups>> = Mutex::new(None);

fn record(line: &str) {
    let group = super::summary::warning_group(line);
    let mut g = GROUPS.lock().unwrap_or_else(|e| e.into_inner());
    let g = g.get_or_insert_with(Groups::default);
    if let Some(e) = g.map.get_mut(&group) {
        e.0 += 1;
    } else if g.map.len() < MAX_GROUPS {
        g.map.insert(group, (1, line.chars().take(300).collect()));
    } else {
        g.dropped += 1;
    }
}

/// The groups so far: (group, count, example), the commonest first (the summary keeps 50).
pub fn groups() -> Vec<(String, u64, String)> {
    let g = GROUPS.lock().unwrap_or_else(|e| e.into_inner());
    let Some(g) = g.as_ref() else { return Vec::new() };
    let mut v: Vec<_> = g.map.iter().map(|(k, (n, e))| (k.clone(), *n, e.clone())).collect();
    if g.dropped > 0 {
        v.push(("(other warnings)".to_string(), g.dropped, String::new()));
    }
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

struct Message(String);

impl tracing::field::Visit for Message {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.0 = value.to_string();
        }
    }
}

struct WarnCounter;

impl<S: tracing::Subscriber> Layer<S> for WarnCounter {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let level = *event.metadata().level();
        // TRACE (`[Log] …=trace`) is too many lines to format for the recorder each; the run
        // log still has them (§4.2 keeps debug and up).
        if level == tracing::Level::TRACE {
            return;
        }
        let mut m = Message(String::new());
        event.record(&mut m);
        // Every event: the flight recorder and the anomaly detectors (anomaly.rs).
        super::anomaly::on_log(level.as_str(), event.metadata().target(), &m.0);
        if level != tracing::Level::WARN && level != tracing::Level::ERROR {
            return;
        }
        record(&format!("{} {}: {}", level, event.metadata().target(), m.0));
    }
}

/// The layer, for the log plugin (crash.rs `file_layer`), only when reports are on.
pub fn layer() -> Option<bevy::log::BoxedLayer> {
    super::active().is_some().then(|| Box::new(WarnCounter) as bevy::log::BoxedLayer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_that_differ_in_numbers_are_one_group() {
        record("WARN x: missing texture 12");
        record("WARN x: missing texture 345");
        let g = groups();
        let missing = g.iter().find(|g| g.0.starts_with("WARN x: missing texture")).unwrap();
        assert_eq!(missing.1, 2);
        assert_eq!(missing.2, "WARN x: missing texture 12");
    }
}
