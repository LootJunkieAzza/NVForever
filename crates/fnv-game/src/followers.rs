//! The player's teammates: companions hired by the game's own scripts (docs/design/followers.md).
//!
//! Hiring is the companion's dialogue: a line's result script calls `SetPlayerTeammate 1`,
//! `SetIgnoreFriendlyHits 1`, sets the hire variables of the quest VNPCFollowers and
//! `ResetAI` [observed: FalloutNV.esm, Boone's INFO 0x0015F30D and 0x0015D162]. The
//! companion's Follow package (e.g. FollowersBooneFollowPlayerDEFAULT, conditioned on
//! `VNPCFollowers.bBooneHired == 1` and `CraigBooneREF.Waiting == 0`) and its Wait package
//! (a Guard package on `Waiting == 1`) are then chosen by the ordinary package choice
//! [observed]. What this module adds is what `SetPlayerTeammate` itself does [documented:
//! GECK wiki SetPlayerTeammate]:
//! - "Changing cells will teleport the teammate to the player" ([`Game::bring_teammates`];
//!   so does an actor following the player on a Follow package, owner decision 2026-10-23);
//! - "The actor will act and react as if he is in the player faction for the purposes of
//!   combat. Other actors will treat the actor as if he were in the player faction"
//!   (`Game::would_attack`, [`Game::teammates_assist`]);
//! - "Teammate state is cleared on death of the actor" (`Game::kill`);
//! - OpenTeammateContainer.
//!
//! Friendly fire is `crate::provocation`'s: a teammate is the player's ally, and the hire
//! scripts' `SetIgnoreFriendlyHits 1` makes it ignore every hit by the player.

use std::collections::BTreeMap;

use fnv_script::vm::Value;
use fnv_sim::FormId;

use crate::awareness::Cause;
use crate::detection::Band;
use crate::functions::{Args, Ctx};
use crate::game::Game;
use crate::trace::EventKind;
use crate::package::{PackageType, TargetKind};
use crate::provocation::Note;

/// Companion Nerve per point of the player's Charisma ([`Game::companion_nerve`]).
pub const COMPANION_NERVE_PER_CHARISMA: f64 = 0.05;

/// How far beside the player a teammate is put when the player changes cells, and the
/// step between several teammates. Through a load door the original put a following
/// Boone about 170 units to the player's left and 12 to 30 ahead, settling about 145 to
/// 155 left [observed: od_03_teammate_door 2026-10-18 and 2026-10-19, Novac's motel
/// door, owner-decision-captures.md §7]; was 64 to the right [design: the wiki says only
/// "teleport to the player"]. One door was measured: the side and distance may depend on
/// where the door's marker stands [hypothesis].
pub const BESIDE: f32 = 160.0;

/// How near the player's old place, outdoors, an actor following the player (not a
/// teammate) must be to come along when the player changes cells; indoors, the same cell
/// is enough [design: about the distance a follower falls behind and still catches up].
pub const FOLLOW_REACH: f32 = 4096.0;

impl Game<'_> {
    /// Whether `r` is the player's teammate now (SetPlayerTeammate 1, alive).
    pub fn is_teammate(&self, r: FormId) -> bool {
        r != FormId::PLAYER_REF && self.extra.get(&r).is_some_and(|e| e.teammate && !e.dead)
    }

    /// Companion Nerve of `actor`: for a teammate, the share its damage and armour go up
    /// by, 0.05 per point of the player's Charisma (0.25 at 5, 0.5 at 10) [documented:
    /// Fallout Wiki, Fallout: New Vegas derived statistics, "(CHA × 5)%"; Charisma (Fallout:
    /// New Vegas), "+5% to their damage and armor, up to a maximum of +50% at 10
    /// Charisma"; no setting in the masters names it, observed]; 0 for anyone else. The
    /// player's current Charisma within 1 to 10 [hypothesis: current, not permanent, as
    /// the other derived statistics here, docs/spec-notes/actor-values.md §1].
    pub fn companion_nerve(&mut self, actor: FormId) -> f64 {
        if !self.is_teammate(actor) {
            return 0.0;
        }
        COMPANION_NERVE_PER_CHARISMA * crate::functions::actor_value_capped(self, FormId::PLAYER_REF, crate::actor_values::CHARISMA)
    }

    /// The player's teammates, by FormID.
    pub fn teammates(&self) -> Vec<FormId> {
        self.extra.iter().filter(|(r, e)| e.teammate && !e.dead && **r != FormId::PLAYER_REF).map(|(r, _)| *r).collect()
    }

    /// Whether `actor`'s current package is a Follow or Accompany package with the player
    /// as its target (a companion that is not told to wait).
    pub fn follows_player(&mut self, actor: FormId) -> bool {
        let data = self.data;
        self.current_package(actor).and_then(|p| data.package_defs.get(&p)).is_some_and(|d| {
            matches!(d.ty, PackageType::Follow | PackageType::Accompany) && matches!(d.target.map(|t| t.kind), Some(TargetKind::Reference(Some(r))) if r == FormId::PLAYER_REF)
        })
    }

    /// Whether a teammate was told to wait: its current package is a Guard package (the
    /// companions' Wait packages are, chosen by their `Waiting` variable) [hypothesis].
    pub fn told_to_wait(&mut self, actor: FormId) -> bool {
        let data = self.data;
        self.current_package(actor).and_then(|p| data.package_defs.get(&p)).is_some_and(|d| d.ty == PackageType::Guard)
    }

    /// The player has just changed cells (a load door, MoveTo, fast travel), from `from`
    /// (the player's cell and position before the move): every teammate not told to wait,
    /// and every other actor following the player from near where the player was
    /// ([`Game::follows_player`], [`FOLLOW_REACH`]), is put beside the player, in the
    /// player's cell, facing the player's way [documented: "Changing cells will teleport
    /// the teammate to the player"; observed: the original brought Boone along on his own
    /// PATROL package, not following, through a door and by MoveTo, and left him behind
    /// with `Waiting` 1 (od_03 and the `followers` campaign, owner-decision-captures.md §7,
    /// npc-behaviour-compare.md §13); hypothesis: that waiting is what kept him, read here
    /// as a Guard package; owner decision 2026-10-23: an actor on a Follow package that is
    /// not a teammate (Deputy Beagle in the Primm hotel) comes along at once too, not on
    /// foot through the door seconds later]. They go to [`Game::teleported`] like a MoveTo
    /// and to [`Game::brought_along`] for the log. Returns them.
    pub fn bring_teammates(&mut self, from: Option<(Option<FormId>, [f32; 3])>) -> Vec<FormId> {
        let Some((Some(cell), pos, rot)) = self.world.peek(FormId::PLAYER_REF).map(|p| (p.cell, p.pos, p.rot)) else { return Vec::new() };
        let mut who: Vec<FormId> = self.teammates().into_iter().filter(|&t| self.present(t) && !self.told_to_wait(t)).collect();
        if let Some((from_cell, from_pos)) = from {
            let candidates: Vec<FormId> = self
                .extra
                .iter()
                .filter(|(r, e)| **r != FormId::PLAYER_REF && e.current_package.is_some() && !e.dead && !e.teammate)
                .map(|(r, _)| *r)
                .collect();
            for r in candidates {
                if self.present(r) && self.follows_player(r) && self.was_near(r, from_cell, from_pos) {
                    who.push(r);
                }
            }
        }
        let mut out = Vec::new();
        for t in who {
            // To the player's left (as observed), then right, one step further for each.
            let n = out.len();
            let side = if n % 2 == 0 { -1.0 } else { 1.0 } * BESIDE * (1 + n / 2) as f32;
            let h = rot[2];
            let at = [pos[0] + h.cos() * side, pos[1] - h.sin() * side, pos[2]];
            let cell = self.world.placed.exterior_cell_at(cell, at).unwrap_or(cell);
            let Ok(s) = self.world.get(t) else { continue };
            (s.cell, s.pos, s.rot) = (Some(cell), at, [0.0, 0.0, h]);
            if self.ref_script(t).is_some() {
                self.moved.insert(t);
            }
            self.teleported.insert(t);
            self.brought_along.insert(t);
            self.emit(EventKind::Moved { reference: t, cell: Some(cell), pos: at });
            out.push(t);
        }
        out
    }

    /// Whether `r` stands where the player was before a move: in the same cell, or in the
    /// same worldspace within [`FOLLOW_REACH`].
    fn was_near(&self, r: FormId, from_cell: Option<FormId>, from_pos: [f32; 3]) -> bool {
        let Some((cell, pos)) = self.world.peek(r).map(|s| (s.cell, s.pos)).or_else(|| self.world.placed.get(r).map(|p| (p.cell, p.pos))) else { return false };
        let (Some(cell), Some(from_cell)) = (cell, from_cell) else { return false };
        if cell == from_cell {
            return true;
        }
        let near = (pos[0] - from_pos[0]).hypot(pos[1] - from_pos[1]) <= FOLLOW_REACH;
        near && self.world.placed.world_of(cell).is_some_and(|w| self.world.placed.world_of(from_cell) == Some(w))
    }

    /// The player's teammates take up the player's fights: a teammate that perceives the
    /// player (band Detected) gets an alarm at each actor that has the player on its
    /// target list and that it has neither on its own list nor as an alarm already, so it
    /// goes Alert and fights once it detects that actor (`Game::would_attack`: whoever
    /// fights the player is a teammate's enemy) [documented: a teammate acts "as if he is
    /// in the player faction for the purposes of combat"; design: the alarm, as for an
    /// ally's assistance, without the Assistance check]. Called with
    /// `Game::call_allies`. Returns how many alarms were given.
    pub fn teammates_assist(&mut self) -> usize {
        let mates: Vec<FormId> = self
            .teammates()
            .into_iter()
            .filter(|&t| self.present(t) && self.detection.pairs.get(&(t, FormId::PLAYER_REF)).is_some_and(|(d, _)| d.band == Band::Detected))
            .collect();
        if mates.is_empty() {
            return 0;
        }
        let foes: BTreeMap<FormId, [f32; 3]> = self
            .awareness
            .actors
            .iter()
            .filter(|(id, a)| a.state.in_combat() && a.targets.contains_key(&FormId::PLAYER_REF) && !self.is_teammate(**id))
            .filter_map(|(id, _)| self.world.peek(*id).map(|s| s.pos).or_else(|| self.world.placed.get(*id).map(|p| p.pos)).map(|p| (*id, p)))
            .collect();
        let mut given = 0;
        for t in mates {
            for (&foe, &at) in &foes {
                if foe == t || self.is_dead(foe) {
                    continue;
                }
                let knows = self.awareness.get(t).is_some_and(|a| a.targets.contains_key(&foe) || a.alarms.iter().any(|x| x.owner == foe));
                if knows || !self.would_attack(t, foe) {
                    continue;
                }
                let tm = self.awareness.timers.unwrap_or_default();
                let now = self.time;
                self.awareness.actors.entry(t).or_default().alarm(foe, at, 0.0, Cause::Ally, now, &tm);
                self.provocations.notes.push(Note::Assists { helper: t, victim: FormId::PLAYER_REF, attacker: foe });
                given += 1;
            }
        }
        given
    }
}

/// `actor.OpenTeammateContainer [ignoreTeammate]`: the container menu on the actor's
/// inventory, "which allows the player to freely swap inventory with the actor"; without
/// the flag only for the player's teammates [documented: GECK wiki
/// OpenTeammateContainer]. The engine shows it (`Game::teammate_containers`) once no
/// conversation is open [design: the calls are in result scripts of companions' lines].
pub fn open_teammate_container(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if a.num_or(0, 0.0) == 0.0 && !g.is_teammate(r) {
        return Ok(Value::NONE);
    }
    g.teammate_containers.push(r);
    Ok(Value::NONE)
}
