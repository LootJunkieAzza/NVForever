//! Reputations (fame and infamy per `REPU`), docs/spec-notes/scripts.md §7.6.
//!
//! The player holds infamy (axis 0) and fame (axis 1) per reputation
//! ([`crate::game::PlayerState::reputation`]) [documented: GECK wiki GetReputation].
//! A reputation's `DATA` value is its scale, the most fame or infamy it counts
//! [documented: GECK wiki GetReputationPct; schema `gameplay.json` REPU].

use super::{none, Args, Ctx};
use crate::game::Game;
use crate::trace::EventKind;
use fnv_script::vm::Value;
use fnv_sim::FormId;

/// Points per deed size 1–5 for AddReputation / RemoveReputation: the game settings
/// fReputationBumpVeryMinor … fReputationBumpVeryMajor when a plugin defines them,
/// else their defaults 1, 2, 4, 7, 12 [documented: GECK wiki AddReputation]. The
/// shipped masters define none of the five [observed: FalloutNV.esm GMST list].
const BUMPS: [(&str, f64); 5] = [
    ("fReputationBumpVeryMinor", 1.0),
    ("fReputationBumpMinor", 2.0),
    ("fReputationBumpAverage", 4.0),
    ("fReputationBumpMajor", 7.0),
    ("fReputationBumpVeryMajor", 12.0),
];

/// Fractions of the scale at which fame or infamy reaches levels 1, 2 and 3
/// [hypothesis: not documented anywhere we can read; to be captured (docs/spec-notes/
/// scripts.md §10)].
pub const LEVEL_FRACTIONS: [f64; 3] = [0.25, 0.5, 0.75];

fn rep_axis(a: &Args, i: usize) -> Result<usize, String> {
    match a.int(i)? {
        0 => Ok(0),
        1 => Ok(1),
        o => Err(format!("reputation axis {o} is not 0 (infamy) or 1 (fame)")),
    }
}

fn reputation(g: &Game, a: &Args) -> Result<FormId, String> {
    let r = a.form(0)?;
    if g.data.reputations.contains_key(&r) || g.data.sig(r).as_ref() == Some(b"REPU") {
        Ok(r)
    } else {
        Err(format!("{} is not a reputation", g.data.name(r)))
    }
}

/// The scale of a reputation (its `DATA`); 0 when the record has none.
pub fn scale(g: &Game, r: FormId) -> f64 {
    g.data.reputations.get(&r).copied().unwrap_or(0.0) as f64
}

/// Infamy and fame.
pub fn values(g: &Game, r: FormId) -> [f64; 2] {
    g.player.reputation.get(&r).copied().unwrap_or([0.0; 2])
}

/// Change one axis; the result is not allowed below 0 [hypothesis] and is traced.
pub fn change(g: &mut Game, r: FormId, axis: usize, delta: f64) {
    let v = g.player.reputation.entry(r).or_default();
    v[axis] = (v[axis] + delta).max(0.0);
    let value = v[axis];
    g.emit(EventKind::ReputationChanged { reputation: r, axis, value });
}

fn bump(g: &Game, size: i32) -> Result<f64, String> {
    let (name, default) = *BUMPS.get((size - 1) as usize).ok_or_else(|| format!("deed size {size} is not 1 to 5"))?;
    Ok(g.world.forms.setting_f32(name).map(|v| v as f64).unwrap_or(default))
}

/// `AddReputation rep axis size`: fame or infamy grows by the deed size's points
/// [documented: GECK wiki AddReputation].
pub fn add_reputation(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = reputation(g, a)?;
    let axis = rep_axis(a, 1)?;
    let p = bump(g, a.int(2)?)?;
    change(g, r, axis, p);
    none()
}

/// `RemoveReputation rep axis size`: the same amount taken away [documented: GECK wiki
/// RemoveReputation].
pub fn remove_reputation(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = reputation(g, a)?;
    let axis = rep_axis(a, 1)?;
    let p = bump(g, a.int(2)?)?;
    change(g, r, axis, -p);
    none()
}

/// `AddReputationExact rep axis amount` [documented: GECK wiki AddReputationExact].
pub fn add_reputation_exact(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = reputation(g, a)?;
    let axis = rep_axis(a, 1)?;
    let p = a.num(2)?;
    change(g, r, axis, p);
    none()
}

/// Fame or infamy over the scale [documented: GECK wiki GetReputationPct], capped at 1
/// [hypothesis].
pub fn get_reputation_pct(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = reputation(g, a)?;
    let axis = rep_axis(a, 1)?;
    let s = scale(g, r);
    let v = values(g, r)[axis];
    Ok(Value::Num(if s > 0.0 { (v / s).min(1.0) } else { 0.0 }))
}

/// The level (0–3) of fame or infamy: how many of [`LEVEL_FRACTIONS`] it reaches.
pub fn level(value: f64, scale: f64) -> usize {
    if scale <= 0.0 {
        return 0;
    }
    LEVEL_FRACTIONS.iter().filter(|&&f| value / scale + 1e-9 >= f).count()
}

/// The tier on each axis (mixed, good, bad) for fame level `f` and infamy level `i`.
///
/// Documented [GECK wiki GetReputationThreshold]: values 0–6; with no fame and no
/// infamy every axis reads 1 (Neutral); the player is on one axis at a time and the
/// others read 0; the good axis lists Idolized 6, Liked 5, Accepted 4, Good Natured
/// Rascal 3, Smiling Troublemaker 2; the bad axis Vilified 6, Hated 5, Shunned 4,
/// Merciful Thug 3, Sneering Punk 2; the mixed axis Wild Child 5, Unpredictable 4,
/// Mixed 3, Dark Hero or Soft Hearted Devil 2.
///
/// The grid that places each name at a fame and infamy level is a [hypothesis]: fame
/// only (levels 1–3) gives Accepted, Liked, Idolized; infamy only Shunned, Hated,
/// Vilified; equal levels Mixed, Unpredictable, Wild Child; fame 2 or 3 over infamy 1
/// gives Smiling Troublemaker, Good Natured Rascal; infamy 2 or 3 over fame 1 Sneering
/// Punk, Merciful Thug; fame 3 with infamy 2 and fame 2 with infamy 3 the two names of
/// mixed 2. It fills the wiki's three axes exactly, which is why it is the working rule.
pub fn tiers(f: usize, i: usize) -> [i32; 3] {
    match (f, i) {
        (0, 0) => [1, 1, 1],
        (f, 0) => [0, 3 + f as i32, 0],
        (0, i) => [0, 0, 3 + i as i32],
        (f, i) if f == i => [2 + f as i32, 0, 0],
        (3, 2) | (2, 3) => [2, 0, 0],
        (f, 1) => [0, f as i32, 0],
        (1, i) => [0, 0, i as i32],
        _ => [0, 0, 0],
    }
}

/// `GetReputationThreshold rep axis` (0 mixed, 1 good, 2 bad): see [`tiers`].
pub fn get_reputation_threshold(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = reputation(g, a)?;
    let axis = a.int(1)?;
    if !(0..=2).contains(&axis) {
        return Err(format!("reputation threshold axis {axis} is not 0, 1 or 2"));
    }
    let s = scale(g, r);
    let [infamy, fame] = values(g, r);
    let t = tiers(level(fame, s), level(infamy, s));
    Ok(Value::Num(t[axis as usize] as f64))
}
