//! Controller rumble (docs/design/controller.md §Rumble): the pad shakes when the player
//! fires a weapon (its record's rumble when it sets one, else harder for a heavier one),
//! takes damage, lands a hard fall, and when an
//! explosion goes off near the player.
//!
//! The original rumbles the pad, by its `bGamePadRumble` switch (`1` in the owner's
//! `FalloutPrefs.ini` [observed 2026-10-16]); how strongly and on what is not captured,
//! so every strength and length here is [design] (combat is not a parity target). Only
//! while a controller works the controls (`crate::bindings::InputDevice::Controller`) and
//! `Rumble` in `[Controller]` of `NVForever.ini` is on (`crate::bindings::Bindings::
//! rumble`, the Controls tab's last row with Controller chosen). Each rumble goes to every
//! connected controller as a Bevy `GamepadRumbleRequest` (gilrs plays it); the first of
//! each kind is logged (`rumble: first shot …`).

use crate::bindings::{Bindings, InputDevice};
use crate::combat::{ActorHit, Combat, ExplosionFx, ShotFired};
use bevy::input::gamepad::{GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy::prelude::*;
use fnv_sim::FormId;
use std::time::Duration;

/// What made the pad rumble.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Shot,
    Hurt,
    Landing,
    Explosion,
}

impl Kind {
    const ALL: [Kind; 4] = [Kind::Shot, Kind::Hurt, Kind::Landing, Kind::Explosion];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Shot => "shot",
            Kind::Hurt => "hurt",
            Kind::Landing => "hard landing",
            Kind::Explosion => "explosion",
        }
    }
}

/// One rumble: the strong (low, heavy) and weak (high, buzzing) motors from 0 to 1, for
/// `secs` seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pulse {
    pub strong: f32,
    pub weak: f32,
    pub secs: f32,
}

/// A weapon this heavy (WEAP DATA weight, pounds) or heavier kicks the most [design: the
/// minigun is 25, the Fat Man 30, a 9mm pistol 2, a hunting rifle 9].
pub const SHOT_HEAVY_WEIGHT: f32 = 20.0;

/// A shot of a weapon of `weight` [design]: a short buzz for a pistol, a longer thump
/// with the strong motor for a heavy weapon.
pub fn shot(weight: f32) -> Pulse {
    let h = (weight.max(0.0) / SHOT_HEAVY_WEIGHT).clamp(0.0, 1.0);
    Pulse { strong: 0.7 * h, weak: 0.3 + 0.4 * h, secs: 0.07 + 0.13 * h }
}

/// A weapon's own rumble (WEAP DNAM left and right motor strength and duration)
/// [documented: xEdit field names; hypothesis: the left motor is the strong one, as on the
/// Xbox 360 pad; docs/spec-notes/item-fields.md §1].
pub fn weapon_rumble((left, right, secs): (f32, f32, f32)) -> Pulse {
    Pulse { strong: left.clamp(0.0, 1.0), weak: right.clamp(0.0, 1.0), secs: secs.clamp(0.02, 1.0) }
}

/// A hit that took `share` of the player's maximum health [design: any hit is felt, a
/// quarter of the health or more the most].
pub fn hurt(share: f32) -> Pulse {
    let s = (share.max(0.0) * 4.0).clamp(0.0, 1.0);
    Pulse { strong: 0.35 + 0.65 * s, weak: 0.5, secs: 0.15 + 0.30 * s }
}

/// Slowest touchdown that rumbles, units/s [design: a jump lands at about 290
/// (`fnv_phys::air`), this is a drop of about 260 units under the engine's gravity of
/// 687 units/s²]; at [`LANDING_HARDEST`] (a drop of about 1050) the most.
pub const LANDING_HARD: f32 = 600.0;
pub const LANDING_HARDEST: f32 = 1200.0;

/// A landing at `speed` (downward, units/s): none below [`LANDING_HARD`] [design].
pub fn landing(speed: f32) -> Option<Pulse> {
    if speed < LANDING_HARD {
        return None;
    }
    let t = ((speed - LANDING_HARD) / (LANDING_HARDEST - LANDING_HARD)).clamp(0.0, 1.0);
    Some(Pulse { strong: 0.45 + 0.55 * t, weak: 0.3, secs: 0.15 + 0.25 * t })
}

/// How far past its own radius an explosion is felt, units [design: about 20 m].
pub const EXPLOSION_FELT: f32 = 1400.0;

/// An explosion of `radius` (EXPL DATA) `distance` units from the player: the most
/// inside its radius, fading to none [`EXPLOSION_FELT`] beyond it [design].
pub fn explosion(distance: f32, radius: f32) -> Option<Pulse> {
    let reach = radius.max(0.0) + EXPLOSION_FELT;
    let t = if distance <= radius { 1.0 } else { 1.0 - (distance - radius) / EXPLOSION_FELT };
    if t <= 0.05 || distance > reach {
        return None;
    }
    Some(Pulse { strong: t, weak: 0.6 * t, secs: 0.25 + 0.35 * t })
}

/// The player's character touched down this step at `speed` (downward, units/s); sent by
/// `crate::player`'s physics step.
#[derive(Message, Clone, Copy, Debug)]
pub struct Landed {
    pub speed: f32,
}

/// Whether the pad rumbles now: a controller works the controls with Rumble on.
pub fn wanted(b: &Bindings) -> bool {
    b.device == InputDevice::Controller && b.rumble
}

#[allow(clippy::too_many_arguments)]
fn rumble(
    bindings: Option<Res<Bindings>>,
    pads: Query<Entity, With<Gamepad>>,
    combat: Option<Res<Combat>>,
    player: Option<Res<crate::player::Player>>,
    (mut shots, mut hits, mut booms, mut landed): (MessageReader<ShotFired>, MessageReader<ActorHit>, MessageReader<ExplosionFx>, MessageReader<Landed>),
    mut out: MessageWriter<GamepadRumbleRequest>,
    mut logged: Local<[bool; 4]>,
) {
    let mut pulses: Vec<(Kind, Pulse)> = Vec::new();
    for s in shots.read().filter(|s| s.shooter == FormId::PLAYER_REF) {
        let w = combat.as_deref().and_then(|c| c.forms.weapons.get(&s.weapon));
        // The weapon's own rumble when its record sets one, else by its weight.
        let pulse = w.and_then(|w| w.rumble).map_or_else(|| shot(w.map_or(0.0, |w| w.weight)), weapon_rumble);
        pulses.push((Kind::Shot, pulse));
    }
    for h in hits.read().filter(|h| h.form_id == FormId::PLAYER_REF.0 && h.share > 0.0) {
        pulses.push((Kind::Hurt, hurt(h.share)));
    }
    for e in booms.read() {
        let Some(feet) = player.as_deref().map(|p| p.feet) else { continue };
        let radius = combat.as_deref().and_then(|c| c.forms.explosions.get(&e.explosion)).map_or(0.0, |x| x.radius);
        let d = Vec3::from(feet).distance(Vec3::from(e.point));
        if let Some(p) = explosion(d, radius) {
            pulses.push((Kind::Explosion, p));
        }
    }
    for l in landed.read() {
        if let Some(p) = landing(l.speed) {
            pulses.push((Kind::Landing, p));
        }
    }
    if pulses.is_empty() || !bindings.as_deref().is_some_and(wanted) {
        return;
    }
    // The strongest of each motor and the longest of this frame's pulses, as one.
    let one = pulses.iter().fold(Pulse { strong: 0.0, weak: 0.0, secs: 0.0 }, |a, (_, p)| Pulse { strong: a.strong.max(p.strong), weak: a.weak.max(p.weak), secs: a.secs.max(p.secs) });
    for (kind, p) in &pulses {
        let i = Kind::ALL.iter().position(|k| k == kind).unwrap_or(0);
        if !std::mem::replace(&mut logged[i], true) {
            info!("rumble: first {} (strong {:.2}, weak {:.2}, {:.2} s) on {} controller(s)", kind.name(), p.strong, p.weak, p.secs, pads.iter().count());
        }
    }
    let intensity = GamepadRumbleIntensity { strong_motor: one.strong.clamp(0.0, 1.0), weak_motor: one.weak.clamp(0.0, 1.0) };
    for gamepad in &pads {
        out.write(GamepadRumbleRequest::Add { duration: Duration::from_secs_f32(one.secs), intensity, gamepad });
    }
}

pub struct RumblePlugin;

impl Plugin for RumblePlugin {
    fn build(&self, app: &mut App) {
        // Bevy's gilrs plugin adds the requests where it runs; headless runs and tests
        // have none.
        app.add_message::<GamepadRumbleRequest>()
            .add_message::<Landed>()
            .add_message::<ShotFired>()
            .add_message::<ActorHit>()
            .add_message::<ExplosionFx>()
            .add_systems(Update, rumble);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weapon_rumble_from_the_record() {
        let p = weapon_rumble((0.8, 0.4, 0.25));
        assert_eq!((p.strong, p.weak, p.secs), (0.8, 0.4, 0.25));
        // Out of range values are held to what the pad takes.
        let p = weapon_rumble((2.0, -1.0, 0.0));
        assert_eq!((p.strong, p.weak, p.secs), (1.0, 0.0, 0.02));
    }

    #[test]
    fn heavier_weapons_kick_harder() {
        let (pistol, rifle, minigun) = (shot(2.0), shot(9.0), shot(25.0));
        assert!(pistol.strong < rifle.strong && rifle.strong < minigun.strong);
        assert!(pistol.secs < minigun.secs);
        assert_eq!(minigun, shot(SHOT_HEAVY_WEIGHT));
        assert!(pistol.weak > 0.0);
    }

    #[test]
    fn soft_landings_and_far_blasts_do_not_rumble() {
        assert_eq!(landing(290.0), None, "a jump");
        assert!(landing(LANDING_HARD).is_some());
        assert!(landing(2000.0).unwrap().strong <= 1.0);
        assert_eq!(explosion(5000.0, 300.0), None);
        assert_eq!(explosion(100.0, 300.0).map(|p| p.strong), Some(1.0));
        assert!(explosion(1000.0, 300.0).unwrap().strong < 1.0);
    }

    #[test]
    fn only_with_the_controller_and_rumble_on() {
        let mut app = App::new();
        app.init_resource::<Bindings>().add_plugins(RumblePlugin);
        let pad = app.world_mut().spawn(Gamepad::default()).id();
        let sent = |app: &mut App| {
            app.world_mut().write_message(ShotFired { projectile: FormId(1), weapon: FormId(2), shooter: FormId::PLAYER_REF, at: None, dir: None });
            app.update();
            let m = app.world().resource::<Messages<GamepadRumbleRequest>>();
            m.iter_current_update_messages().filter(|r| r.gamepad() == pad).count()
        };
        assert_eq!(sent(&mut app), 0, "keyboard and mouse");
        app.world_mut().resource_mut::<Bindings>().device = InputDevice::Controller;
        assert_eq!(sent(&mut app), 1);
        app.world_mut().resource_mut::<Bindings>().rumble = false;
        assert_eq!(sent(&mut app), 0, "Rumble off");
    }
}
