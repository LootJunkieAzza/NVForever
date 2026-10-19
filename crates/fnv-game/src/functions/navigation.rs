//! Navmesh switches: EnableNavMesh, DisableNavmesh and setAllReachable
//! (docs/design/script-stubs.md §3.16), and the AI's per-frame switches SetAllVisible
//! and SetNoAvoidance (§3.19, [`FrameSwitches`]). The rules keep what the scripts switched
//! ([`NavSwitches`], saved); the layer that plans ways over the navmesh applies it to its
//! graph (`fnv_ai::Ai::apply_navmesh_switches`): a disabled navmesh takes no part in
//! paths until it is enabled again.

use super::{none, Args, Ctx};
use crate::game::Game;
use fnv_script::vm::{ArgValue, Value};
use fnv_sim::FormId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What the scripts switched about navigation. Saved with the game.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NavSwitches {
    /// Navmeshes (NAVM FormIDs) a script enabled (`true`) or disabled (`false`); the
    /// others are as the load order has them (the record's "Initially Disabled" flag).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub navmeshes: BTreeMap<FormId, bool>,
    /// setAllReachable's switch ([`set_all_reachable`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub all_reachable: bool,
}

impl NavSwitches {
    pub fn is_empty(&self) -> bool {
        self.navmeshes.is_empty() && !self.all_reachable
    }

    /// The navmesh switches by raw FormID, as `fnv_nav::NavGraph::apply_switches` takes
    /// them.
    pub fn by_id(&self) -> BTreeMap<u32, bool> {
        self.navmeshes.iter().map(|(k, v)| (k.0, *v)).collect()
    }
}

/// The navmesh a call names: a reference to the NAVM record, a raw FormID as a number
/// (the wiki's "only works using FormIDs"; the decompiled bridge script shows two of its
/// four arguments as bare numbers, `be880` and `be87f` [observed]), or, from the console,
/// an Editor ID or hex FormID as text.
fn navmesh_arg(g: &Game, a: &Args) -> Result<FormId, String> {
    match a.get(0) {
        Some(ArgValue::Str(s)) => super::console::form(g, s),
        Some(ArgValue::Value(Value::Num(n))) if *n >= 1.0 => Ok(FormId(*n as u32)),
        _ => a.form(0),
    }
}

/// `EnableNavMesh navmesh`: the navmesh takes part in paths again [documented: GECK
/// wiki EnableNavMesh, "Enables the specified navmesh"]. The shipped calls open ways
/// once something that blocked them is gone: Red Rock Canyon's bridge
/// (`RCExBridge1SCRIPT`, SCPT 0x00072021, once its "forward" animation has played) and
/// the Lonesome Road elevator ride (LonesomeRoad.esm SCPT 0x04003217, 75 s after it
/// set off) [observed: decompiled scripts]. The argument names the NAVM record itself
/// [observed: `NVDLC04ElevNav01REF` and `NVDLC04ElevNav02REF` are NAVM records].
pub fn enable_navmesh(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let m = navmesh_arg(g, a)?;
    g.nav_switches.navmeshes.insert(m, true);
    none()
}

/// `DisableNavmesh navmesh`: the navmesh takes no part in paths until it is enabled
/// again [documented: GECK wiki DisableNavmesh]: actors plan no way across it, and one
/// walking a way over it plans again (`fnv_ai`). The shipped calls (LonesomeRoad.esm
/// SCPT 0x04003217) take the elevator's two navmeshes away while it rides [observed:
/// decoded script]. The wiki notes that the call works with raw FormIDs, without the
/// load order's fix-up [documented: GECK wiki DisableNavmesh notes]; here the scripts'
/// references are fixed up like any other, so a DLC's call names its own navmesh
/// [design; listed in docs/owner-review.md].
pub fn disable_navmesh(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let m = navmesh_arg(g, a)?;
    g.nav_switches.navmeshes.insert(m, false);
    none()
}

/// `setAllReachable flag`: kept for the AI ([`NavSwitches::all_reachable`]). The two
/// shipped calls are optimisations of the Hoover Dam battle beside SetAllVisible and
/// SetNoAvoidance: `VHDBattleOptimizerScript` (SCPT 0x001418DB) and
/// `ComplexSceneScriptPS3SCRIPT` (SCPT 0x0017B714), whose quest stops itself unless the
/// game runs on a PS3 [observed: decompiled scripts]. Its meaning, every place counting
/// as reachable so that the AI skips the reachability test [hypothesis: from the name
/// and its company]. The ways here are planned in full either way, so nothing reads the
/// switch's effect: the flag is kept and saved, and the battle's script runs as written.
pub fn set_all_reachable(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.nav_switches.all_reachable = a.num_or(0, 0.0) != 0.0;
    none()
}

/// The two of the original's four performance switches for crowded fights
/// [documented: GECK wiki SetAllVisible, SetNoAvoidance: "one of 4 functions which can
/// be used to increase the game's performance"] that act for one frame only: "the flag
/// is cleared every frame, so this function needs to be called in a script running on a
/// reference in the loaded area" [documented: same pages]. The rules clear them at each
/// [`Game::tick`] before the scripts run, so between two ticks they hold what that
/// frame's scripts said; they are not saved. The shipped calls, all `... 1` from
/// GameMode blocks [observed: decompiled scripts, 2026-10-19]: the Hoover Dam battle's
/// optimiser quest (`VHDBattleOptimizerScript`, SCPT 0x001418DB, beside setAllReachable;
/// a quest script, so on the frames its quest runs), an object script that starts a
/// fight with the player (SCPT 0x0015E8F9, beside SetSceneIsComplex, once it has),
/// `ComplexSceneScriptPS3SCRIPT` (SCPT 0x0017B714, PS3 only) and two Lonesome Road
/// object scripts (LonesomeRoad.esm SCPT 0x0400DEBC and 0x0400974E, SetAllVisible alone).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameSwitches {
    /// SetAllVisible: actors skip line-of-sight tests and take what they look at as seen
    /// [documented: "they assume they can see its head"]. The engine's detection reads it
    /// (`nvforever` detection.rs): an NPC's sight of an actor needs no ray then; the
    /// field of view still applies [documented: only the line-of-sight check is named].
    pub all_visible: bool,
    /// SetNoAvoidance: actors skip their "avoid box movement": they "do not open doors
    /// or angle away from each other" [documented: GECK wiki SetNoAvoidance]. Kept and
    /// read by nothing: NVForever's walkers do no avoid-box steering, and its keeping
    /// apart (`fnv_ai::apart`) stands for the character controllers' blocking, which the
    /// flag does not name [hypothesis]; NPCs opening doors is left as it is [design: a
    /// door kept shut in the Hoover Dam battle would strand the fight, unverified].
    pub no_avoidance: bool,
}

/// `SetAllVisible flag`: this frame's [`FrameSwitches::all_visible`].
pub fn set_all_visible(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.frame_switches.all_visible = a.num_or(0, 0.0) != 0.0;
    none()
}

/// `SetNoAvoidance flag`: this frame's [`FrameSwitches::no_avoidance`].
pub fn set_no_avoidance(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.frame_switches.no_avoidance = a.num_or(0, 0.0) != 0.0;
    none()
}
