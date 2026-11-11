//! The player's choice about reports, `[Reports]` in the home's `NVForever.ini`
//! (docs/design/player-reports.md §2.2), and the home's `reports` folder.
//!
//! ```ini
//! [Reports]
//! Send=0              ; 0 off (the default, and after "Not now"), 1 on
//! ConsentVersion=1    ; the version of what is sent that the player answered
//! InstallId=          ; random UUID made when the player says yes; empty while off
//! DeleteToken=        ; random, made with InstallId; lets only this install delete its reports
//! Endpoint=           ; empty = the project's server
//! PastInstalls=       ; id:token,… of earlier installs (turned off), kept so their reports can still be deleted
//! ```
//!
//! Saving changes only these keys and keeps every other line of the file.

use crate::{guard_write, ini_get, ini_set, write_whole, EngineHome, InstallError};
use std::path::PathBuf;

/// `[Reports]` as read; `None` where a key is missing or empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reports {
    /// The player's answer: `None` never asked (or no answer stored).
    pub send: Option<bool>,
    /// The version of what is sent that the answer was given to.
    pub consent_version: Option<u32>,
    pub install_id: Option<String>,
    pub delete_token: Option<String>,
    /// A self-hosted server; `None` the project's own.
    pub endpoint: Option<String>,
    /// Earlier install IDs with their delete tokens (reports turned off since), so "Delete
    /// my reports" still reaches what they sent.
    pub past: Vec<(String, String)>,
}

const SECTION: &str = "Reports";

fn get(text: &str, key: &str) -> Option<String> {
    ini_get(text, SECTION, key).filter(|v| !v.is_empty())
}

impl Reports {
    pub fn from_ini(text: &str) -> Reports {
        Reports {
            send: get(text, "Send").map(|v| v == "1" || v.eq_ignore_ascii_case("true")),
            consent_version: get(text, "ConsentVersion").and_then(|v| v.parse().ok()),
            install_id: get(text, "InstallId"),
            delete_token: get(text, "DeleteToken"),
            endpoint: get(text, "Endpoint"),
            past: get(text, "PastInstalls")
                .map(|v| v.split(',').filter_map(|p| p.trim().split_once(':')).map(|(i, t)| (i.to_string(), t.to_string())).collect())
                .unwrap_or_default(),
        }
    }

    /// `text` with this section's keys set (an empty value for a `None` id or token, so
    /// turning reports off clears them; the endpoint is written only when set).
    pub fn to_ini(&self, text: &str) -> String {
        let mut t = ini_set(text, SECTION, "Send", if self.send == Some(true) { "1" } else { "0" });
        if let Some(v) = self.consent_version {
            t = ini_set(&t, SECTION, "ConsentVersion", &v.to_string());
        }
        t = ini_set(&t, SECTION, "InstallId", self.install_id.as_deref().unwrap_or(""));
        t = ini_set(&t, SECTION, "DeleteToken", self.delete_token.as_deref().unwrap_or(""));
        if let Some(e) = &self.endpoint {
            t = ini_set(&t, SECTION, "Endpoint", e);
        }
        if !self.past.is_empty() || get(&t, "PastInstalls").is_some() {
            let past: Vec<String> = self.past.iter().map(|(i, k)| format!("{i}:{k}")).collect();
            t = ini_set(&t, SECTION, "PastInstalls", &past.join(","));
        }
        t
    }

    pub fn load(home: &EngineHome) -> Reports {
        std::fs::read_to_string(home.config_file()).map(|t| Reports::from_ini(&t)).unwrap_or_default()
    }

    pub fn save(&self, home: &EngineHome) -> Result<(), InstallError> {
        let p = home.config_file();
        guard_write(&p, None)?;
        let old = match std::fs::read_to_string(&p) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(InstallError::Io(p, e)),
        };
        std::fs::create_dir_all(&home.root).map_err(|e| InstallError::Io(home.root.clone(), e))?;
        write_whole(&p, self.to_ini(&old).as_bytes())
    }
}

impl EngineHome {
    /// Reports waiting to be sent and the last ones sent (player-reports.md §1.3, §8).
    pub fn reports(&self) -> PathBuf {
        self.root.join("reports")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_round_trip_and_keep_the_rest_of_the_file() {
        let old = "; comment\n[Game]\nPath=F:\\Games\\FNV\n[Settings]\nmenu-pause=1\n";
        let r = Reports {
            send: Some(true),
            consent_version: Some(1),
            install_id: Some("3f2b8c1e-7a4d-4e2b-9c1a-0d5e6f7a8b9c".into()),
            delete_token: Some("abc".into()),
            endpoint: None,
            past: vec![("old-id".into(), "old-token".into())],
        };
        let text = r.to_ini(old);
        assert!(text.contains("Path=F:\\Games\\FNV") && text.contains("menu-pause=1") && text.starts_with("; comment"));
        assert_eq!(Reports::from_ini(&text), r);
        // Turned off: the id and token are cleared.
        let off = Reports { send: Some(false), install_id: None, delete_token: None, past: Vec::new(), ..r }.to_ini(&text);
        let back = Reports::from_ini(&off);
        assert_eq!((back.send, back.install_id, back.delete_token), (Some(false), None, None));
        assert_eq!(Reports::from_ini("[Game]\nPath=x\n"), Reports::default());
    }
}
