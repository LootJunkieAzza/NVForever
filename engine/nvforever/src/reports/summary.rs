//! The session summary, `nvf-session/1` (player-reports.md §3.1; the server checks it
//! against LootJunkieAzza/NVForever-Data `docs/wire-format.md`). [`Summary`] holds what the
//! collector gathered (collect.rs); [`Summary::to_json`] writes the document. Capped at
//! [`MAX_BYTES`]: when it would be larger the least useful lists are cut first.

use serde_json::{json, Map, Value};

/// The schema name the server expects.
pub const SCHEMA: &str = "nvf-session/1";
/// The summary's size cap (§3.1, §5.1).
pub const MAX_BYTES: usize = 64 * 1024;
/// Lower bounds (ms) of the frame-time buckets (§3.1 `perf`); the last is open.
pub const BUCKETS_MS: [f32; 8] = [0.0, 8.0, 12.0, 16.7, 25.0, 33.0, 50.0, 100.0];
/// Cells kept, in first-visit order (§3.1 `play`).
pub const MAX_CELLS: usize = 200;
/// Warning groups kept (§3.1 `warnings`).
pub const MAX_WARNING_GROUPS: usize = 50;

/// How the session ended (§3.1 `end`), as the running game writes it. A summary still
/// saying `unknown` at the next start was cut short; that start makes it `panic` or
/// `native` from what crash.rs left (mod.rs `finish_earlier`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum End {
    Clean,
    /// Not known yet: the session is still running (the file is rewritten as it goes).
    #[default]
    Unknown,
}

impl End {
    pub fn as_str(self) -> &'static str {
        match self {
            End::Clean => "clean",
            End::Unknown => "unknown",
        }
    }
}

/// The bucket a frame time falls in.
pub fn bucket(ms: f32) -> usize {
    BUCKETS_MS.iter().rposition(|&lo| ms >= lo).unwrap_or(0)
}

/// Frame times in one place (a worldspace or an interior).
#[derive(Debug, Clone, Default)]
pub struct Place {
    pub name: String,
    pub interior: bool,
    pub histogram: [u64; 8],
    /// Frames over 100 ms.
    pub hitches: u64,
    /// The longest hitches: (seconds into the session, cell, ms), longest first, at most 10.
    pub top_hitches: Vec<(u64, String, f32)>,
}

impl Place {
    pub fn add_frame(&mut self, ms: f32, t_s: u64, cell: &str) {
        self.histogram[bucket(ms)] += 1;
        if ms > 100.0 {
            self.hitches += 1;
            if self.top_hitches.len() < 10 || self.top_hitches.last().is_some_and(|h| ms > h.2) {
                self.top_hitches.push((t_s, cell.to_string(), ms));
                self.top_hitches.sort_by(|a, b| b.2.total_cmp(&a.2));
                self.top_hitches.truncate(10);
            }
        }
    }

    /// A percentile (0..1) estimated from the histogram, interpolated inside a bucket.
    pub fn percentile(&self, p: f64) -> Option<f64> {
        let total: u64 = self.histogram.iter().sum();
        if total == 0 {
            return None;
        }
        let target = p * total as f64;
        let mut acc = 0.0;
        for (i, &n) in self.histogram.iter().enumerate() {
            let (lo, hi) = (f64::from(BUCKETS_MS[i]), BUCKETS_MS.get(i + 1).map_or(250.0, |&h| f64::from(h)));
            if n > 0 && acc + n as f64 >= target {
                return Some(((lo + (target - acc) / n as f64 * (hi - lo)) * 10.0).round() / 10.0);
            }
            acc += n as f64;
        }
        Some(250.0)
    }
}

/// Load times of one kind (new game, save, door, fast travel), in ms.
#[derive(Debug, Clone, Default)]
pub struct Loads {
    pub times_ms: Vec<f32>,
}

fn pct(sorted: &[f32], p: f32) -> Option<f32> {
    (!sorted.is_empty()).then(|| sorted[((sorted.len() - 1) as f32 * p).round() as usize])
}

/// Everything a summary says (§3.1). Strings are redacted by the writer.
#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub install_id: String,
    pub session_id: String,
    pub version: String,
    pub commit: Option<String>,
    pub profile: &'static str,
    pub dirty: Option<bool>,
    /// Session start, ISO 8601 UTC, rounded to the minute.
    pub start_utc: String,
    pub duration_s: u64,
    pub end: End,
    pub os: Option<String>,
    pub os_build: Option<String>,
    pub cpu: Option<String>,
    pub cores: Option<usize>,
    pub ram_gib: Option<u64>,
    pub gpu_name: Option<String>,
    pub gpu_vendor: Option<String>,
    pub gpu_driver: Option<String>,
    pub graphics_api: Option<String>,
    pub display: Option<(u32, u32, Option<u32>, String)>,
    /// INI settings that differ from the defaults, `Section.Key` = value; paths left out.
    pub settings: Vec<(String, String)>,
    /// Plugins in load order: (file name, size, CRC32 as hex).
    pub plugins: Vec<(String, Option<u64>, Option<String>)>,
    pub mo2: bool,
    /// Cells visited, first-visit order: (form ID as 8 hex digits, editor ID).
    pub cells: Vec<(String, Option<String>)>,
    pub worldspace_time_s: Vec<(String, u64)>,
    pub places: Vec<Place>,
    pub loads: Vec<(String, Loads)>,
    /// Warning and error lines grouped by their text with numbers stripped: (group, count, one example).
    pub warnings: Vec<(String, u64, String)>,
    /// Unimplemented script functions called: (name, count).
    pub script_stubs: Vec<(String, u64)>,
}

/// A log line's group: its text with numbers and form IDs stripped (§3.1 `warnings`).
pub fn warning_group(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() || (c == '0' && chars.peek() == Some(&'x')) {
            while chars.peek().is_some_and(|c| c.is_ascii_hexdigit() || *c == 'x' || *c == '.') {
                chars.next();
            }
            out.push('#');
        } else {
            out.push(c);
        }
    }
    out.chars().take(200).collect()
}

impl Summary {
    pub fn to_json(&self) -> Value {
        let mut v = json!({
            "schema": SCHEMA,
            "install_id": self.install_id,
            "session_id": self.session_id,
            "build": { "version": self.version, "profile": self.profile },
            "start_utc": self.start_utc,
            "duration_s": self.duration_s,
            "end": self.end.as_str(),
        });
        let o = v.as_object_mut().expect("an object");
        let b = o["build"].as_object_mut().expect("an object");
        if let Some(c) = &self.commit {
            b.insert("commit".into(), json!(c));
        }
        if let Some(d) = self.dirty {
            b.insert("dirty".into(), json!(d));
        }
        let mut sys = Map::new();
        let put = |m: &mut Map<String, Value>, k: &str, val: Option<Value>| {
            if let Some(val) = val {
                m.insert(k.into(), val);
            }
        };
        put(&mut sys, "os", self.os.clone().map(Value::from));
        put(&mut sys, "os_build", self.os_build.clone().map(Value::from));
        put(&mut sys, "cpu", self.cpu.clone().map(Value::from));
        put(&mut sys, "cores", self.cores.map(Value::from));
        put(&mut sys, "ram_gib", self.ram_gib.map(Value::from));
        put(&mut sys, "graphics_api", self.graphics_api.clone().map(Value::from));
        let mut gpu = Map::new();
        put(&mut gpu, "name", self.gpu_name.clone().map(Value::from));
        put(&mut gpu, "vendor", self.gpu_vendor.clone().map(Value::from));
        put(&mut gpu, "driver", self.gpu_driver.clone().map(Value::from));
        if !gpu.is_empty() {
            sys.insert("gpu".into(), Value::Object(gpu));
        }
        o.insert("system".into(), Value::Object(sys));
        if let Some((w, h, hz, mode)) = &self.display {
            o.insert("display".into(), json!({ "width": w, "height": h, "refresh_hz": hz, "window_mode": mode }));
        }
        o.insert("settings".into(), Value::Object(self.settings.iter().map(|(k, v)| (k.clone(), json!(v))).collect()));
        let plugins: Vec<Value> = self
            .plugins
            .iter()
            .map(|(n, size, crc)| {
                let mut p = Map::new();
                p.insert("name".into(), json!(n));
                put(&mut p, "size", size.map(Value::from));
                put(&mut p, "crc32", crc.clone().map(Value::from));
                Value::Object(p)
            })
            .collect();
        o.insert("load_order".into(), json!({ "plugins": plugins, "mo2": self.mo2 }));
        let cells: Vec<Value> = self
            .cells
            .iter()
            .take(MAX_CELLS)
            .map(|(id, ed)| match ed {
                Some(e) => json!({ "form_id": id, "editor_id": e }),
                None => json!({ "form_id": id }),
            })
            .collect();
        o.insert("play".into(), json!({ "cells": cells, "worldspace_time_s": self.worldspace_time_s.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<Map<_, _>>() }));
        let places: Vec<Value> = self
            .places
            .iter()
            .filter(|p| p.histogram.iter().sum::<u64>() > 0)
            .map(|p| {
                json!({
                    "place": p.name,
                    "kind": if p.interior { "interior" } else { "worldspace" },
                    "histogram": p.histogram,
                    "p50_ms": p.percentile(0.5), "p95_ms": p.percentile(0.95), "p99_ms": p.percentile(0.99),
                    "hitches": p.hitches,
                    "top_hitches": p.top_hitches.iter().map(|(t, c, ms)| json!({ "t_s": t, "cell": c, "ms": (ms * 10.0).round() / 10.0 })).collect::<Vec<_>>(),
                })
            })
            .collect();
        o.insert("perf".into(), json!({ "places": places }));
        let loads: Map<String, Value> = self
            .loads
            .iter()
            .filter(|(_, l)| !l.times_ms.is_empty())
            .map(|(k, l)| {
                let mut s = l.times_ms.clone();
                s.sort_by(f32::total_cmp);
                (k.clone(), json!({ "count": s.len(), "p50_ms": pct(&s, 0.5), "p95_ms": pct(&s, 0.95) }))
            })
            .collect();
        o.insert("loads".into(), Value::Object(loads));
        let mut warnings = self.warnings.clone();
        warnings.sort_by_key(|w| std::cmp::Reverse(w.1));
        warnings.truncate(MAX_WARNING_GROUPS);
        o.insert("warnings".into(), Value::Array(warnings.iter().map(|(g, n, e)| json!({ "group": g, "count": n, "example": e })).collect()));
        o.insert("script_stubs".into(), Value::Array(self.script_stubs.iter().map(|(n, c)| json!({ "name": n, "count": c })).collect()));
        v
    }

    /// The document as bytes, redacted and within [`MAX_BYTES`]: when too large, the
    /// warnings' examples, then the cells, then the plugins' sizes are dropped.
    pub fn to_bytes(&self, redact: impl Fn(&str) -> String) -> Vec<u8> {
        let mut v = self.to_json();
        let text = |v: &Value| redact(&v.to_string()).into_bytes();
        let mut out = text(&v);
        let cuts: [fn(&mut Value); 3] = [
            |v| {
                if let Some(w) = v["warnings"].as_array_mut() {
                    w.iter_mut().for_each(|w| _ = w.as_object_mut().map(|o| o.remove("example")));
                }
            },
            |v| v["play"]["cells"] = json!([]),
            |v| {
                if let Some(p) = v["load_order"]["plugins"].as_array_mut() {
                    p.iter_mut().for_each(|p| _ = p.as_object_mut().map(|o| (o.remove("size"), o.remove("crc32"))));
                }
            },
        ];
        for cut in cuts {
            if out.len() <= MAX_BYTES {
                break;
            }
            cut(&mut v);
            out = text(&v);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Summary {
        let mut place = Place { name: "WastelandNV".into(), ..Default::default() };
        for _ in 0..90 {
            place.add_frame(14.0, 1, "x");
        }
        for _ in 0..10 {
            place.add_frame(30.0, 2, "x");
        }
        place.add_frame(340.0, 812, "FreesideNorthGate");
        Summary {
            install_id: "3f2b8c1e-7a4d-4e2b-9c1a-0d5e6f7a8b9c".into(),
            session_id: "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d".into(),
            version: "0.6.0".into(),
            profile: "release",
            start_utc: "2026-11-10T08:00:00Z".into(),
            duration_s: 3600,
            end: End::Clean,
            plugins: vec![("FalloutNV.esm".into(), Some(1), Some("DEADBEEF".into()))],
            places: vec![place],
            loads: vec![("door".into(), Loads { times_ms: vec![800.0, 900.0, 2000.0] })],
            ..Default::default()
        }
    }

    #[test]
    fn frame_times_fall_in_the_spec_buckets() {
        assert_eq!(bucket(0.0), 0);
        assert_eq!(bucket(7.9), 0);
        assert_eq!(bucket(8.0), 1);
        assert_eq!(bucket(16.6), 2);
        assert_eq!(bucket(16.7), 3);
        assert_eq!(bucket(99.0), 6);
        assert_eq!(bucket(5000.0), 7);
    }

    #[test]
    fn the_document_has_what_the_server_requires() {
        let v = sample().to_json();
        assert_eq!(v["schema"], "nvf-session/1");
        assert_eq!(v["end"], "clean");
        assert_eq!(v["build"]["version"], "0.6.0");
        let p = &v["perf"]["places"][0];
        assert_eq!(p["histogram"].as_array().unwrap().len(), 8);
        assert_eq!(p["hitches"], 1);
        assert_eq!(p["top_hitches"][0]["cell"], "FreesideNorthGate");
        assert!(p["p50_ms"].as_f64().unwrap() > 12.0 && p["p50_ms"].as_f64().unwrap() < 16.7);
        assert_eq!(v["loads"]["door"]["count"], 3);
        assert_eq!(v["loads"]["door"]["p50_ms"], 900.0);
    }

    #[test]
    fn redaction_and_the_size_cap_apply_to_the_bytes() {
        let mut s = sample();
        s.warnings = (0..50).map(|i| (format!("group {i}"), 1, format!(r"C:\Users\Alex\{}", "x".repeat(2000)))).collect();
        let bytes = s.to_bytes(|t| t.replace(r"C:\\Users\\Alex", "%USERPROFILE%"));
        assert!(bytes.len() <= MAX_BYTES, "{}", bytes.len());
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(v["warnings"][0].get("example").is_none(), "examples cut first");
        assert_eq!(v["warnings"][0]["group"], "group 0");
    }

    #[test]
    fn warning_groups_strip_numbers() {
        assert_eq!(warning_group("missing texture 12 for 0x00ABCDEF at 1.5"), "missing texture # for # at #");
    }
}
