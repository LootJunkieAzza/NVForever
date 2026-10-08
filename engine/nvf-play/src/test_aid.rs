//! Timed test aids for unattended in-game checks (docs/design/layer4-play.md §6), next to
//! `NVF_PLAY_TEST_ACTIVATE` (activate.rs), `NVF_PLAY_TEST_MENU` and `NVF_PLAY_TEST_DROP`
//! (menu/test_aid.rs). Times are seconds after start.
//!
//! - `NVF_PLAY_TEST_LOOK=<seconds>:<heading deg>:<pitch deg>[,...]` turns the view (the
//!   compass heading, 0 north, 90 east; the pitch, negative looks down) at those times,
//!   as if the mouse had moved there.
//! - `NVF_PLAY_TEST_GOTO=<seconds>:<x>:<y>:<z>[,...]` puts the player's feet on whatever
//!   is under that point (game space, in the current cell or worldspace) at those times.
//! - `NVF_PLAY_TEST_SHOTS=<seconds>:<path.png>[,...]` saves a screenshot of the window
//!   at those times.
//!
//! With any `NVF_PLAY_TEST_*` variable set the run is unattended ([`Unattended`]): the
//! cursor is not grabbed, so the desktop's mouse neither leaves the user nor turns the
//! view (as with `--auto-walk`).

use crate::phys::PhysState;
use crate::player::Player;
use crate::PlaySet;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use std::path::PathBuf;

/// Parse `<seconds>:<rest>` items separated by commas; `rest` keeps its own colons.
pub fn timed(v: &str) -> Result<Vec<(f32, &str)>, String> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|item| {
            let (t, rest) = item.split_once(':').ok_or_else(|| format!("{item:?}: expected <seconds>:..."))?;
            let t: f32 = t.trim().parse().map_err(|_| format!("{item:?}: bad seconds {t:?}"))?;
            Ok((t, rest.trim()))
        })
        .collect()
}

/// `<heading deg>:<pitch deg>` → radians.
pub fn look_angles(rest: &str) -> Result<(f32, f32), String> {
    let (h, p) = rest.split_once(':').ok_or_else(|| format!("{rest:?}: expected <heading>:<pitch>"))?;
    let num = |s: &str| s.trim().parse::<f32>().map_err(|_| format!("{rest:?}: bad angle {s:?}"));
    Ok((num(h)?.to_radians(), num(p)?.to_radians()))
}

/// Whether a test aid drives this run (some `NVF_PLAY_TEST_*` variable is set).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Unattended(pub bool);

impl Unattended {
    pub fn from_env() -> Unattended {
        Unattended(std::env::vars_os().any(|(k, _)| k.to_str().is_some_and(|k| k.starts_with("NVF_PLAY_TEST_"))))
    }
}

/// `<x>:<y>:<z>` → a game-space point.
pub fn point(rest: &str) -> Result<[f32; 3], String> {
    let v: Vec<f32> = rest.split(':').map(|s| s.trim().parse::<f32>()).collect::<Result<_, _>>().map_err(|_| format!("{rest:?}: expected <x>:<y>:<z>"))?;
    <[f32; 3]>::try_from(v).map_err(|_| format!("{rest:?}: expected <x>:<y>:<z>"))
}

/// Scheduled actions, soonest last (popped from the end).
#[derive(Resource, Default)]
struct Timed {
    looks: Vec<(f32, f32, f32)>,
    gotos: Vec<(f32, [f32; 3])>,
    shots: Vec<(f32, PathBuf)>,
}

pub struct TestAidPlugin;

impl Plugin for TestAidPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Unattended::from_env());
        let mut t = Timed::default();
        if let Ok(v) = std::env::var("NVF_PLAY_TEST_LOOK") {
            match timed(&v).and_then(|items| items.into_iter().map(|(s, r)| look_angles(r).map(|(h, p)| (s, h, p))).collect()) {
                Ok(l) => t.looks = l,
                Err(e) => warn!("NVF_PLAY_TEST_LOOK: {e}"),
            }
        }
        if let Ok(v) = std::env::var("NVF_PLAY_TEST_GOTO") {
            match timed(&v).and_then(|items| items.into_iter().map(|(s, r)| point(r).map(|p| (s, p))).collect()) {
                Ok(g) => t.gotos = g,
                Err(e) => warn!("NVF_PLAY_TEST_GOTO: {e}"),
            }
        }
        if let Ok(v) = std::env::var("NVF_PLAY_TEST_SHOTS") {
            match timed(&v) {
                Ok(items) => t.shots = items.into_iter().map(|(s, p)| (s, PathBuf::from(p))).collect(),
                Err(e) => warn!("NVF_PLAY_TEST_SHOTS: {e}"),
            }
        }
        if t.looks.is_empty() && t.gotos.is_empty() && t.shots.is_empty() {
            return;
        }
        t.looks.sort_by(|a, b| b.0.total_cmp(&a.0));
        t.gotos.sort_by(|a, b| b.0.total_cmp(&a.0));
        t.shots.sort_by(|a, b| b.0.total_cmp(&a.0));
        app.insert_resource(t).add_systems(Update, run.in_set(PlaySet::Input));
    }
}

fn run(time: Res<Time>, mut t: ResMut<Timed>, mut player: Option<ResMut<Player>>, phys: Option<ResMut<PhysState>>, mut commands: Commands) {
    let now = time.elapsed_secs();
    if let (Some(player), Some(mut phys)) = (player.as_mut(), phys) {
        while t.gotos.last().is_some_and(|g| g.0 <= now) && phys.player.is_some() {
            let (_, at) = t.gotos.pop().expect("checked");
            let hit = phys.place_player(at, 60.0, 1000.0);
            let feet = phys.player.as_ref().map_or(at, |c| c.feet);
            player.feet = feet;
            info!(
                "test aid: go to ({:.0}, {:.0}, {:.0}): feet ({:.1}, {:.1}, {:.1}) on {:?}",
                at[0],
                at[1],
                at[2],
                feet[0],
                feet[1],
                feet[2],
                hit.map(|h| h.owner)
            );
        }
    }
    if let Some(mut player) = player {
        while t.looks.last().is_some_and(|l| l.0 <= now) {
            let (_, h, p) = t.looks.pop().expect("checked");
            info!("test aid: look heading {:.1} pitch {:.1}", h.to_degrees(), p.to_degrees());
            (player.heading, player.pitch) = (h, p);
        }
    }
    while t.shots.last().is_some_and(|s| s.0 <= now) {
        let (_, path) = t.shots.pop().expect("checked");
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(dir);
        }
        info!("test aid: screenshot {}", path.display());
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timed_items() {
        assert_eq!(timed("3:a.png, 7.5:b:c").unwrap(), vec![(3.0, "a.png"), (7.5, "b:c")]);
        assert_eq!(timed("").unwrap(), vec![]);
        assert!(timed("x:a").is_err());
        assert!(timed("3").is_err());
        let (h, p) = look_angles("90:-45").unwrap();
        assert!((h - std::f32::consts::FRAC_PI_2).abs() < 1e-6 && (p + std::f32::consts::FRAC_PI_4).abs() < 1e-6);
        assert!(look_angles("90").is_err());
        assert_eq!(point("1:-2.5:3").unwrap(), [1.0, -2.5, 3.0]);
        assert!(point("1:2").is_err());
    }
}
