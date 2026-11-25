//! #3479: E on an actor it otherwise does nothing to (hostile, or a creature shown by name
//! only) opens the perks' choices on it, and never its default action.
use super::*;
use crate::script::{with_world, ScriptState};
use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_game::detection::{Band, Detection};
use fnv_game::functions::senses::Motion;

const SHUTDOWN: u32 = 0x900;

fn sub(sig: &[u8; 4], data: impl Into<Vec<u8>>) -> Subrecord {
    Subrecord { sig: *sig, data: data.into(), extended: false }
}

fn record(sig: &[u8; 4], id: u32, flags: u32, subs: &[Subrecord]) -> Vec<u8> {
    let body = write_subrecords(subs);
    [&sig[..], &(body.len() as u32).to_le_bytes(), &flags.to_le_bytes(), &id.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &body].concat()
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), sig, &[0; 4], &[0; 8], body].concat()
}

/// `function(p1) == value` on the tab's subject.
fn ctda(function: u16, p1: u32, value: f32) -> Subrecord {
    sub(b"CTDA", [&[0u8; 4][..], &value.to_le_bytes(), &function.to_le_bytes(), &[0; 2], &p1.to_le_bytes(), &[0; 12]].concat())
}

/// A perk with one activate choice: the player sneaks (IsSneaking), and the target is
/// alive, conscious and does not detect the player (GetDead, GetUnconscious, GetDetected);
/// its script is `SetUnconscious 1`.
fn plugin() -> Vec<u8> {
    let scda = [&0x10F1u16.to_le_bytes()[..], &7u16.to_le_bytes(), &1u16.to_le_bytes(), b"n", &1i32.to_le_bytes()].concat();
    let schr = [&[0u8; 8][..], &(scda.len() as u32).to_le_bytes(), &[0; 4], &0u16.to_le_bytes(), &1u16.to_le_bytes()].concat();
    let perk = [
        sub(b"EDID", *b"Shutdown\0"),
        sub(b"FULL", *b"Shutdown\0"),
        sub(b"DATA", [0, 1, 1, 1, 0]),
        sub(b"PRKE", [2, 0, 0]),
        sub(b"DATA", [27, 9, 2]),
        sub(b"PRKC", [0]),
        ctda(286, 0, 1.0),
        sub(b"PRKC", [1]),
        ctda(46, 0, 0.0),
        ctda(242, 0, 0.0),
        ctda(45, FormId::PLAYER_REF.0, 0.0),
        sub(b"EPFT", [4]),
        sub(b"EPF2", *b"Switch Off\0"),
        sub(b"EPF3", [0, 0]),
        sub(b"SCHR", schr),
        sub(b"SCDA", scda),
        sub(b"PRKF", []),
    ];
    let hedr = [1.34f32.to_le_bytes(), 0u32.to_le_bytes(), 0x1000u32.to_le_bytes()].concat();
    let robot = [sub(b"EDID", *b"Robot\0"), sub(b"ACBS", [0; 24])];
    [record(b"TES4", 0, 1, &[sub(b"HEDR", hedr)]), top(b"CREA", &record(b"CREA", ROBOT, 0, &robot)), top(b"PERK", &record(b"PERK", SHUTDOWN, 0, &perk))].concat()
}

/// The player at the origin looking at the robot, hostile (a fight StartCombat forced) or
/// not.
fn robot_scene(hostile: bool) -> App {
    let bytes = plugin();
    let mut script = ScriptState::over_plugins(&[fnv_esm::Plugin::parse(&bytes, "Robot.esm".into()).unwrap()]);
    if hostile {
        script.rt.as_mut().unwrap().game.awareness.forced.insert((FormId(ROBOT_REF), FormId::PLAYER_REF));
    }
    let mut app = scene(&[(ROBOT_REF, 60.0)]);
    app.insert_resource(script);
    app
}

fn runtime<R>(app: &mut App, f: impl FnOnce(&mut crate::script::Runtime) -> R) -> R {
    app.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
        let mut sim = world.resource_mut::<SimState>();
        with_world(script.rt.as_mut().unwrap(), &mut sim, f)
    })
}

fn sneak(app: &mut App, sneaking: bool) {
    runtime(app, |rt| rt.game.set_motion(FormId::PLAYER_REF, Motion { sneaking, ..Default::default() }));
}

fn detect(app: &mut App, band: Band) {
    runtime(app, |rt| {
        rt.game.detection.record(FormId(ROBOT_REF), FormId::PLAYER_REF, Detection { band, ..Default::default() }, 0.0);
    });
}

/// The open list's labels, if one is open.
fn list(app: &mut App) -> Option<Vec<String>> {
    runtime(app, |rt| rt.game.activate_menu.as_ref().map(|m| m.choices.iter().map(|c| c.label.clone()).collect()))
}

fn unconscious(app: &mut App) -> bool {
    runtime(app, |rt| rt.game.extra.get(&FormId(ROBOT_REF)).is_some_and(|e| e.unconscious))
}

fn activations(app: &mut App) -> usize {
    runtime(app, |rt| rt.game.events.iter().filter(|e| matches!(e.kind, fnv_game::trace::EventKind::Activated { .. })).count())
}

fn switched_off_by_e(hostile: bool) {
    let mut app = robot_scene(hostile);
    app.update();
    let action = if hostile { Action::Hostile } else { Action::Name };
    let shown = app.world().resource::<Target>().0.as_ref().map(|t| (t.form_id, t.action.clone(), t.prompted()));
    assert_eq!(shown, Some((ROBOT_REF, action, !hostile)));
    // Without the perk, standing, or detected: nothing.
    sneak(&mut app, true);
    press_e(&mut app);
    assert_eq!(list(&mut app), None, "no perk");
    runtime(&mut app, |rt| rt.game.player.perks.insert(FormId(SHUTDOWN), 1));
    sneak(&mut app, false);
    press_e(&mut app);
    assert_eq!(list(&mut app), None, "standing");
    sneak(&mut app, true);
    detect(&mut app, Band::Detected);
    press_e(&mut app);
    assert_eq!(list(&mut app), None, "detected");
    // Sneaking and unseen: the list, its last button closing it.
    detect(&mut app, Band::Hidden);
    app.update();
    assert_eq!(list(&mut app), None, "not without E");
    press_e(&mut app);
    assert_eq!(list(&mut app), Some(vec!["Switch Off".to_string()]));
    let panel = {
        let state = app.world().resource::<ScriptState>();
        let g = &state.rt.as_ref().unwrap().game;
        crate::script_hud::activate_panel(g, app.world().resource::<SimState>(), g.activate_menu.as_ref().unwrap())
    };
    assert_eq!(panel.buttons, ["Switch Off", "Cancel"], "never Talk");
    assert!(runtime(&mut app, |rt| rt.pick_activate_choice(1)));
    app.update();
    let again = runtime(&mut app, |rt| (rt.activate_again, rt.game.dialogue.is_some()));
    assert_eq!((list(&mut app), again, unconscious(&mut app)), (None, (None, false), false), "closed, nothing done");
    // The choice: its script runs on the robot, and the list is not offered again.
    press_e(&mut app);
    assert!(runtime(&mut app, |rt| rt.pick_activate_choice(0)));
    assert!(unconscious(&mut app));
    press_e(&mut app);
    assert_eq!(list(&mut app), None, "already off");
    assert_eq!(activations(&mut app), 0, "E never activated the robot itself");
    assert!(drain::<crate::pickpocket::OpenPickpocket>(&mut app).is_empty());
}

#[test]
fn e_opens_the_perks_choices_on_a_hostile_actor() {
    switched_off_by_e(true);
}

#[test]
fn e_opens_the_perks_choices_on_a_creature_with_nothing_to_say() {
    switched_off_by_e(false);
}
