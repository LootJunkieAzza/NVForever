//! Dismemberment (docs/spec-notes/dismemberment.md): the hits that may sever or explode a
//! body part are queued by combat ([`Check`]); [`resolve`] rolls them by the rules of
//! `fnv_combat::dismember`, records the gone parts in the game (saved, read by IsLimbGone)
//! and tells the renderer ([`Dismembered`]: nvforever `dismember` draws the stumps, cuts
//! the ragdoll and throws the gore).

use bevy::prelude::*;
use fnv_combat::dismember::{self, Blow, Fate, OnHit, Settings};
use fnv_combat::rng::SplitMix;
use fnv_combat::vec::V3;
use fnv_sim::FormId;

use crate::combat::Combat;
use crate::hitvol::ActorParts;
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;

/// A hit that may dismember: the killing hit, or a hit on a corpse, on a body part.
#[derive(Clone, Copy, Debug)]
pub struct Check {
    pub target: FormId,
    /// The part's index in the actor's body part data (BPTD).
    pub part: usize,
    pub on_hit: OnHit,
    /// The part's condition is at or below 0 after the hit.
    pub crippled: bool,
    pub corpse: bool,
    pub overkill: f32,
    pub max_health: f32,
    pub by: Option<FormId>,
    /// Where it landed and the way it was going (game space).
    pub point: V3,
    pub dir: V3,
    /// KillActor's limb: this fate, no roll.
    pub scripted: Option<Fate>,
}

/// A body part went (docs/spec-notes/dismemberment.md §5).
#[derive(Message, Clone, Copy, Debug)]
pub struct Dismembered {
    pub form_id: u32,
    /// The body part data and the part's index in it.
    pub data: FormId,
    pub part: usize,
    /// GetHitLocation's number of the part.
    pub location: i32,
    pub exploded: bool,
    /// Where the hit landed and the way it was going (game space).
    pub point: V3,
    pub dir: V3,
}

/// The queue and the rolls (their own generator, so the other rolls keep their sequences).
#[derive(Resource)]
pub struct Dismemberment {
    pub checks: Vec<Check>,
    rng: SplitMix,
    settings: Option<Settings>,
    /// Parts gone so far (for the run report).
    pub severed: u32,
    pub exploded: u32,
}

impl Default for Dismemberment {
    fn default() -> Dismemberment {
        Dismemberment { checks: Vec::new(), rng: SplitMix(0xD15E), settings: None, severed: 0, exploded: 0 }
    }
}

/// Roll the queued checks; record and announce what went.
pub fn resolve(
    c: Res<Combat>,
    mut d: ResMut<Dismemberment>,
    mut sim: ResMut<SimState>,
    mut script: Option<ResMut<ScriptState>>,
    parts: Res<ActorParts>,
    mut out: MessageWriter<Dismembered>,
) {
    if d.checks.is_empty() {
        return;
    }
    let d = &mut *d;
    let settings = *d.settings.get_or_insert_with(|| Settings::from_forms(&c.forms));
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else {
        d.checks.clear();
        return;
    };
    for k in std::mem::take(&mut d.checks) {
        let Some(data_id) = parts.data_of(k.target.0) else {
            debug!("dismember: {} has no body parts drawn", k.target);
            continue;
        };
        let Some(data) = c.forms.body_parts.get(&data_id) else { continue };
        let Some(part) = data.parts.get(k.part) else { continue };
        if rt.game.limb_gone(k.target, part.location()) {
            continue;
        }
        // A creature flagged No Head, No Left Arm or No Right Arm has no such part to lose
        // (actor-fields.md §1).
        if sim.base_of(k.target.0).is_some_and(|b| rt.game.data.minor_flags(b, k.target).lacks(part.location())) {
            debug!("dismember: {} {}: the creature has no such part", k.target, part.name.as_deref().unwrap_or("part"));
            continue;
        }
        let (fate, why) = match k.scripted {
            Some(f) => (Some(f), "KillActor".to_string()),
            None => {
                let bloody_mess = k.by.is_some_and(|a| with_world(rt, &mut sim, |rt| rt.game.has_bloody_mess(a)));
                let blow = Blow { on_hit: k.on_hit, corpse: k.corpse, crippled: k.crippled, overkill: k.overkill, max_health: k.max_health, bloody_mess };
                let (pe, ps) = dismember::chances(part, &blow, &settings);
                let fate = dismember::decide(part, &blow, &settings, d.rng.next_f32());
                (fate, format!("overkill {:.0} of limb share {:.0}, explode {pe:.0} %, sever {ps:.0} %{}", k.overkill, dismember::limb_health(part, k.max_health), if bloody_mess { ", Bloody Mess" } else { "" }))
            }
        };
        let Some(fate) = fate else {
            debug!("dismember: {} {}: stays ({why})", k.target, part.name.as_deref().unwrap_or("part"));
            continue;
        };
        let gone = dismember::parts_gone(data, k.part, fate);
        let locations: Vec<(i32, bool)> = gone.iter().filter_map(|&(i, f)| Some((data.parts.get(i)?.location(), f == Fate::Explode))).collect();
        let new = with_world(rt, &mut sim, |rt| rt.game.dismember(k.target, &locations, k.by));
        let name = sim.ref_name(Some(&rt.game), k.target.0).map_or_else(|| k.target.to_string(), str::to_string);
        for (i, f) in gone {
            let Some(p) = data.parts.get(i) else { continue };
            let exploded = f == Fate::Explode;
            if !new.contains(&(p.location(), exploded)) {
                continue;
            }
            if exploded {
                d.exploded += 1;
            } else {
                d.severed += 1;
            }
            info!("dismember: {name} {}: {} {} ({why})", k.target, p.name.as_deref().unwrap_or("part"), if exploded { "exploded" } else { "severed" });
            out.write(Dismembered { form_id: k.target.0, data: data_id, part: i, location: p.location(), exploded, point: k.point, dir: k.dir });
        }
    }
}

/// KillActor's limb (`CombatRequest::Dismember`): the part at that location that can go,
/// its fate by its flags.
pub fn scripted_check(c: &Combat, parts: &ActorParts, sim: &SimState, actor: FormId, location: i32, by: Option<FormId>) -> Option<Check> {
    let data = c.forms.body_parts.get(&parts.data_of(actor.0)?)?;
    let (part, fate) = dismember::scripted(data, location)?;
    let point = sim.world.peek(actor).map_or([0.0; 3], |s| [s.pos[0], s.pos[1], s.pos[2] + 40.0]);
    Some(Check { target: actor, part, on_hit: OnHit::Normal, crippled: true, corpse: true, overkill: 0.0, max_health: 0.0, by, point, dir: [0.0, 0.0, 1.0], scripted: Some(fate) })
}
