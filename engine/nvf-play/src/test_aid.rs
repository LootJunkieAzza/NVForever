//! Play-wide test aids for unattended runs (`--play` only; no effect unless set):
//! - `NVF_PLAY_TEST_SHOTS=<seconds,...>`: save a screenshot at each time after the start,
//!   as `play-<seconds>s.png` in `NVF_PLAY_TEST_SHOT_DIR` (default: the current folder).
//! - `NVF_PLAY_TEST_FACE=<seconds:reference FormID hex,...>`: at each time, put the player
//!   [`FACE_DISTANCE`] units in front of the reference, by the arrival rule, facing it
//!   level. In front: towards the door's own arrival point (where its partner door's XTEL
//!   puts travellers, so on the side the door is used from), else towards the player. A stand-in for walking up to a door
//!   after a travel, so one run can go through a door and back
//!   (`NVF_PLAY_TEST_ACTIVATE` presses E). Not a game behaviour.

use crate::phys::PhysState;
use crate::player::Player;
use crate::sim::SimState;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use fnv_sim::FormId;
use std::path::PathBuf;

/// How far in front of the reference `NVF_PLAY_TEST_FACE` puts the feet (game units;
/// inside the activation reach).
pub const FACE_DISTANCE: f32 = 100.0;

#[derive(Resource, Default)]
struct Aids {
    shots: Vec<f32>,
    shot_dir: PathBuf,
    face: Vec<(f32, FormId)>,
}

/// `"6, 12.5"` → times; bad entries are skipped.
fn times(v: &str) -> Vec<f32> {
    v.split(',').filter_map(|t| t.trim().parse().ok()).collect()
}

/// `"14:116424, 30:0x13BB0D"` → (time, reference); bad entries are skipped.
fn faces(v: &str) -> Vec<(f32, FormId)> {
    v.split(',')
        .filter_map(|e| {
            let (t, id) = e.trim().split_once(':')?;
            Some((t.trim().parse().ok()?, id.trim().parse().ok()?))
        })
        .collect()
}

/// The ones of `due` whose time has come (removed from it).
fn take_due<T: Copy>(due: &mut Vec<(f32, T)>, now: f32) -> Vec<T> {
    let fired = due.iter().filter(|(t, _)| *t <= now).map(|&(_, v)| v).collect();
    due.retain(|(t, _)| *t > now);
    fired
}

pub(crate) fn setup(app: &mut App) {
    let shots = std::env::var("NVF_PLAY_TEST_SHOTS").map(|v| times(&v)).unwrap_or_default();
    let face = std::env::var("NVF_PLAY_TEST_FACE").map(|v| faces(&v)).unwrap_or_default();
    if shots.is_empty() && face.is_empty() {
        return;
    }
    let shot_dir = std::env::var_os("NVF_PLAY_TEST_SHOT_DIR").map_or_else(|| PathBuf::from("."), PathBuf::from);
    app.insert_resource(Aids { shots, shot_dir, face })
        .add_systems(Update, (take_shots, face_reference).in_set(crate::PlaySet::Input));
}

fn take_shots(time: Res<Time>, mut aids: ResMut<Aids>, mut commands: Commands) {
    let now = time.elapsed_secs();
    let mut due: Vec<(f32, f32)> = aids.shots.iter().map(|&t| (t, t)).collect();
    let fired = take_due(&mut due, now);
    if fired.is_empty() {
        return;
    }
    aids.shots = due.into_iter().map(|(t, _)| t).collect();
    for t in fired {
        let path = aids.shot_dir.join(format!("play-{t}s.png"));
        info!("test aid: screenshot {}", path.display());
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
}

fn face_reference(time: Res<Time>, mut aids: ResMut<Aids>, sim: Res<SimState>, mut phys: ResMut<PhysState>, mut player: ResMut<Player>) {
    for id in take_due(&mut aids.face, time.elapsed_secs()) {
        let Some(at) = sim.world.peek(id).map(|s| s.pos).or_else(|| sim.refs.get(id).map(|p| p.pos)) else {
            warn!("test aid: face {id}: unknown reference");
            continue;
        };
        let feet = phys.player.as_ref().map_or(player.feet, |c| c.feet);
        let arrival = sim.refs.get(id).and_then(|p| p.teleport).and_then(|tp| sim.refs.get(tp.door)?.teleport).filter(|t| t.door == id);
        let from = arrival.map_or(feet, |t| t.pos);
        let (dx, dy) = (at[0] - from[0], at[1] - from[1]);
        let len = dx.hypot(dy).max(1e-3);
        let stand = [at[0] - dx / len * FACE_DISTANCE, at[1] - dy / len * FACE_DISTANCE, at[2]];
        let a = phys.arrive_player(stand);
        player.feet = a.feet;
        player.heading = dx.atan2(dy);
        player.pitch = 0.0;
        info!(
            "test aid: face {id} at ({:.0}, {:.0}, {:.0}): feet ({:.0}, {:.0}, {:.1}), heading {:.1}",
            at[0],
            at[1],
            at[2],
            a.feet[0],
            a.feet[1],
            a.feet[2],
            player.heading.to_degrees().rem_euclid(360.0)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_fires_once() {
        assert_eq!(times("6, 12.5,x"), vec![6.0, 12.5]);
        assert_eq!(faces("14:116424, 30:0x13BB0D, bad"), vec![(14.0, FormId(0x116424)), (30.0, FormId(0x13BB0D))]);
        let mut due = vec![(1.0, 'a'), (2.0, 'b')];
        assert_eq!(take_due(&mut due, 1.5), vec!['a']);
        assert_eq!(take_due(&mut due, 1.6), Vec::<char>::new());
        assert_eq!(take_due(&mut due, 5.0), vec!['b']);
        assert!(due.is_empty());
    }
}
