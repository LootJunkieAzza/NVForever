//! Radiation from the environment: radiation markers and irradiated water giving the
//! player rads by the second, cut by Rad Resistance (docs/spec-notes/radiation.md).
//!
//! - **Markers** ([`RadMarker`]): placed references carrying XRAD (rads a second) and XRDS
//!   (the radius they reach) [documented: xEdit REFR; GECK wiki GetRadiationExtraData].
//!   In FalloutNV.esm all 119 are references of the static `RadiationMarker` [observed].
//!   Inside a marker's radius its whole level holds; where markers overlap the strongest
//!   counts [hypothesis: the masters put markers of rising level and falling radius on
//!   one spot (Clark Field 3/2000 and 720/10, the crater by Camp Searchlight 5/10000,
//!   10/6000, 18/2000), which builds a gradient only if each is flat and they do not
//!   add; radiation.md §2]. A disabled marker gives nothing [hypothesis].
//! - **Water** ([`WaterContact`], told by the engine): swimming or wading in a water whose
//!   actor effect (WATR XNAM, the effect of drinking it) gives rads gives that many rads a
//!   second times `fSwimRadiationDamageMult` or `fWadeRadiationDamageMult` (0.1 each in
//!   FalloutNV.esm) [observed: the settings and the effects; hypothesis: the rule].
//! - **The level** (GetRadiationLevel, the HUD's meter): markers plus water, before Rad
//!   Resistance [documented: GECK wiki GetRadiationLevel, "a radiation level of 3 would
//!   give you 3 rads/sec"].
//! - **Rads taken** a second: level × `fRadiationAccumulationRate` (1.0) × (1 − Rad
//!   Resistance ÷ 100), the resistance held to [`RESIST_CAP`] [hypothesis: the setting's
//!   name; resistance as a percentage, as Rad-X's +50 % reads]. They raise the Rads
//!   actor value; its stages (radiation sickness) follow in `crate::needs`.
//! - Only the player takes environment radiation here [design]; GetRadiationLevel on
//!   another actor is 0 [documented: GECK wiki GetRadiationLevelAlt, the vanilla function
//!   "would only return a value when called on the player"].

use crate::actor_values::RAD_RESISTANCE;
use crate::aid::{value_change, RAD_LEVEL};
use crate::combat::setting;
use crate::data::GameData;
use crate::functions::actor as fa;
use crate::game::Game;
use fnv_sim::refs::RefTable;
use fnv_sim::FormId;
use std::collections::HashMap;

/// The highest Rad Resistance that counts, percent [hypothesis: as Damage Resistance's
/// 85 % cap].
pub const RESIST_CAP: f64 = 85.0;

/// One radiation marker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadMarker {
    pub id: FormId,
    pub pos: [f32; 3],
    /// Rads a second inside the radius (XRAD).
    pub level: f32,
    /// Game units (XRDS; 0 when absent).
    pub radius: f32,
}

/// What the engine says of the player and water.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaterContact {
    /// The water type (WATR).
    pub water: FormId,
    /// Swimming; else wading (the feet under the surface).
    pub swimming: bool,
}

/// The player's exposure as the last frame measured it (not saved: the next frame
/// measures again).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Exposure {
    /// The water the player is in, as the engine told it.
    pub water: Option<WaterContact>,
    /// Rads a second from the markers, and the strongest marker covering the player.
    pub markers: f64,
    pub marker: Option<FormId>,
    /// Rads a second from the water.
    pub from_water: f64,
    /// The level (markers and water, before resistance): GetRadiationLevel.
    pub level: f64,
    /// Rads a second the player takes (after Rad Resistance).
    pub rate: f64,
    /// The Rad Resistance that cut it (percent, before the cap).
    pub resist: f64,
}

/// The markers of the placed references, by the space they are in (the worldspace of an
/// exterior cell, else the interior cell).
pub fn markers(refs: &RefTable) -> HashMap<FormId, Vec<RadMarker>> {
    let mut out: HashMap<FormId, Vec<RadMarker>> = HashMap::new();
    for (&id, p) in &refs.refs {
        let (Some(level), Some(cell)) = (p.radiation, p.cell) else { continue };
        if level <= 0.0 {
            continue;
        }
        let space = refs.world_of(cell).unwrap_or(cell);
        out.entry(space).or_default().push(RadMarker { id, pos: p.pos, level, radius: p.radius.unwrap_or(0.0).max(0.0) });
    }
    out
}

/// The level at `pos` from `markers` and the strongest marker covering it (the first in
/// the list on a tie): a marker counts within its radius (3-D distance), whole.
pub fn marker_level(markers: &[RadMarker], pos: [f32; 3], enabled: impl Fn(FormId) -> bool) -> (f64, Option<FormId>) {
    let mut best: (f64, Option<FormId>) = (0.0, None);
    for m in markers {
        let d2: f32 = (0..3).map(|i| (m.pos[i] - pos[i]).powi(2)).sum();
        if d2 <= m.radius * m.radius && (m.level as f64) > best.0 && enabled(m.id) {
            best = (m.level as f64, Some(m.id));
        }
    }
    best
}

/// Rads a second a water's actor effect gives when drunk: its value changes that raise
/// Rads ("Damage Radiation Level"), added up; 0 for a water without effect.
pub fn water_rads(data: &GameData, water: FormId) -> f64 {
    let Some(spell) = data.water_effects.get(&water) else { return 0.0 };
    let Some(items) = data.effect_items.get(spell) else { return 0.0 };
    items
        .iter()
        .filter_map(|e| value_change(data.magic_effects.get(&e.effect)?, e, 1.0, false))
        .filter(|c| c.code == RAD_LEVEL && c.amount < 0.0)
        .map(|c| -c.amount)
        .sum()
}

/// Rads a second taken at `level` with this Rad Resistance (percent).
pub fn taken(level: f64, resist: f64, accumulation: f64) -> f64 {
    (level * accumulation * (1.0 - resist.clamp(0.0, RESIST_CAP) / 100.0)).max(0.0)
}

impl Game<'_> {
    /// The engine tells what water the player is in (`None`: out of water).
    pub fn set_player_water(&mut self, water: Option<WaterContact>) {
        self.senses.radiation.water = water;
    }

    /// The player's exposure as last measured.
    pub fn radiation(&self) -> Exposure {
        self.senses.radiation
    }

    /// One frame of game-mode time: measure the player's exposure and add the rads.
    pub(crate) fn tick_radiation(&mut self, dt: f64) {
        let p = FormId::PLAYER_REF;
        let data = self.data;
        let place = self.world.peek(p).map(|s| (s.cell, s.pos)).or_else(|| self.world.placed.get(p).map(|r| (r.cell, r.pos)));
        let (markers, marker) = match place {
            Some((Some(cell), pos)) => {
                let space = self.world.placed.world_of(cell).unwrap_or(cell);
                let list = data.radiation_markers.get(&space).map(Vec::as_slice).unwrap_or_default();
                let world = &self.world;
                marker_level(list, pos, |id| world.is_enabled(id).unwrap_or(true))
            }
            _ => (0.0, None),
        };
        let water = self.senses.radiation.water;
        let from_water = water.map_or(0.0, |w| {
            let mult = if w.swimming { setting(self, "fSwimRadiationDamageMult", 0.1) } else { setting(self, "fWadeRadiationDamageMult", 0.1) };
            water_rads(data, w.water) * mult
        });
        let level = markers + from_water;
        let resist = if level > 0.0 { fa::actor_value(self, p, RAD_RESISTANCE).unwrap_or(0.0) } else { 0.0 };
        let rate = taken(level, resist, setting(self, "fRadiationAccumulationRate", 1.0));
        if rate > 0.0 && dt > 0.0 {
            self.shift_need(RAD_LEVEL, rate * dt);
        }
        self.senses.radiation = Exposure { water, markers, marker, from_water, level, rate, resist };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: u32, level: f32, radius: f32) -> RadMarker {
        RadMarker { id: FormId(id), pos: [0.0, 0.0, 0.0], level, radius }
    }

    #[test]
    fn a_marker_counts_whole_within_its_radius_and_the_strongest_wins() {
        // The Camp Searchlight crater's three markers on one spot.
        let list = [m(1, 5.0, 10000.0), m(2, 10.0, 6000.0), m(3, 18.0, 2000.0)];
        let all = |_| true;
        assert_eq!(marker_level(&list, [1000.0, 0.0, 0.0], all), (18.0, Some(FormId(3))));
        assert_eq!(marker_level(&list, [3000.0, 0.0, 0.0], all), (10.0, Some(FormId(2))));
        assert_eq!(marker_level(&list, [9999.0, 0.0, 0.0], all), (5.0, Some(FormId(1))));
        assert_eq!(marker_level(&list, [0.0, 0.0, 10001.0], all), (0.0, None));
        // A disabled marker gives nothing.
        assert_eq!(marker_level(&list, [1000.0, 0.0, 0.0], |id| id != FormId(3)), (10.0, Some(FormId(2))));
    }

    #[test]
    fn rad_resistance_cuts_the_rads_up_to_its_cap() {
        assert_eq!(taken(10.0, 0.0, 1.0), 10.0);
        // Endurance 5: (5 - 1) x 2 = 8 %.
        assert!((taken(10.0, 8.0, 1.0) - 9.2).abs() < 1e-9);
        // Rad-X's +50 % on top.
        assert!((taken(10.0, 58.0, 1.0) - 4.2).abs() < 1e-9);
        assert!((taken(10.0, 120.0, 1.0) - 1.5).abs() < 1e-9);
        assert_eq!(taken(0.0, 0.0, 1.0), 0.0);
    }
}
