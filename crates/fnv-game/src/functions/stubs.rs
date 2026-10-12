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
pub const COMBAT: &str = "E2 combat AI";
pub const CRIME: &str = "J karma and crime";
pub const FOLLOWERS: &str = "H followers";

/// Stubs whose system another workstream is building now or next.
pub const ASSIGNED: &[(u16, &str)] = &[
    (0x105F, COMBAT), // SendAssaultAlarm (assault rules)
    (0x10B1, COMBAT), // SetCombatStyle
    (0x11A0, COMBAT), // GetGroupMemberCount
    (0x11A1, COMBAT), // GetGroupTargetCount
    (0x11E2, COMBAT), // FireWeapon
    (0x119C, COMBAT), // UseWeapon
    (0x1042, COMBAT), // GetShouldAttack
    (0x1203, COMBAT), // IsCombatTarget
    (0x10F5, COMBAT), // ForceFlee
    (0x106B, COMBAT), // GetKnockedState
    (0x1259, COMBAT), // GetForceHitReaction
    (0x118D, COMBAT), // GetCauseofDeath
    (0x118E, COMBAT), // IsLimbGone
    (0x1051, COMBAT), // GetArmorRating
    (0x11AC, COMBAT), // GetPlantedExplosive
    (0x119F, COMBAT), // Exists (combat dialogue)
    (0x115C, CRIME),  // SendTrespassAlarm
    (0x1090, CRIME),  // GetTrespassWarningLevel
    (0x1091, CRIME),  // IsTrespassing
    (0x103D, CRIME),  // GetAlarmed
    (0x1075, CRIME),  // GetMajorCrimeCount
    (0x124E, CRIME),  // GetOwnerLastTarget
    (0x11C8, FOLLOWERS), // OpenTeammateContainer
];

/// Stubs whose system exists, left for a later round; the text says what is missing.
pub const IMPLEMENTABLE: &[(u16, &str)] = &[
    (0x1166, "IsPlayerMovingIntoNewSpace: the engine's load doors and fast travel would tell it"),
    (0x1122, "SetPackDuration: the package model has durations; a per-actor override is missing"),
    (0x118B, "SetActorsAI: the residents and the AI could skip an actor; the switch is missing"),
    (0x1004, "Rotate: references move (SetAngle); a per-frame turn is missing"),
    (0x11F8, "ShowAllMapMarkers: the Pip-Boy map reads markers; the game crate has no marker list"),
    (0x1186, "SetGlobalTimeMultiplier: the engine has a time scale; the game crate does not ask for one"),
    (0x118A, "PushActorAway: the engine has ragdolls; a scripted push is missing"),
    (0x1225, "AutoDisplayObjectives: quest messages exist; the switch for when the Pip-Boy is away is missing"),
    (0x101A, "IsTurning: the engine knows the turn rate; it does not tell the game yet"),
    (0x10D3, "SetForceSneak: the player's sneak exists; NPCs never sneak yet"),
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
        assert!(a.iter().any(|(_, n, s)| *n == "SendTrespassAlarm" && *s == Status::Assigned(CRIME)));
        assert!(a.iter().all(|(op, ..)| !super::super::implemented(*op) || matches!(lookup(*op), Some(Kind::NoOp(_)))));
    }
}
