//! Activation diagnostics [design, #2528; docs/spec-notes/cells-refs.md §5].
//! `NVF_PLAY_TEST_ACTIVATION_SWEEP=<target hex>|<x>:<y>:<z>:<heading deg>,...`
//! sweeps three yaws at pitch -60 degrees at each supplied standing spot, on the world
//! aids' clock. It logs every look, then replays the first successful view and presses
//! E through AutoActivate. It never queues activation of an explicit reference.

use crate::activate::{AutoActivate, Target};
use crate::pause::AidTime;
use crate::phys::PhysState;
use crate::player::Player;
use crate::reticle::Prompt;
use crate::sim::SimState;
use crate::{PlaySet, PlayState};
use bevy::prelude::*;
use fnv_sim::FormId;

// Sweep coverage and settling times [design, #2528]; not original-game rules.
const YAWS: [f32; 3] = [-30.0, 0.0, 30.0];
const PITCHES: [f32; 1] = [-60.0];
const SETTLE: f32 = 0.5;
const DOOR_RADIUS: f32 = 400.0;
const ENV: &str = "NVF_PLAY_TEST_ACTIVATION_SWEEP";

/// Orders the diagnostic after the prompt's drawing pass [design: reticle.rs::draw].
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Draw;

#[derive(Clone, Copy, Debug, PartialEq)]
struct View {
    feet: [f32; 3],
    heading: f32,
    pitch: f32,
}

#[derive(Resource)]
pub(crate) struct ActivationSweep {
    target: FormId,
    views: Vec<View>,
    next: usize,
    active: Option<(usize, f32)>,
    next_at: f32,
    first: Option<View>,
    replayed: bool,
    pub(crate) drawn: Option<Prompt>,
}

impl ActivationSweep {
    fn parse(value: &str) -> Result<Self, String> {
        let (target, spots) = value.split_once('|').ok_or("expected <target hex>|<x>:<y>:<z>:<heading>,...")?;
        let target = u32::from_str_radix(target.trim().trim_start_matches("0x"), 16).map_err(|_| "bad target hex")?;
        if target == 0 {
            return Err("target must be nonzero".into());
        }
        let mut views = Vec::new();
        for spot in spots.split(',') {
            let values = spot.split(':').map(|s| s.trim().parse::<f32>()).collect::<Result<Vec<_>, _>>().map_err(|_| "bad spot number")?;
            let [x, y, z, heading] = values.as_slice() else { return Err("expected x:y:z:heading".into()) };
            if !values.iter().all(|v| v.is_finite()) {
                return Err("spot numbers must be finite".into());
            }
            for yaw in YAWS {
                for pitch in PITCHES {
                    views.push(View { feet: [*x, *y, *z], heading: (heading + yaw).to_radians(), pitch: pitch.to_radians() });
                }
            }
        }
        Ok(Self { target: FormId(target), views, next: 0, active: None, next_at: 4.0, first: None, replayed: false, drawn: None })
    }

    fn remember(&mut self, target: Option<u32>, view: View) {
        if target == Some(self.target.0) && self.first.is_none() {
            self.first = Some(view);
        }
    }
}

pub(super) fn setup(app: &mut App) {
    let Ok(value) = std::env::var(ENV) else { return };
    match ActivationSweep::parse(&value) {
        Ok(sweep) => {
            app.insert_resource(sweep)
                .add_systems(Update, prepare.in_set(PlaySet::Input).after(crate::control::read_controls).run_if(in_state(PlayState::Playing)))
                .add_systems(Update, sample.in_set(PlaySet::Ui).after(Draw).run_if(in_state(PlayState::Playing)));
        }
        Err(e) => warn!("{ENV}: {e}"),
    }
}

fn prepare(time: AidTime, mut sweep: ResMut<ActivationSweep>, mut player: ResMut<Player>, mut phys: ResMut<PhysState>, mut auto: ResMut<AutoActivate>) {
    let Some(now) = time.world_secs() else { return };
    if sweep.replayed || sweep.active.is_some() || now < sweep.next_at || phys.player.is_none() {
        return;
    }
    if let Some(view) = sweep.views.get(sweep.next).copied() {
        if sweep.next.is_multiple_of(YAWS.len() * PITCHES.len()) {
            phys.place_player(view.feet, 60.0, 1000.0);
            player.feet = phys.player.as_ref().unwrap().feet;
        }
        player.heading = view.heading;
        player.pitch = view.pitch;
        sweep.active = Some((sweep.next, now));
        sweep.next += 1;
    } else {
        sweep.replayed = true;
        if let Some(view) = sweep.first {
            player.feet = phys.arrive_player(view.feet).feet;
            player.heading = view.heading;
            player.pitch = view.pitch;
            auto.test_press_at(now + 1.0);
            info!("activation sweep: complete looks={} replay=first-success press=E", sweep.views.len());
        } else {
            info!("activation sweep: complete looks={} replay=none press=none", sweep.views.len());
        }
    }
}

// Reference metadata and positions come only from the loaded simulation [design:
// sim.rs, fnv-sim refs.rs/world.rs], never from hardcoded record identities.
fn metadata(sim: &SimState, id: Option<u32>) -> (u32, String) {
    let base = id.and_then(|id| sim.base_of(id));
    let sig = base.and_then(|b| sim.forms.get(b)).map_or("----".into(), |b| String::from_utf8_lossy(&b.sig).into_owned());
    (base.map_or(0, |b| b.0), sig)
}

fn nearby_door(sig: &str, at: [f32; 3], center: [f32; 3]) -> Option<f32> {
    let distance = Vec3::from(at).distance(Vec3::from(center));
    (sig == "DOOR" && distance <= DOOR_RADIUS).then_some(distance)
}

fn hit_door(sim: &SimState, center: Option<[f32; 3]>, id: Option<u32>) -> String {
    let Some((center, id)) = center.zip(id) else { return "none".into() };
    let (base, sig) = metadata(sim, Some(id));
    let at = sim.world.peek(FormId(id)).map(|r| r.pos).or_else(|| sim.refs.get(FormId(id)).map(|r| r.pos));
    at.and_then(|at| nearby_door(&sig, at, center)).map_or("none".into(), |d| format!("0x{id:08X}/0x{base:08X}@{d:.1}"))
}

fn sample(time: AidTime, mut sweep: ResMut<ActivationSweep>, player: Res<Player>, target: Res<Target>, sim: Res<SimState>, script: Option<Res<crate::script::ScriptState>>) {
    let Some(now) = time.world_secs() else { return };
    let Some((index, began)) = sweep.active else { return };
    if now < began + SETTLE {
        return;
    }
    let crosshair = script.as_deref().and_then(|s| s.rt.as_ref()).and_then(|r| r.game.senses.crosshair_ref).map(|r| r.0);
    let far = script.as_deref().and_then(|s| s.rt.as_ref()).and_then(|r| r.game.senses.crosshair_far).map(|r| r.0);
    let selected = target.0.as_ref().map(|t| t.form_id);
    let (base, sig) = metadata(&sim, crosshair);
    let (selected_base, selected_sig) = metadata(&sim, selected);
    let center = sim.world.peek(sweep.target).map(|r| r.pos).or_else(|| sim.refs.get(sweep.target).map(|r| r.pos));
    let collision_door = hit_door(&sim, center, crosshair);
    let target_door = hit_door(&sim, center, selected);
    let far_door = hit_door(&sim, center, far);
    let prompt = sweep.drawn.as_ref().map(|p| (p.action.as_str(), p.name.as_str()));
    info!(
        "activation sweep: look={:02} spot={} feet=({:.1},{:.1},{:.1}) yaw={:.1} pitch={:.1} GetCrosshairRef=0x{:08X} base=0x{:08X} type={} target=0x{:08X} target_base=0x{:08X} target_type={} prompt={:?} nearby_DOOR_collision={} nearby_DOOR_target={} nearby_DOOR_far={}",
        index + 1, index / (YAWS.len() * PITCHES.len()) + 1,
        player.feet[0], player.feet[1], player.feet[2], player.heading.to_degrees().rem_euclid(360.0), player.pitch.to_degrees(),
        crosshair.unwrap_or(0), base, sig, selected.unwrap_or(0), selected_base, selected_sig, prompt, collision_door, target_door, far_door,
    );
    sweep.remember(selected, View { feet: player.feet, heading: player.heading, pitch: player.pitch });
    sweep.active = None;
    sweep.next_at = began + 1.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_covers_every_spot_yaw_and_pitch() {
        let sweep = ActivationSweep::parse("123|1:2:3:90,4:5:6:0,7:8:9:270").unwrap();
        assert_eq!(sweep.views.len(), 3 * YAWS.len() * PITCHES.len());
        for (spot, chunk) in sweep.views.chunks(YAWS.len() * PITCHES.len()).enumerate() {
            for (yaw, group) in chunk.chunks(PITCHES.len()).enumerate() {
                for (pitch, view) in group.iter().enumerate() {
                    assert_eq!(view.feet, [[1., 2., 3.], [4., 5., 6.], [7., 8., 9.]][spot]);
                    assert!((view.heading.to_degrees() - ([90., 0., 270.][spot] + YAWS[yaw])).abs() < 0.001);
                    assert!((view.pitch.to_degrees() - PITCHES[pitch]).abs() < 0.001);
                }
            }
        }
    }

    #[test]
    fn invalid_configuration_is_rejected() {
        for value in ["", "x|1:2:3:0", "0|1:2:3:0", "123|", "123|1:2:3", "123|1:2:NaN:0", "123|1:2:3:inf"] {
            assert!(ActivationSweep::parse(value).is_err(), "{value}");
        }
    }

    #[test]
    fn only_the_first_success_is_replayed() {
        let mut sweep = ActivationSweep::parse("123|1:2:3:90").unwrap();
        let first = sweep.views[0];
        sweep.remember(None, first);
        sweep.remember(Some(0x456), first);
        assert!(sweep.first.is_none());
        sweep.remember(Some(0x123), first);
        sweep.remember(Some(0x123), sweep.views[1]);
        assert_eq!(sweep.first, Some(first));
    }

    #[test]
    fn door_radius_uses_three_dimensions_and_includes_boundary() {
        assert_eq!(nearby_door("DOOR", [0., 0., 400.], [0.; 3]), Some(400.));
        assert!(nearby_door("DOOR", [0., 0., 400.1], [0.; 3]).is_none());
        assert!(nearby_door("STAT", [0.; 3], [0.; 3]).is_none());
    }
}
