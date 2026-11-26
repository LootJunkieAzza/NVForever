//! Per-shot / per-mine non-flag session `PROJ` overrides applied onto loaded
//! [`ProjectileStats`], [`ShotChain`], and [`ProjectileFx`] without mutating the shared
//! [`crate::data::CombatForms`] or [`crate::fx::CombatFx`] caches.
//!
//! `DATA/flags` (`ProjectileStats::flags` / `ProjectileFx::supersonic`) is intentionally
//! excluded here (owned separately by projectile flag helpers).

use crate::data::{ProjectileStats, ShotChain};
use crate::fx::ProjectileFx;
use fnv_sim::FormId;

/// Sparse non-flag session overrides for a `PROJ` record (`SetProjectileTraitNumeric`,
/// `SetProjectileExplosion`, `SetProjectileMuzzleFlash`, `SetProjectileSound`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProjectileOverrides {
    pub kind: Option<Option<&'static str>>,
    pub gravity: Option<f32>,
    pub speed: Option<f32>,
    pub range: Option<f32>,
    pub tracer_chance: Option<f32>,
    pub alt_trigger_proximity: Option<f32>,
    pub alt_trigger_timer: Option<f32>,
    pub muzzle_flash_duration: Option<f32>,
    pub fade_duration: Option<f32>,
    pub impact_force: Option<f32>,
    pub bouncy_mult: Option<f32>,
    pub sound_level: Option<Option<&'static str>>,
    pub explosion: Option<Option<FormId>>,
    pub muzzle_flash_model: Option<Option<String>>,
    pub sound: Option<Option<FormId>>,
    pub sound_countdown: Option<Option<FormId>>,
    pub sound_disable: Option<Option<FormId>>,
}

impl ProjectileOverrides {
    /// Whether no non-flag override is present.
    pub fn is_empty(&self) -> bool {
        self.kind.is_none()
            && self.gravity.is_none()
            && self.speed.is_none()
            && self.range.is_none()
            && self.tracer_chance.is_none()
            && self.alt_trigger_proximity.is_none()
            && self.alt_trigger_timer.is_none()
            && self.muzzle_flash_duration.is_none()
            && self.fade_duration.is_none()
            && self.impact_force.is_none()
            && self.bouncy_mult.is_none()
            && self.sound_level.is_none()
            && self.explosion.is_none()
            && self.muzzle_flash_model.is_none()
            && self.sound.is_none()
            && self.sound_countdown.is_none()
            && self.sound_disable.is_none()
    }

    /// Return a cloned [`ProjectileStats`] with every present, finite non-flag override
    /// applied. `base.flags` and `base.muzzle_flash_light` / `base.default_weapon` stay
    /// untouched.
    pub fn apply_stats(&self, base: &ProjectileStats) -> ProjectileStats {
        let mut out = base.clone();
        if let Some(kind) = self.kind {
            out.kind = kind.map(str::to_string);
        }
        if let Some(v) = self.gravity.filter(|v| v.is_finite()) {
            out.gravity = v;
        }
        if let Some(v) = self.speed.filter(|v| v.is_finite()) {
            out.speed = v;
        }
        if let Some(v) = self.range.filter(|v| v.is_finite()) {
            out.range = v;
        }
        if let Some(v) = self.tracer_chance.filter(|v| v.is_finite()) {
            out.tracer_chance = v;
        }
        if let Some(v) = self.alt_trigger_proximity.filter(|v| v.is_finite()) {
            out.alt_trigger_proximity = v;
        }
        if let Some(v) = self.alt_trigger_timer.filter(|v| v.is_finite()) {
            out.alt_trigger_timer = v;
        }
        if let Some(v) = self.muzzle_flash_duration.filter(|v| v.is_finite()) {
            out.muzzle_flash_duration = v;
        }
        if let Some(v) = self.fade_duration.filter(|v| v.is_finite()) {
            out.fade_duration = v;
        }
        if let Some(v) = self.impact_force.filter(|v| v.is_finite()) {
            out.impact_force = v;
        }
        if let Some(v) = self.bouncy_mult.filter(|v| v.is_finite()) {
            out.bouncy_mult = v;
        }
        if let Some(level) = self.sound_level {
            out.sound_level = level.map(str::to_string);
        }
        if let Some(explosion) = self.explosion {
            out.explosion = explosion;
        }
        if let Some(model) = &self.muzzle_flash_model {
            out.muzzle_flash_model = model.clone();
        }
        out
    }

    /// Recompute `ShotChain::explosion` from the effective [`ProjectileStats`] (honoring
    /// its `"Explosion"` flag via [`ProjectileStats::explodes`]).
    pub fn apply_chain(&self, base_chain: &ShotChain, effective_stats: &ProjectileStats) -> ShotChain {
        let mut out = base_chain.clone();
        out.explosion = effective_stats.explodes();
        out
    }

    /// Return a cloned [`ProjectileFx`] with present `SetProjectileSound` overrides
    /// (`sound`, `sound_countdown`, `sound_disable`) applied; `base.supersonic` stays
    /// untouched.
    pub fn apply_fx(&self, base: &ProjectileFx) -> ProjectileFx {
        let mut out = base.clone();
        if let Some(sound) = self.sound {
            out.sound = sound;
        }
        if let Some(countdown) = self.sound_countdown {
            out.sound_countdown = countdown;
        }
        if let Some(disable) = self.sound_disable {
            out.sound_disable = disable;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mine::{is_mine, MineRules};
    use crate::projectile::Motion;
    use crate::shot::fire_projectile;
    use crate::tuning::CombatTuning;

    fn base_proj() -> ProjectileStats {
        ProjectileStats {
            edid: Some("OwnTestProj".into()),
            kind: Some("Missile".into()),
            flags: vec!["Explosion".into(), "Alt. Trigger".into()],
            gravity: 0.5,
            speed: 1000.0,
            range: 5000.0,
            tracer_chance: 0.1,
            impact_force: 6.0,
            fade_duration: 0.5,
            muzzle_flash_duration: 0.04,
            muzzle_flash_model: Some("effects\\flash_base.nif".into()),
            explosion: Some(FormId(0x100)),
            alt_trigger_timer: 3.0,
            alt_trigger_proximity: 0.0,
            bouncy_mult: 0.5,
            sound_level: Some("Normal".into()),
            ..Default::default()
        }
    }

    #[test]
    fn overrides_flow_into_spawn_shot_chain_mines_and_fx_without_mutating_base() {
        let base = base_proj();
        let base_chain = ShotChain {
            weapon: FormId(0x10),
            ammo_choices: vec![FormId(0x20)],
            ammo: Some(FormId(0x20)),
            projectile: Some(FormId(0x30)),
            projectile_from: Some("ammo"),
            projectiles_per_shot: 1,
            damage_per_projectile: 25.0,
            explosion: base.explodes(),
            ammo_effects: Vec::new(),
        };
        let base_fx = ProjectileFx {
            sound: Some(FormId(0x200)),
            sound_countdown: Some(FormId(0x201)),
            sound_disable: Some(FormId(0x202)),
            supersonic: true,
        };

        let ov = ProjectileOverrides {
            kind: Some(Some("Lobber")),
            gravity: Some(2.0),
            speed: Some(450.0),
            range: Some(1200.0),
            tracer_chance: Some(0.75),
            alt_trigger_proximity: Some(150.0),
            alt_trigger_timer: Some(5.0),
            muzzle_flash_duration: Some(0.25),
            fade_duration: Some(1.25),
            impact_force: Some(18.0),
            bouncy_mult: Some(1.5),
            sound_level: Some(Some("Silent")),
            explosion: Some(Some(FormId(0x101))),
            muzzle_flash_model: Some(Some("effects\\flash_override.nif".into())),
            sound: Some(Some(FormId(0x210))),
            sound_countdown: Some(Some(FormId(0x211))),
            sound_disable: Some(Some(FormId(0x212))),
        };
        assert!(!ov.is_empty());

        let eff = ov.apply_stats(&base);
        let eff_chain = ov.apply_chain(&base_chain, &eff);
        let eff_fx = ov.apply_fx(&base_fx);

        // Flags and base records stay untouched.
        assert_eq!(eff.flags, base.flags);
        assert_eq!(base.speed, 1000.0);
        assert_eq!(base.explosion, Some(FormId(0x100)));
        assert_eq!(eff.kind.as_deref(), Some("Lobber"));
        assert_eq!(eff.tracer_chance, 0.75);
        assert_eq!(eff.muzzle_flash_duration, 0.25);
        assert_eq!(eff.fade_duration, 1.25);
        assert_eq!(eff.impact_force, 18.0);
        assert_eq!(eff.sound_level.as_deref(), Some("Silent"));
        assert_eq!(eff.muzzle_flash_model.as_deref(), Some("effects\\flash_override.nif"));
        assert_eq!(eff_chain.explosion, Some(FormId(0x101)));

        // Combat shot/spawn consumer observes speed, gravity, range, explosion, timer, proximity, bounce, kind, mine.
        let tuning = CombatTuning::default();
        let mut r = || 0.5_f32;
        let shots = fire_projectile(&eff, &eff_chain, [0.0; 3], [0.0, 1.0, 0.0], 0.0, None, 386.0, &tuning, &mut r);
        assert_eq!(shots.len(), 1);
        let s = &shots[0];
        assert_eq!(s.motion, Motion::Lobber);
        assert_eq!(s.speed, 450.0);
        assert_eq!(s.gravity, 2.0 * 386.0);
        assert_eq!(s.range, 1200.0);
        assert_eq!(s.explosion, Some(FormId(0x101)));
        assert_eq!(s.timer, 5.0);
        assert_eq!(s.proximity, 150.0);
        assert!((s.bounce - tuning.default_bounce * 1.5).abs() < 1e-6);
        assert!(s.mine);

        // Mine rules consume live kind, proximity, and timer.
        assert!(!is_mine(&base));
        assert!(is_mine(&eff));
        let rules = MineRules::default();
        assert!((rules.trigger_radius(&eff, true) - 150.0 * rules.exterior_radius_mult).abs() < 1e-4);
        assert!((rules.fuse(&eff, 100.0) - 5.0).abs() < 1e-4);

        // ProjectileFx consumes all three SetProjectileSound slots while keeping supersonic flag.
        assert_eq!(eff_fx.sound, Some(FormId(0x210)));
        assert_eq!(eff_fx.sound_countdown, Some(FormId(0x211)));
        assert_eq!(eff_fx.sound_disable, Some(FormId(0x212)));
        assert!(eff_fx.supersonic);
        assert_eq!(base_fx.sound, Some(FormId(0x200)));
    }

    #[test]
    fn non_finite_numeric_overrides_preserve_authored_values_and_explosion_respects_flag() {
        let mut base = base_proj();
        base.flags.clear(); // No "Explosion" flag.
        let base_chain = ShotChain {
            weapon: FormId(0x10),
            ammo_choices: Vec::new(),
            ammo: None,
            projectile: Some(FormId(0x30)),
            projectile_from: Some("weapon"),
            projectiles_per_shot: 1,
            damage_per_projectile: 25.0,
            explosion: None,
            ammo_effects: Vec::new(),
        };
        let ov = ProjectileOverrides {
            speed: Some(f32::NAN),
            gravity: Some(f32::INFINITY),
            explosion: Some(Some(FormId(0x101))),
            ..Default::default()
        };
        let eff = ov.apply_stats(&base);
        let eff_chain = ov.apply_chain(&base_chain, &eff);
        assert_eq!(eff.speed, 1000.0);
        assert_eq!(eff.gravity, 0.5);
        assert_eq!(eff.explosion, Some(FormId(0x101)));
        assert_eq!(eff_chain.explosion, None, "Explosion flag stays gated by ProjectileStats::explodes");
    }
}
