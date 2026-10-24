//! Area effects (docs/spec-notes/area-effects.md): an effect item whose EFIT "area" is
//! above 0 reaches every actor within that radius of where it lands, not only its
//! target.
//!
//! - **Where it lands.** A spell cast on oneself (CastImmediateOnSelf) lands on the
//!   caster; a weapon's object effect lands on the actor hit. Both reach the actors
//!   around that one ([`Game::spread_area`]); the one it lands on has it already. A
//!   blast's object effect does not spread: the explosion's own radius picks those it
//!   catches (`nvf_play::combat`) [hypothesis: EuclidExplosionEnchantment's area, 1024,
//!   would reach far past any blast].
//! - **Radius.** The area is in feet, [`AREA_UNIT`] game units each [observed: capture
//!   esmfx1, area-effects.md §4: LegionExplorerAura's 50 reached an actor 899 units away
//!   and not one at 1,170].
//! - **Who.** Loaded, present actors in the same cell (interior) or worldspace, the dead
//!   left out, each within the radius of the centre (straight-line distance). The caster
//!   is reached too when it is not the centre; a spell cast on oneself lands on the caster
//!   [observed: esmfx1, IsSpellTarget 1 on the player after its own CIOS].
//! - **Line of sight.** Unless the spell has "Area Effect Ignores LOS" (SPIT flag 4)
//!   [documented: xEdit SPEL], an actor the centre is known not to see clearly is left
//!   out (the engine's detection measures, `crate::detection`); an actor never measured
//!   counts as in sight [hypothesis]. Object effects (ENCH) have no such flag and
//!   ignore it [hypothesis].
//! - Log: `area effect: <item> around <centre>: <n> actor(s) reached (<names>)`.
//! - Each reached actor gets the item's area effects only (those with an area), applied
//!   as on the centre: a hit's for a weapon, a spell's for a cast.

use crate::game::Game;
use fnv_sim::FormId;

/// Game units per unit of an EFIT area: a foot of 64 / 3 units [observed: capture
/// esmfx1, see the module notes].
pub const AREA_UNIT: f32 = 64.0 / 3.0;
/// SPEL SPIT flag bit 4, "Area Effect Ignores LOS" [documented: xEdit wbDefinitionsFNV.pas
/// SPEL].
pub const SPELL_AREA_IGNORES_LOS: u8 = 1 << 4;

/// The radius in game units of an EFIT area.
pub fn area_radius(area: u32) -> f32 {
    area as f32 * AREA_UNIT
}

impl Game<'_> {
    /// A reference's cell and position, as the world has them now.
    fn area_place(&self, r: FormId) -> Option<(FormId, [f32; 3])> {
        match self.world.peek(r) {
            Some(s) => Some((s.cell?, s.pos)),
            None => self.world.placed.get(r).and_then(|p| Some((p.cell?, p.pos))),
        }
    }

    /// The actors item `item`'s area effects reach around `center`: loaded, present and
    /// alive, in the same cell or worldspace, within the largest area's radius, in sight
    /// unless the spell ignores sight; `center` itself left out. With each, the effect
    /// indices whose own radius reaches it.
    pub fn area_targets(&self, center: FormId, item: FormId) -> Vec<(FormId, Vec<usize>)> {
        let Some(items) = self.data.effect_items.get(&item) else { return Vec::new() };
        let radii: Vec<(usize, f32)> = items.iter().enumerate().filter(|(_, e)| e.area > 0).map(|(i, e)| (i, area_radius(e.area))).collect();
        if radii.is_empty() {
            return Vec::new();
        }
        let Some((cell, at)) = self.area_place(center) else { return Vec::new() };
        let placed = self.world.placed;
        let space = |c: FormId| placed.world_of(c).unwrap_or(c);
        let ignores_los = !self.data.spell_types.contains_key(&item) || self.data.spell_flags.get(&item).is_some_and(|f| f & SPELL_AREA_IGNORES_LOS != 0);
        let mut actors = self.package_actors();
        actors.insert(FormId::PLAYER_REF);
        let mut out = Vec::new();
        for a in actors {
            if a == center || !self.effects_loaded(a) || self.is_dead(a) {
                continue;
            }
            let Some((c, p)) = self.area_place(a) else { continue };
            if space(c) != space(cell) {
                continue;
            }
            let d = (0..3).map(|i| (p[i] - at[i]).powi(2)).sum::<f32>().sqrt();
            let reached: Vec<usize> = radii.iter().filter(|(_, r)| d <= *r).map(|(i, _)| *i).collect();
            if reached.is_empty() {
                continue;
            }
            if !ignores_los && !self.area_in_sight(center, a) {
                continue;
            }
            out.push((a, reached));
        }
        out
    }

    /// Whether `target` is in sight of `center` for an area effect: false only when the
    /// engine's detection measured the pair and found no clear line (either way round).
    fn area_in_sight(&self, center: FormId, target: FormId) -> bool {
        let clear = |a: FormId, b: FormId| self.detection.get(a, b).map(|d| d.clear);
        !matches!((clear(center, target), clear(target, center)), (Some(false), None) | (None, Some(false)) | (Some(false), Some(false)))
    }

    /// Spread `item`'s area effects from `center` to the actors around it
    /// ([`Game::area_targets`]); `hit`: applied as a hit's effects (a weapon's), else as a
    /// spell's. Returns the actors reached and the effects started on each.
    pub fn spread_area(&mut self, center: FormId, item: FormId, caster: Option<FormId>, hit: bool) -> Vec<(FormId, Vec<u32>)> {
        let mut out = Vec::new();
        for (target, indices) in self.area_targets(center, item) {
            if hit && self.is_immune(target) {
                continue;
            }
            let mut ids = Vec::new();
            for index in indices {
                let mods = crate::aid::Mods { hit, only: Some(index), ..crate::aid::Mods::default() };
                ids.extend(self.apply_magic_item_with(target, item, caster, false, &mods).into_iter().filter(|a| !a.extended).map(|a| a.id));
            }
            out.push((target, ids));
        }
        if !out.is_empty() {
            let who = |r: FormId| if r == FormId::PLAYER_REF { "the player".to_string() } else { format!("{} {r}", self.data.name(r)) };
            let names: Vec<String> = out.iter().map(|(r, _)| who(*r)).collect();
            tracing::info!("area effect: {} around {}: {} actor(s) reached ({})", self.data.name(item), who(center), out.len(), names.join(", "));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn areas_in_feet() {
        // LegionExplorerAura's 50: about 1,067 units.
        assert!((area_radius(50) - 1066.667).abs() < 0.01);
        assert_eq!(area_radius(0), 0.0);
        assert_eq!(SPELL_AREA_IGNORES_LOS, 0x10);
    }
}
