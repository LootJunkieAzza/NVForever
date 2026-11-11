//! Test aid `NVF_PLAY_TEST_QUERIES=<job.steps.json>`: the extender lab's twin on the
//! running engine (#1024). The job's runtime-compiled queries (fnv.capture.steps/1, as
//! tools/capture/jip_probes.py writes them) run inside `--play`, one per frame in order,
//! as nvf_capture runs one step per game loop [documented:
//! docs/spec-notes/xnvse-capture.md §4], each through `Game::run_query` on no reference
//! before the frame's script tick, where the console's lines run. Unlike the headless
//! `fnvtool ext queries`, the answers come from the game with its renderer, physics,
//! navmeshes, animation and ray casts: hits, projectiles, NIF blocks, kNVSE animation and
//! the order events fire in.
//!
//! - `NVF_PLAY_TEST_QUERIES_OUT=<file>`: where fnv.ext.probes/1 goes (the same document as
//!   `fnvtool ext queries`, `fnv_game::ext_probes`); never inside the game folder
//!   (`fnv_install::guard_write`). Default: `<set>.probes.json` in the working folder.
//! - `NVF_PLAY_TEST_QUERIES_AT=<seconds>`: when the first step runs on the world aids'
//!   clock (`crate::pause::AidTime::world_secs`, from the scripts' first run with
//!   `NVF_PLAY_AIDS_FROM_SCRIPTS=1`); default 3, so the start cell has settled.
//!
//! The run quits once the document is written. Log lines: `query job: <set>: <n> steps
//! from <file>`, `query job: <set>: <n> steps, <r> rows, <e> errors, written to <file>`,
//! and at exit `query job: <set>: stopped after <k> of <n> steps` when time ran out.

use bevy::prelude::*;
use fnv_game::ext_probes::{probes_doc, run_step, QueryStep};
use fnv_game::Game;
use serde_json::{json, Value as Json};
use std::path::PathBuf;

/// Seconds on the world aids' clock before the first step when `_AT` is not given.
const DEFAULT_AT: f32 = 3.0;

/// A query job read from `NVF_PLAY_TEST_QUERIES`.
pub struct QueryJob {
    pub set: String,
    steps: Vec<QueryStep>,
    next: usize,
    at: f32,
    out: PathBuf,
    observations: Vec<Json>,
    /// The document is written; the run may end.
    pub finished: bool,
}

impl QueryJob {
    /// The job of the test aid, if set; a job that cannot be read is logged and run as
    /// none.
    pub fn from_env() -> Option<QueryJob> {
        let path = PathBuf::from(std::env::var_os("NVF_PLAY_TEST_QUERIES").filter(|v| !v.is_empty())?);
        let job = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str::<Json>(&t).map_err(|e| e.to_string()))
            .and_then(|j| fnv_game::ext_probes::query_steps(&j));
        let (set, steps) = match job {
            Ok(j) => j,
            Err(e) => {
                warn!("NVF_PLAY_TEST_QUERIES {}: {e}", path.display());
                return None;
            }
        };
        let at = match std::env::var("NVF_PLAY_TEST_QUERIES_AT") {
            Ok(v) => v.trim().parse::<f32>().ok().filter(|s| *s >= 0.0).unwrap_or_else(|| {
                warn!("NVF_PLAY_TEST_QUERIES_AT={v}: expected seconds; {DEFAULT_AT} instead");
                DEFAULT_AT
            }),
            Err(_) => DEFAULT_AT,
        };
        let out = std::env::var_os("NVF_PLAY_TEST_QUERIES_OUT").filter(|v| !v.is_empty()).map_or_else(|| PathBuf::from(format!("{set}.probes.json")), PathBuf::from);
        info!("query job: {set}: {} steps from {}, the first at {at} s", steps.len(), path.display());
        Some(QueryJob::new(set, steps, at, out))
    }

    pub fn new(set: String, steps: Vec<QueryStep>, at: f32, out: PathBuf) -> QueryJob {
        QueryJob { set, steps, next: 0, at, out, observations: Vec::new(), finished: false }
    }

    /// Run the next step if it is due (`world_secs` on the world aids' clock, none while
    /// the world is loading); after the last one, write the document. True when it ran a
    /// step.
    pub fn frame(&mut self, game: &mut Game<'_>, world_secs: Option<f32>, loading: bool) -> bool {
        if self.finished || loading || world_secs.is_none_or(|w| w < self.at) {
            return false;
        }
        let Some(step) = self.steps.get(self.next) else {
            self.finish();
            return false;
        };
        let mut obs = run_step(game, step);
        obs.insert("label".into(), json!(step.label));
        debug!("query job: {} = {}", step.label, Json::Object(obs.clone()));
        self.observations.push(Json::Object(obs));
        self.next += 1;
        if self.next == self.steps.len() {
            self.finish();
        }
        true
    }

    /// The fnv.ext.probes/1 document of the steps run so far.
    pub fn document(&self) -> Json {
        let mut doc = probes_doc(&self.set, &self.observations, "nvforever --play (NVF_PLAY_TEST_QUERIES)");
        doc["steps"] = json!({"run": self.next, "of": self.steps.len()});
        doc
    }

    fn finish(&mut self) {
        self.finished = true;
        let doc = self.document();
        let written = fnv_install::guard_write(&self.out, None)
            .map_err(|e| e.to_string())
            .and_then(|_| serde_json::to_string_pretty(&doc).map_err(|e| e.to_string()))
            .and_then(|t| std::fs::write(&self.out, t).map_err(|e| e.to_string()));
        let rows = doc["rows"].as_array().map_or(0, Vec::len);
        let errors = doc["errors"].as_array().map_or(0, Vec::len);
        match written {
            Ok(()) => info!("query job: {}: {} steps, {rows} rows, {errors} errors, written to {}", self.set, self.next, self.out.display()),
            Err(e) => warn!("query job: {}: {}: {e}", self.set, self.out.display()),
        }
    }
}

/// The job is done: the run ends (a test aid drives it).
pub fn exit_when_done(script: Res<crate::script::ScriptState>, mut exit: MessageWriter<AppExit>, mut told: Local<bool>) {
    if !*told && script.query_job.as_ref().is_some_and(|j| j.finished) {
        *told = true;
        info!("query job: done, quitting");
        exit.write(AppExit::Success);
    }
}

/// At exit: a job that did not finish says how far it got (its document is not written).
pub fn report(mut exit: MessageReader<AppExit>, script: Res<crate::script::ScriptState>) {
    if exit.read().next().is_none() {
        return;
    }
    if let Some(j) = script.query_job.as_ref().filter(|j| !j.finished) {
        warn!("query job: {}: stopped after {} of {} steps (no document written)", j.set, j.next, j.steps.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_job_writes_its_document() {
        let dir = std::env::temp_dir().join(format!("nvf-query-job-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("empty.probes.json");
        let mut job = QueryJob::new("empty".into(), Vec::new(), 2.0, out.clone());
        assert_eq!(job.document()["steps"], json!({"run": 0, "of": 0}));
        job.finish();
        assert!(job.finished);
        let doc: Json = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(doc["format"], "fnv.ext.probes/1");
        assert_eq!(doc["set"], "empty");
        assert_eq!(doc["rows"], json!([]));
        std::fs::remove_dir_all(&dir).ok();
    }
}
