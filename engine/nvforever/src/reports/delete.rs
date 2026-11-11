//! "Delete my reports" (player-reports.md §6): the settings row Gameplay > Reports >
//! "Reports already sent". Choosing "Delete them now" asks the server to delete every
//! report this PC sent: the current install ID and the earlier ones kept since reports were
//! turned off (`[Reports] PastInstalls`), each with its delete token (§5.1 `DELETE
//! /v1/install/{id}`). The server removes them within 7 days. When it has taken every
//! request, the past IDs are forgotten and, with reports on, the install gets a new ID, so
//! later reports are not linked to the deleted ones.

use bevy::prelude::*;
use fnv_install::reports::Reports;
use fnv_install::EngineHome;
use std::sync::atomic::{AtomicU8, Ordering};

pub const SETTING_KEY: &str = "reports-delete";
const KEPT: &str = "Kept on the server";
const DELETING: &str = "Deleting…";
const DELETED: &str = "Deleted";
const FAILED: &str = "Could not delete, try again";
const NONE: &str = "None sent from this PC";
const DELETE: &str = "Delete them now";

static STATE: AtomicU8 = AtomicU8::new(0); // 0 idle, 1 deleting, 2 deleted, 3 failed

/// The installs to delete: the current one (if any) and the past ones.
fn installs(r: &Reports) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = r.install_id.clone().zip(r.delete_token.clone()).into_iter().collect();
    v.extend(r.past.iter().cloned());
    v
}

fn status() -> &'static str {
    match STATE.load(Ordering::Relaxed) {
        1 => DELETING,
        2 => DELETED,
        3 => FAILED,
        _ if EngineHome::user().is_none_or(|h| installs(&Reports::load(&h)).is_empty()) => NONE,
        _ => KEPT,
    }
}

/// Where deletions go: the endpoint reports use (`NVF_REPORTS_ENDPOINT`, `[Reports]
/// Endpoint`, else the project's server). Deleting is allowed from any build.
fn endpoint(r: &Reports) -> String {
    std::env::var("NVF_REPORTS_ENDPOINT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| r.endpoint.clone())
        .unwrap_or_else(|| super::upload::DEFAULT_ENDPOINT.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn start() {
    if STATE.swap(1, Ordering::Relaxed) == 1 {
        return;
    }
    let _ = std::thread::Builder::new().name("nvf-reports-delete".into()).spawn(|| {
        let Some(home) = EngineHome::user() else {
            STATE.store(3, Ordering::Relaxed);
            return;
        };
        let r = Reports::load(&home);
        let url = endpoint(&r);
        let all = installs(&r);
        // 202: deletion scheduled. Anything else (no answer, a refused token) leaves the IDs
        // in place so the player can try again.
        let ok = all.iter().all(|(id, token)| matches!(super::upload::http::delete(&format!("{url}/v1/install/{id}"), token), Some(200..=299)));
        if !ok {
            warn!("reports: deleting the reports sent from this PC failed (the server could not be reached or refused); try again later");
            STATE.store(3, Ordering::Relaxed);
            return;
        }
        let mut r = Reports::load(&home);
        r.past.clear();
        if r.send == Some(true) {
            r.install_id = Some(super::new_uuid());
            r.delete_token = Some(super::new_token());
        } else {
            r.install_id = None;
            r.delete_token = None;
        }
        match r.save(&home) {
            Ok(()) => {
                info!("reports: the server was asked to delete the reports of {} install ID(s) from this PC; later reports get a new ID", all.len());
                STATE.store(2, Ordering::Relaxed);
            }
            Err(e) => {
                warn!("reports: deleted on the server, but the new ID could not be stored: {e}");
                STATE.store(2, Ordering::Relaxed);
            }
        }
    });
}

pub fn add_row(app: &mut App) {
    nvf_play::settings::add_choice_list(
        app,
        nvf_play::settings::Setting {
            key: SETTING_KEY,
            group: "Reports",
            label: "Reports already sent",
            note: "Delete them now asks the server to delete every report this PC sent (done within 7 days); later reports get a new ID",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        nvf_play::settings::ChoiceList {
            names: |_| {
                let s = status();
                if matches!(s, NONE | DELETING) {
                    vec![s.to_string()]
                } else {
                    vec![s.to_string(), DELETE.to_string()]
                }
            },
            chosen: |_| status().to_string(),
            pick: |_, name| {
                if name != DELETE {
                    return false;
                }
                start();
                true
            },
            // Nothing to keep: a start never deletes by itself.
            save: |_| "kept".to_string(),
            load: |_, _| true,
            ask_keep: false,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_install_this_pc_had_is_deleted() {
        let r = Reports {
            install_id: Some("now".into()),
            delete_token: Some("t-now".into()),
            past: vec![("old".into(), "t-old".into())],
            ..Default::default()
        };
        assert_eq!(installs(&r), vec![("now".to_string(), "t-now".to_string()), ("old".to_string(), "t-old".to_string())]);
        assert!(installs(&Reports::default()).is_empty());
    }
}
