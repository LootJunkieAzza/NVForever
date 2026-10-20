//! Dismemberment: whether a hit severs or explodes the body part it lands on
//! (docs/spec-notes/dismemberment.md §4). Pure rules: the caller supplies the rolls.

use crate::data::{BodyPart, BodyPartData, CombatForms};
use serde::Serialize;

/// A weapon's On Hit choice (WEAP `DNAM`) [documented: GECK wiki Weapons].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub enum OnHit {
    /// "Normal formula behavior": the weapon rolls to sever and to explode.
    #[default]
    Normal,
    DismemberOnly,
    ExplodeOnly,
    Neither,
}

impl OnHit {
    /// From the schema's label; an unknown or missing value is Normal.
    pub fn from_label(s: Option<&str>) -> OnHit {
        match s {
            Some("Dismember Only") => OnHit::DismemberOnly,
            Some("Explode Only") => OnHit::ExplodeOnly,
            Some("No Dismember/Explode") => OnHit::Neither,
            _ => OnHit::Normal,
        }
    }

    pub fn may_explode(self) -> bool {
        matches!(self, OnHit::Normal | OnHit::ExplodeOnly)
    }

    pub fn may_sever(self) -> bool {
        matches!(self, OnHit::Normal | OnHit::DismemberOnly)
    }
}

/// What happens to a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Fate {
    Sever,
    Explode,
}

/// The game settings the rule reads, with the masters' values as defaults [observed:
/// FalloutNV.esm GMST, §3].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Settings {
    pub dismember_chance: f32,
    pub explode_chance: f32,
    pub mess_intact_dismember: f32,
    pub mess_intact_explode: f32,
    pub mess_crippled_explode: f32,
    pub mess_torso_explode: f32,
    /// Seconds from the hit to the exploded part's skin going.
    pub explode_delay: f32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { dismember_chance: 50.0, explode_chance: 75.0, mess_intact_dismember: 15.0, mess_intact_explode: 25.0, mess_crippled_explode: 25.0, mess_torso_explode: 10.0, explode_delay: 0.03 }
    }
}

impl Settings {
    pub fn from_forms(forms: &CombatForms) -> Settings {
        let d = Settings::default();
        let s = |k: &str, v: f32| forms.setting(k).unwrap_or(v);
        Settings {
            dismember_chance: s("iCombatDismemberPartChance", d.dismember_chance),
            explode_chance: s("iCombatExplodePartChance", d.explode_chance),
            mess_intact_dismember: s("iMessIntactLimbDismemberChance", d.mess_intact_dismember),
            mess_intact_explode: s("iMessIntactLimbExplodeBonus", d.mess_intact_explode),
            mess_crippled_explode: s("iMessCrippledLimbExplodeBonus", d.mess_crippled_explode),
            mess_torso_explode: s("iMessTorsoExplodeChance", d.mess_torso_explode),
            explode_delay: s("fExplodeLimbRemovalDelay", d.explode_delay),
        }
    }
}

/// The hit, as the rule sees it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Blow {
    pub on_hit: OnHit,
    /// The target was already dead (a corpse).
    pub corpse: bool,
    /// The part hit is crippled (its condition at or below 0 after the hit).
    pub crippled: bool,
    /// Damage beyond the health the target had left (the whole damage on a corpse).
    pub overkill: f32,
    /// The target's maximum health.
    pub max_health: f32,
    /// The attacker has Bloody Mess.
    pub bloody_mess: bool,
}

/// The part's share of the target's maximum health, at least 1.
pub fn limb_health(part: &BodyPart, max_health: f32) -> f32 {
    (max_health * part.health_percent as f32 / 100.0).max(1.0)
}

/// The chances (percent) that the blow explodes and, failing that, severs the part.
pub fn chances(part: &BodyPart, b: &Blow, s: &Settings) -> (f32, f32) {
    let ready = b.corpse || b.crippled || b.bloody_mess;
    if !ready {
        return (0.0, 0.0);
    }
    let intact = !b.corpse && !b.crippled;
    let torso = part.location() == 0;
    let mut explode = 0.0;
    let lh = limb_health(part, b.max_health);
    if part.explodable() && b.on_hit.may_explode() && b.overkill >= lh {
        explode = part.explodable_chance_percent as f32 + s.explode_chance * ((b.overkill - lh) / lh).min(1.0);
        if b.bloody_mess {
            explode += if intact { s.mess_intact_explode } else { s.mess_crippled_explode };
            if torso {
                explode += s.mess_torso_explode;
            }
        }
    }
    let mut sever = 0.0;
    if part.severable() && b.on_hit.may_sever() {
        sever = if intact { s.mess_intact_dismember } else { s.dismember_chance };
    }
    (explode.clamp(0.0, 100.0), sever.clamp(0.0, 100.0))
}

/// What the blow does to the part, given two rolls uniform in [0, 1).
pub fn decide(part: &BodyPart, b: &Blow, s: &Settings, explode_roll: f32, sever_roll: f32) -> Option<Fate> {
    let (explode, sever) = chances(part, b, s);
    if explode_roll * 100.0 < explode {
        Some(Fate::Explode)
    } else if sever_roll * 100.0 < sever {
        Some(Fate::Sever)
    } else {
        None
    }
}

/// The parts that go with part `index` (it and every severable or explodable part sharing
/// its actor value), each with its own fate: an exploded part takes the others with it, and
/// only the Explodable ones among them explode [documented: GECK wiki KillActor notes].
pub fn parts_gone(data: &BodyPartData, index: usize, fate: Fate) -> Vec<(usize, Fate)> {
    let Some(hit) = data.parts.get(index) else { return Vec::new() };
    let mut out = vec![(index, fate)];
    for (i, p) in data.parts.iter().enumerate() {
        if i == index || !(p.severable() || p.explodable()) {
            continue;
        }
        if p.actor_value.is_some() && p.actor_value == hit.actor_value {
            out.push((i, if fate == Fate::Explode && p.explodable() { Fate::Explode } else { Fate::Sever }));
        }
    }
    out
}

/// The part KillActor's dismember argument names: the first severable or explodable part at
/// that location, and its fate (explode if Explodable, else sever) [hypothesis, §5].
pub fn scripted(data: &BodyPartData, location: i32) -> Option<(usize, Fate)> {
    let (i, p) = data.parts.iter().enumerate().find(|(_, p)| p.location() == location && (p.severable() || p.explodable()))?;
    Some((i, if p.explodable() { Fate::Explode } else { Fate::Sever }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(kind: &str, flags: &[&str], health: u32, explode: u32, av: &str) -> BodyPart {
        BodyPart {
            part_type: Some(kind.into()),
            flags: flags.iter().map(|s| s.to_string()).collect(),
            health_percent: health,
            explodable_chance_percent: explode,
            actor_value: Some(av.into()),
            ..BodyPart::default()
        }
    }

    fn head() -> BodyPart {
        part("Head 1", &["Severable", "Explodable"], 20, 30, "Perception Condition")
    }

    fn blow(on_hit: OnHit, overkill: f32) -> Blow {
        Blow { on_hit, crippled: true, overkill, max_health: 100.0, ..Blow::default() }
    }

    #[test]
    fn on_hit_labels() {
        assert_eq!(OnHit::from_label(Some("Explode Only")), OnHit::ExplodeOnly);
        assert_eq!(OnHit::from_label(Some("No Dismember/Explode")), OnHit::Neither);
        assert_eq!(OnHit::from_label(None), OnHit::Normal);
    }

    #[test]
    fn overkill_below_the_limb_share_never_explodes() {
        let s = Settings::default();
        // Head share of 100 health is 20.
        assert_eq!(chances(&head(), &blow(OnHit::ExplodeOnly, 19.0), &s), (0.0, 0.0));
        assert_eq!(chances(&head(), &blow(OnHit::ExplodeOnly, 20.0), &s).0, 30.0);
        assert_eq!(chances(&head(), &blow(OnHit::ExplodeOnly, 30.0), &s).0, 30.0 + 37.5);
        assert_eq!(chances(&head(), &blow(OnHit::ExplodeOnly, 400.0), &s).0, 100.0);
    }

    #[test]
    fn weapons_choose_what_they_may_do() {
        let s = Settings::default();
        assert_eq!(chances(&head(), &blow(OnHit::DismemberOnly, 400.0), &s), (0.0, 50.0));
        assert_eq!(chances(&head(), &blow(OnHit::Neither, 400.0), &s), (0.0, 0.0));
        assert_eq!(chances(&head(), &blow(OnHit::Normal, 5.0), &s), (0.0, 50.0));
    }

    #[test]
    fn intact_limbs_stay_unless_bloody_mess() {
        let s = Settings::default();
        let mut b = blow(OnHit::Normal, 25.0);
        b.crippled = false;
        assert_eq!(chances(&head(), &b, &s), (0.0, 0.0));
        b.bloody_mess = true;
        // 30 + 75 × 5/20 + 25 (intact bonus).
        assert_eq!(chances(&head(), &b, &s), (30.0 + 18.75 + 25.0, 15.0));
    }

    #[test]
    fn corpses_are_always_ready() {
        let s = Settings::default();
        let b = Blow { on_hit: OnHit::DismemberOnly, corpse: true, overkill: 10.0, max_health: 100.0, ..Blow::default() };
        assert_eq!(decide(&head(), &b, &s, 0.0, 0.49), Some(Fate::Sever));
        assert_eq!(decide(&head(), &b, &s, 0.0, 0.5), None);
    }

    #[test]
    fn torso_only_explodes() {
        let s = Settings::default();
        let torso = part("Torso", &["Explodable"], 60, 50, "Endurance Condition");
        assert_eq!(chances(&torso, &blow(OnHit::Normal, 59.0), &s), (0.0, 0.0));
        assert_eq!(chances(&torso, &blow(OnHit::Normal, 60.0), &s), (50.0, 0.0));
    }

    #[test]
    fn shared_actor_value_parts_go_together() {
        let brain = part("Brain", &["Severable"], 20, 0, "Perception Condition");
        let data = BodyPartData { edid: None, model: None, parts: vec![head(), part("Torso", &["Explodable"], 60, 50, "Endurance Condition"), brain], ragdoll: None };
        assert_eq!(parts_gone(&data, 0, Fate::Explode), vec![(0, Fate::Explode), (2, Fate::Sever)]);
        assert_eq!(scripted(&data, 1), Some((0, Fate::Explode)));
        assert_eq!(scripted(&data, 5), None);
    }
}
