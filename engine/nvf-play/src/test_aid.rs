//! Play-wide test aids for unattended in-game checks (`--play` only; no effect unless set;
//! docs/design/layer4-play.md §6), next to `NVF_PLAY_TEST_ACTIVATE` (activate.rs),
//! `NVF_PLAY_TEST_NEAR` (player.rs), `NVF_PLAY_TEST_MENU` and `NVF_PLAY_TEST_DROP`
//! (menu/test_aid.rs). Times are seconds after the start.
//!
//! - `NVF_PLAY_TEST_SHOTS=<seconds>[:<path.png>][,...]`: save a screenshot at each time,
//!   to the path given, else as `play-<seconds>s.png` in `NVF_PLAY_TEST_SHOT_DIR`
//!   (default: the current folder).
//! - `NVF_PLAY_TEST_FACE=<seconds:reference FormID hex,...>`: at each time, put the player
//!   [`FACE_DISTANCE`] units in front of the reference, by the arrival rule, facing it
//!   level. In front: towards the door's own arrival point (where its partner door's XTEL
//!   puts travellers, so on the side the door is used from), else towards the player. A
//!   stand-in for walking up to a door after a travel, so one run can go through a door
//!   and back. Not a game behaviour.
//! - `NVF_PLAY_TEST_LOOK=<seconds>:<heading deg>:<pitch deg>[,...]` turns the view (the
//!   compass heading, 0 north, 90 east; the pitch, negative looks down) at those times,
//!   as if the mouse had moved there.
//! - `NVF_PLAY_TEST_GOTO=<seconds>:<x>:<y>:<z>[,...]` puts the player's feet on whatever
//!   is under that point (game space, in the current cell or worldspace) at those times.
//! - `NVF_PLAY_TEST_GIVE=<item FormID hex>[:<count>][,...]` adds items to the player's
//!   inventory at the start (`World::add_item`), e.g. something to drop.
//! - `NVF_PLAY_TEST_EQUIP=<item FormID hex>[,...]` gives the player one of each item
//!   unless carried, and equips it (`World::equip`; a weapon for the player body to draw).
//! - `NVF_PLAY_TEST_MOVE=<seconds>:<forward>:<right>[:walk][,...]` holds the movement keys
//!   from each time on: `forward` and `right` in −1..1 (W/S, D/A), `walk` as Shift held;
//!   `<seconds>:0:0` stops.
//! - `NVF_PLAY_TEST_CONTROL=<seconds>:<what>[,...]` presses a control at each time:
//!   `sneak` (Left Ctrl), `weapon` (R), `aim` (the right mouse button, pressed or
//!   released), `view` (F), `jump` (Space), or `zoom=<units>` (the third-person distance).
//!
//! With any `NVF_PLAY_TEST_*` variable set the run is unattended ([`Unattended`]): the
//! cursor is not grabbed, so the desktop's mouse neither leaves the user nor turns the
//! view (as with `--auto-walk`).

use crate::control::{Pov, PlayerControl};
use crate::phys::PhysState;
use crate::player::{MoveIntent, Player};
use crate::sim::SimState;
use crate::PlaySet;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use fnv_sim::FormId;
use std::path::{Path, PathBuf};

/// How far in front of the reference `NVF_PLAY_TEST_FACE` puts the feet (game units;
/// inside the activation reach).
pub const FACE_DISTANCE: f32 = 100.0;

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

/// `<x>:<y>:<z>` → a game-space point.
pub fn point(rest: &str) -> Result<[f32; 3], String> {
    let v: Vec<f32> = rest.split(':').map(|s| s.trim().parse::<f32>()).collect::<Result<_, _>>().map_err(|_| format!("{rest:?}: expected <x>:<y>:<z>"))?;
    <[f32; 3]>::try_from(v).map_err(|_| format!("{rest:?}: expected <x>:<y>:<z>"))
}

/// `NVF_PLAY_TEST_GIVE` items: `<item>[:<count>]` (count 1 by default).
fn gifts(v: &str) -> Result<Vec<(FormId, i32)>, String> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|item| {
            let (id, n) = item.split_once(':').unwrap_or((item, "1"));
            let id = id.trim().parse::<FormId>().map_err(|e| format!("{item:?}: {e}"))?;
            let n = n.trim().parse::<i32>().map_err(|_| format!("{item:?}: bad count {n:?}"))?;
            Ok((id, n))
        })
        .collect()
}

/// `NVF_PLAY_TEST_MOVE` items: `<forward>:<right>[:walk]`.
fn move_keys(rest: &str) -> Result<MoveIntent, String> {
    let parts: Vec<&str> = rest.split(':').map(str::trim).collect();
    let num = |s: &str| s.parse::<f32>().map(|v| v.clamp(-1.0, 1.0)).map_err(|_| format!("{rest:?}: bad axis {s:?}"));
    match parts.as_slice() {
        [f, r] => Ok(MoveIntent { forward: num(f)?, right: num(r)?, ..Default::default() }),
        [f, r, "walk"] => Ok(MoveIntent { forward: num(f)?, right: num(r)?, walk: true, ..Default::default() }),
        _ => Err(format!("{rest:?}: expected <forward>:<right>[:walk]")),
    }
}

/// A control pressed by `NVF_PLAY_TEST_CONTROL`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Press {
    Sneak,
    Weapon,
    Aim,
    View,
    Jump,
    Zoom(f32),
}

fn press(rest: &str) -> Result<Press, String> {
    Ok(match rest {
        "sneak" => Press::Sneak,
        "weapon" => Press::Weapon,
        "aim" => Press::Aim,
        "view" => Press::View,
        "jump" => Press::Jump,
        _ => match rest.strip_prefix("zoom=").map(|d| d.trim().parse::<f32>()) {
            Some(Ok(d)) => Press::Zoom(d),
            _ => return Err(format!("{rest:?}: expected sneak, weapon, aim, view, jump or zoom=<units>")),
        },
    })
}

/// `NVF_PLAY_TEST_SHOTS` items: `<seconds>` (saved as `play-<seconds>s.png` in `dir`) or
/// `<seconds>:<path>`; bad entries are skipped.
fn shots(v: &str, dir: &Path) -> Vec<(f32, PathBuf)> {
    v.split(',')
        .map(str::trim)
        .filter_map(|item| match item.split_once(':') {
            Some((t, path)) => Some((t.trim().parse().ok()?, PathBuf::from(path.trim()))),
            None => {
                let t: f32 = item.parse().ok()?;
                Some((t, dir.join(format!("play-{t}s.png"))))
            }
        })
        .collect()
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

/// The ones of `due` whose time has come (removed from it), in time order.
fn take_due<T: Clone>(due: &mut Vec<(f32, T)>, now: f32) -> Vec<T> {
    let mut fired: Vec<(f32, T)> = due.iter().filter(|(t, _)| *t <= now).cloned().collect();
    fired.sort_by(|a, b| a.0.total_cmp(&b.0));
    due.retain(|(t, _)| *t > now);
    fired.into_iter().map(|(_, v)| v).collect()
}

/// Whether a test aid drives this run (some `NVF_PLAY_TEST_*` variable is set).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Unattended(pub bool);

impl Unattended {
    pub fn from_env() -> Unattended {
        Unattended(std::env::vars_os().any(|(k, _)| k.to_str().is_some_and(|k| k.starts_with("NVF_PLAY_TEST_"))))
    }
}

/// Scheduled actions.
#[derive(Resource, Default)]
struct Aids {
    shots: Vec<(f32, PathBuf)>,
    face: Vec<(f32, FormId)>,
    looks: Vec<(f32, (f32, f32))>,
    gotos: Vec<(f32, [f32; 3])>,
    moves: Vec<(f32, MoveIntent)>,
    /// The movement keys held since the last `moves` entry fired.
    held: Option<MoveIntent>,
    presses: Vec<(f32, Press)>,
}

pub(crate) fn setup(app: &mut App) {
    app.insert_resource(Unattended::from_env());
    if let Ok(v) = std::env::var("NVF_PLAY_TEST_GIVE") {
        match gifts(&v) {
            Ok(g) => {
                app.add_systems(Startup, move |mut sim: ResMut<SimState>| {
                    for &(item, n) in &g {
                        match sim.world.add_item(FormId::PLAYER_REF, item, n) {
                            Ok(_) => info!("test aid: gave the player {n} x {item}"),
                            Err(e) => warn!("NVF_PLAY_TEST_GIVE {item}: {e}"),
                        }
                    }
                });
            }
            Err(e) => warn!("NVF_PLAY_TEST_GIVE: {e}"),
        }
    }
    if let Ok(v) = std::env::var("NVF_PLAY_TEST_EQUIP") {
        match gifts(&v) {
            Ok(g) => {
                app.add_systems(Startup, move |mut sim: ResMut<SimState>| {
                    for &(item, _) in &g {
                        let w = &mut sim.world;
                        let carried = w.item_count(FormId::PLAYER_REF, item).unwrap_or(0) > 0;
                        let r = if carried { Ok(Vec::new()) } else { w.add_item(FormId::PLAYER_REF, item, 1) };
                        match r.and_then(|_| w.equip(FormId::PLAYER_REF, item)) {
                            Ok(e) => info!("test aid: the player equips {item} (unequipped {:?})", e.unequipped),
                            Err(e) => warn!("NVF_PLAY_TEST_EQUIP {item}: {e}"),
                        }
                    }
                });
            }
            Err(e) => warn!("NVF_PLAY_TEST_EQUIP: {e}"),
        }
    }
    let var = |name: &str| std::env::var(name).ok();
    let mut aids = Aids::default();
    if let Some(v) = var("NVF_PLAY_TEST_SHOTS") {
        let dir = std::env::var_os("NVF_PLAY_TEST_SHOT_DIR").map_or_else(|| PathBuf::from("."), PathBuf::from);
        aids.shots = shots(&v, &dir);
    }
    if let Some(v) = var("NVF_PLAY_TEST_FACE") {
        aids.face = faces(&v);
    }
    if let Some(v) = var("NVF_PLAY_TEST_LOOK") {
        match timed(&v).and_then(|items| items.into_iter().map(|(s, r)| look_angles(r).map(|a| (s, a))).collect()) {
            Ok(l) => aids.looks = l,
            Err(e) => warn!("NVF_PLAY_TEST_LOOK: {e}"),
        }
    }
    if let Some(v) = var("NVF_PLAY_TEST_GOTO") {
        match timed(&v).and_then(|items| items.into_iter().map(|(s, r)| point(r).map(|p| (s, p))).collect()) {
            Ok(g) => aids.gotos = g,
            Err(e) => warn!("NVF_PLAY_TEST_GOTO: {e}"),
        }
    }
    if let Some(v) = var("NVF_PLAY_TEST_MOVE") {
        match timed(&v).and_then(|items| items.into_iter().map(|(s, r)| move_keys(r).map(|m| (s, m))).collect()) {
            Ok(m) => aids.moves = m,
            Err(e) => warn!("NVF_PLAY_TEST_MOVE: {e}"),
        }
    }
    if let Some(v) = var("NVF_PLAY_TEST_CONTROL") {
        match timed(&v).and_then(|items| items.into_iter().map(|(s, r)| press(r).map(|p| (s, p))).collect()) {
            Ok(p) => aids.presses = p,
            Err(e) => warn!("NVF_PLAY_TEST_CONTROL: {e}"),
        }
    }
    if aids.shots.is_empty() && aids.face.is_empty() && aids.looks.is_empty() && aids.gotos.is_empty() && aids.moves.is_empty() && aids.presses.is_empty() {
        return;
    }
    app.insert_resource(aids).add_systems(
        Update,
        (take_shots, face_reference, goto_and_look, keys_and_controls).chain().in_set(PlaySet::Input).after(crate::control::read_controls),
    );
}

/// `NVF_PLAY_TEST_MOVE` and `NVF_PLAY_TEST_CONTROL`, after the real keys were read.
fn keys_and_controls(time: Res<Time>, mut aids: ResMut<Aids>, mut intent: ResMut<MoveIntent>, mut control: ResMut<PlayerControl>) {
    let now = time.elapsed_secs();
    for m in take_due(&mut aids.moves, now) {
        info!("test aid: movement keys forward {} right {}{}", m.forward, m.right, if m.walk { " walking" } else { "" });
        aids.held = Some(m);
    }
    if let Some(m) = aids.held {
        (intent.forward, intent.right, intent.walk) = (m.forward, m.right, m.walk);
    }
    for p in take_due(&mut aids.presses, now) {
        match p {
            Press::Sneak => control.sneak = !control.sneak,
            Press::Weapon => control.weapon_out = !control.weapon_out,
            Press::Aim => control.aiming = !control.aiming,
            Press::View => control.pov = if control.pov == Pov::First { Pov::Third } else { Pov::First },
            Press::Jump => intent.jump = true,
            Press::Zoom(d) => control.distance = d.clamp(crate::control::MIN_DISTANCE, crate::control::MAX_DISTANCE),
        }
        info!(
            "test aid: control {p:?}: sneak {} weapon out {} aiming {} view {:?} distance {:.0}",
            control.sneak, control.weapon_out, control.aiming, control.pov, control.distance
        );
    }
}

fn take_shots(time: Res<Time>, mut aids: ResMut<Aids>, mut commands: Commands) {
    for path in take_due(&mut aids.shots, time.elapsed_secs()) {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(dir);
        }
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

fn goto_and_look(time: Res<Time>, mut aids: ResMut<Aids>, mut phys: ResMut<PhysState>, mut player: ResMut<Player>) {
    let now = time.elapsed_secs();
    if phys.player.is_some() {
        for at in take_due(&mut aids.gotos, now) {
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
    for (h, p) in take_due(&mut aids.looks, now) {
        info!("test aid: look heading {:.1} pitch {:.1}", h.to_degrees(), p.to_degrees());
        (player.heading, player.pitch) = (h, p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_fires_once() {
        let dir = Path::new("out");
        assert_eq!(
            shots("6, 12.5,x, 7:C:/a/b.png", dir),
            vec![(6.0, dir.join("play-6s.png")), (12.5, dir.join("play-12.5s.png")), (7.0, PathBuf::from("C:/a/b.png"))]
        );
        assert_eq!(faces("14:116424, 30:0x13BB0D, bad"), vec![(14.0, FormId(0x116424)), (30.0, FormId(0x13BB0D))]);
        let mut due = vec![(2.0, 'b'), (1.0, 'a')];
        assert_eq!(take_due(&mut due, 1.5), vec!['a']);
        assert_eq!(take_due(&mut due, 1.6), Vec::<char>::new());
        assert_eq!(take_due(&mut due, 5.0), vec!['b']);
        assert!(due.is_empty());
        let mut due = vec![(2.0, 'b'), (1.0, 'a')];
        assert_eq!(take_due(&mut due, 5.0), vec!['a', 'b'], "in time order");
    }

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
        assert_eq!(gifts("00015169:2, 0x0000000F").unwrap(), vec![(FormId(0x15169), 2), (FormId(0xF), 1)]);
        assert!(gifts("00015169:x").is_err());
        assert!(point("1:2").is_err());
        assert_eq!(move_keys("1:-0.5").unwrap(), MoveIntent { forward: 1.0, right: -0.5, ..Default::default() });
        assert!(move_keys("-1:0:walk").unwrap().walk);
        assert!(move_keys("1").is_err() && move_keys("1:0:run").is_err());
        assert_eq!(press("zoom=250").unwrap(), Press::Zoom(250.0));
        assert_eq!(press("weapon").unwrap(), Press::Weapon);
        assert!(press("fire").is_err());
    }
}
