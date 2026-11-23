//! The Anh NVSE plugin's functions on the game: armor rating, quest completion and
//! encounter zones [documented: each page in the wiki export; docs/spec-notes/ext-anh.md].
//! Anh is not installed in the probe game, so nothing here is observed. Changes last for
//! the session.

use crate::game::{Game, RunCtx};
use fnv_script::vm::XVal;
use fnv_sim::FormId;

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => 0.0,
    }
}

impl Game<'_> {
    /// An encounter zone's record (ECZN DATA) with saved `SetZoneRespawns`
    /// (`World::zone_respawns_override`) and session `SetZoneMinLevel`,
    /// `SetZoneOwner`, and `SetZoneFlag` (`nvse_lutana`) changes.
    pub(crate) fn zone(&self, z: FormId) -> Option<fnv_sim::forms::ZoneDef> {
        let mut d = self.world.effective_zone_def(z)?;
        if let Some(respawns) = self.world.zone_respawns_override(z) {
            d.never_resets = !respawns;
        }
        if let Some(c) = self.other.zone_changes.get(&z) {
            if let Some(o) = c.owner {
                d.owner = (o.0 != 0).then_some(o);
            }
            d.never_resets = c.never_resets.unwrap_or(d.never_resets);
        }
        Some(d)
    }
}

/// SetZoneOwner's and session SetZoneFlag bit-1 (`nvse_lutana`) changes to a zone
/// (`min_level` and `match_pc_below_min` live on `fnv_sim::World` / `WorldSession`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ZoneChange {
    pub owner: Option<FormId>,
    pub never_resets: Option<bool>,
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let v = match name {
        // `[armor]` and `newDR [armor]`: as GetArmorArmorRating and SetArmorArmorRating,
        // the argument else the calling reference's base [documented:
        // GetArmorArmorRatingAlt, alias GetArmorARAlt; SetArmorArmorRatingAlt, alias
        // SetArmorARAlt].
        "getarmorarmorratingalt" | "getarmoraralt" => return crate::nvse::call(g, ctx, "getarmorarmorrating", subject, args),
        "setarmorarmorratingalt" | "setarmoraralt" => return crate::nvse::call(g, ctx, "setarmorarmorrating", subject, args),
        // 1 when the quest is completed and not failed [documented: GetQuestCompletedAlt].
        "getquestcompletedalt" => {
            let s = form(args, 0).and_then(|q| g.quests.get(&q));
            XVal::Num(f64::from(u8::from(s.is_some_and(|s| s.completed && !s.failed))))
        }
        // [documented: GetZoneLevel, "the current level"]: the level the zone took when
        // its first cell loaded; 0 before then [hypothesis].
        "getzonelevel" => XVal::Num(form(args, 0).and_then(|z| g.world.zone_level(z)).map_or(0.0, f64::from)),
        // [documented: GetZoneMinLevel; GetZoneMatchLevel, "Match PC Below Minimum Level";
        // GetZoneOwner].
        "getzoneminlevel" => XVal::Num(form(args, 0).and_then(|z| g.zone(z)).map_or(0.0, |d| f64::from(d.min_level))),
        "getzonematchlevel" => XVal::Num(form(args, 0).and_then(|z| g.zone(z)).map_or(0.0, |d| f64::from(u8::from(d.match_pc_below_min)))),
        "getzoneowner" => XVal::Ref(form(args, 0).and_then(|z| g.zone(z)).and_then(|d| d.owner).map_or(0, |o| o.0)),
        // `zone level` and `zone owner` [documented: SetZoneMinLevel; SetZoneOwner]; for
        // the session, read by the getters and by `World::enter_zone` / `World::level_at`
        // (a zone already entered keeps its level [hypothesis]).
        "setzoneminlevel" | "setzoneowner" => {
            if let Some(z) = form(args, 0).filter(|z| g.world.forms.zones.contains_key(z)) {
                if name == "setzoneminlevel" {
                    g.world.set_zone_min_level(z, num(args, 1).clamp(f64::from(i8::MIN), f64::from(i8::MAX)) as i8);
                } else {
                    g.other.zone_changes.entry(z).or_default().owner = Some(form(args, 1).unwrap_or(FormId(0)));
                }
            }
            zero
        }
        _ => return None,
    };
    Some(Ok(v))
}
