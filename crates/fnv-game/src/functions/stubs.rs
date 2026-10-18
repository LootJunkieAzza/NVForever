//! What each stubbed function waits for (docs/design/script-stubs.md): the audit of
//! 2026-10-12. A stub ([`super::Kind::Stub`]) is recorded and has no effect; this table
//! says why it is still one. `fnvtool script stubs` lists them with their use in the
//! shipped scripts.

use super::{lookup, registry, Kind};

/// Why a function is not implemented, or why it does nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Its system is not built yet; the text names it.
    NeedsSystem(&'static str),
    /// Another workstream is building its system; the text names the workstream.
    Assigned(&'static str),
    /// The system behind it exists; left for later (little use in the shipped scripts).
    Implementable(&'static str),
    /// Deliberately nothing to do on this engine ([`Kind::NoOp`]).
    NoOp(&'static str),
}

impl Status {
    pub fn key(self) -> &'static str {
        match self {
            Status::NeedsSystem(_) => "needs_system",
            Status::Assigned(_) => "assigned",
            Status::Implementable(_) => "implementable",
            Status::NoOp(_) => "no_op",
        }
    }
}

/// The workstreams of the owner's list 2 (docs/handoff/playtest-2026-10-12.md §3).
pub const CRIME: &str = "J karma and crime";
pub const FOLLOWERS: &str = "H followers";

/// Stubs whose system another workstream is building now or next.
pub const ASSIGNED: &[(u16, &str)] = &[
];

/// Stubs whose system exists, left for a later round; the text says what is missing.
pub const IMPLEMENTABLE: &[(u16, &str)] = &[
    (0x1225, "AutoDisplayObjectives: objectives already show at the left as scripts set them; what the switch adds is not documented beyond the summary"),
];

/// Why the stub `opcode` is one; `None` for an implemented or unknown opcode.
pub fn status(opcode: u16) -> Option<Status> {
    match lookup(opcode)? {
        Kind::Impl(_) => None,
        Kind::NoOp(why) => Some(Status::NoOp(why)),
        Kind::Stub(system) => Some(if let Some((_, w)) = ASSIGNED.iter().find(|(o, _)| *o == opcode) {
            Status::Assigned(w)
        } else if let Some((_, why)) = IMPLEMENTABLE.iter().find(|(o, _)| *o == opcode) {
            Status::Implementable(why)
        } else {
            Status::NeedsSystem(system)
        }),
    }
}

/// Every stub and no-op of the registry with its status, in registry order.
pub fn audit() -> Vec<(u16, &'static str, Status)> {
    registry().iter().filter_map(|(op, name, _)| status(*op).map(|s| (*op, *name, s))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tables_name_stubs_only() {
        // An entry that became implemented is taken out of the tables by whoever
        // implements it (another workstream may: the test says which).
        for (op, _) in ASSIGNED.iter().chain(IMPLEMENTABLE) {
            assert!(matches!(lookup(*op), Some(Kind::Stub(_))), "0x{op:04X} is not a stub any more: take it out of stubs.rs");
        }
    }

    #[test]
    fn every_stub_has_a_status() {
        let a = audit();
        assert!(a.iter().any(|(_, n, s)| *n == "SetRumble" && s.key() == "no_op"));
        assert!(a.iter().all(|(op, ..)| !super::super::implemented(*op) || matches!(lookup(*op), Some(Kind::NoOp(_)))));
    }

    /// The smoke scenario script-stubs expects the totals of tools/smoke/expected/script-stubs.toml;
    /// those the registry decides alone (stubs, by status) are checked here, without game data,
    /// so a branch that implements a stub and leaves the file behind fails `cargo test`.
    #[test]
    fn smoke_totals_match_the_registry() {
        let text = include_str!("../../../../tools/smoke/expected/script-stubs.toml");
        let mut table = "";
        let mut file = std::collections::BTreeMap::new();
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            if let Some(t) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                table = t;
            } else if let Some((k, v)) = line.split_once('=') {
                let v: u64 = v.trim().parse().unwrap_or_else(|_| panic!("not a count: {line}"));
                assert!(file.insert(format!("{table}.{}", k.trim()), v).is_none(), "{table}.{} twice", k.trim());
            }
        }
        let mut want = std::collections::BTreeMap::new();
        for (.., s) in audit() {
            *want.entry(format!("summary.by_status.{}", s.key())).or_insert(0u64) += 1;
        }
        let stubs = want.iter().filter(|(k, _)| !k.ends_with(".no_op")).map(|(_, n)| n).sum::<u64>();
        want.insert("summary.stubs".into(), stubs);
        let got: std::collections::BTreeMap<_, _> =
            file.into_iter().filter(|(k, _)| k == "summary.stubs" || k.starts_with("summary.by_status.")).collect();
        assert_eq!(got, want, "tools/smoke/expected/script-stubs.toml disagrees with the registry: update its stubs and by_status");
    }
}
