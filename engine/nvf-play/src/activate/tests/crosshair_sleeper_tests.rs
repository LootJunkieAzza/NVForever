//! #3231: the crosshair on a bed someone lies on targets the sleeper while the sleeper's
//! body is within reach (`pick::to_occupant`), the bed's collision hit first on the ray.
use super::*;
use crate::hitvol::{ActorParts, PartSegment, PosedParts};
use crate::script::ScriptState;
use fnv_combat::data::Limb;
use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_game::game::FurnitureUse;

/// A plugin of a header alone: the script runtime with no records of its own.
fn header() -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0u32.to_le_bytes(), 0x1000u32.to_le_bytes()].concat();
    let body = write_subrecords(&[Subrecord { sig: *b"HEDR", data: hedr, extended: false }]);
    [b"TES4".to_vec(), (body.len() as u32).to_le_bytes().to_vec(), vec![0; 4], 0u32.to_le_bytes().to_vec(), vec![0; 4], 15u16.to_le_bytes().to_vec(), vec![0; 2], body].concat()
}

/// The player at the origin looking east, eye at 100; the bed's collision box at x 80
/// (the ray hits it at 70); the settler on it (bed marker 1, asleep), its posed body a
/// torso bone from `body_x` to `body_x + 20` at height 80, no collision of its own.
fn sleeper_scene(body_x: f32, asleep: bool) -> App {
    let bytes = header();
    let plugins = [fnv_esm::Plugin::parse(&bytes, "Sleeper.esm".into()).unwrap()];
    let mut script = ScriptState::over_plugins(&plugins);
    let using = asleep.then_some(FurnitureUse { reference: FormId(BED_REF), marker_id: 1, state: 3 });
    script.rt.as_mut().unwrap().game.set_furniture_use(FormId(SETTLER_REF), using);
    let mut app = scene(&[(BED_REF, 80.0)]);
    app.insert_resource(script);
    let bone = PartSegment { part: 0, limb: Limb::Torso, a: [body_x, 0.0, 80.0], b: [body_x + 20.0, 0.0, 80.0], radius: None };
    let mut parts = ActorParts::default();
    parts.by_ref.insert(SETTLER_REF, PosedParts { data: FormId(0), segments: vec![bone], weapon: None });
    app.insert_resource(parts);
    app
}

#[test]
fn a_bed_someone_lies_on_offers_talk_to_the_sleeper_in_reach() {
    let mut app = sleeper_scene(90.0, true);
    app.update();
    assert_eq!(target(&app), Some((SETTLER_REF, Source::Occupant)));
    let t = app.world().resource::<Target>().0.as_ref().unwrap();
    assert_eq!(t.action, Action::Talk);
    // The bone's nearest point (90, 0, 80) from the eye (0, 0, 100).
    assert!((t.distance - (90f32 * 90.0 + 20.0 * 20.0).sqrt()).abs() < 0.1, "{}", t.distance);
}

#[test]
fn a_sleeper_beyond_reach_leaves_the_bed_the_target() {
    // Aimed at the bed's near edge, the sleeper lying past the reach.
    let mut app = sleeper_scene(REACH + 10.0, true);
    app.update();
    assert_eq!(target(&app), Some((BED_REF, Source::Collision)));
}

#[test]
fn a_free_bed_is_the_target() {
    let mut app = sleeper_scene(90.0, false);
    app.update();
    assert_eq!(target(&app), Some((BED_REF, Source::Collision)));
    assert_eq!(app.world().resource::<Target>().0.as_ref().unwrap().action, Action::Activate);
}
