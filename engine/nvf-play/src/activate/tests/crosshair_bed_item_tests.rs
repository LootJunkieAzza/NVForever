//! #3323: something lying on a bed (a suitcase at the Bison Steve) is targeted, not the
//! bed whose render box encloses it (`pick::set_aside_enclosing`). Beds are known by
//! their FURN marker flags (Sleep, bit 31), read by the script runtime.
use super::*;
use crate::script::ScriptState;
use fnv_esm::raw::{write_subrecords, Subrecord};

fn rec(sig: &[u8; 4], id: u32, fields: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let body = write_subrecords(&fields.iter().map(|(sig, data)| Subrecord { sig: **sig, data: data.clone(), extended: false }).collect::<Vec<_>>());
    [sig.to_vec(), (body.len() as u32).to_le_bytes().to_vec(), vec![0; 4], id.to_le_bytes().to_vec(), vec![0; 4], 15u16.to_le_bytes().to_vec(), vec![0; 2], body].concat()
}

/// The script runtime over a plugin whose FURN records give BED the Sleep marker flag and
/// CHAIR the Sit one.
fn furniture_flags() -> ScriptState {
    let hedr = [1.34f32.to_le_bytes(), 0u32.to_le_bytes(), 0x1000u32.to_le_bytes()].concat();
    let header = rec(b"TES4", 0, &[(b"HEDR", hedr)]);
    let furn = [rec(b"FURN", BED, &[(b"MNAM", (1u32 << 31).to_le_bytes().to_vec())]), rec(b"FURN", CHAIR, &[(b"MNAM", (1u32 << 30).to_le_bytes().to_vec())])].concat();
    let group = [b"GRUP".to_vec(), ((24 + furn.len()) as u32).to_le_bytes().to_vec(), b"FURN".to_vec(), 0i32.to_le_bytes().to_vec(), vec![0; 8], furn].concat();
    let bytes = [header, group].concat();
    ScriptState::over_plugins(&[fnv_esm::Plugin::parse(&bytes, "Beds.esm".into()).unwrap()])
}

/// A render entity of `form_id`: a box of half size `half` centred at game point `at`.
fn visual_sized(app: &mut App, form_id: u32, at: [f32; 3], half: f32) {
    let aabb = Aabb { center: Vec3::ZERO.into(), half_extents: Vec3::splat(half).into() };
    let gt = GlobalTransform::from_translation(space::to_bevy(at));
    app.world_mut().spawn((RefVisual(form_id), aabb, gt, InheritedVisibility::VISIBLE, Visibility::Inherited));
}

/// A thin collision slab of `id` across the view at `x` (2 units thick).
fn slab(app: &mut App, id: u32, x: f32) {
    let obj = CollisionObject { shape: Shape::Box { half_extents: [1.0, 40.0, 40.0] }.into(), ..boxed(1.0) };
    let pose = RefPose { form_id: id, position: [x, 0.0, 100.0], rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], scale: 1.0 };
    app.world_mut().resource_mut::<PhysState>().world.add_ref(&pose, &[obj]);
}

/// The player at the origin looking east, eye at 100; the bed's collision (the mattress)
/// hit at 110, its render box from 60 to 140 along the ray; the chest (the suitcase) a
/// box from 98 to 102 inside it when `item`; a wall hit at 40 when `wall`.
fn bed_scene(item: bool, wall: bool) -> App {
    let mut colliding = vec![(BED_REF, 120.0)];
    if wall {
        // An unnamed activator: collision the player cannot activate.
        colliding.push((TRIGGER_REF, 50.0));
    }
    let mut app = scene(&colliding);
    app.insert_resource(furniture_flags());
    visual_sized(&mut app, BED_REF, [100.0, 0.0, 100.0], 40.0);
    if item {
        visual_sized(&mut app, CHEST_REF, [100.0, 0.0, 100.0], 2.0);
    }
    app
}

#[test]
fn a_suitcase_lying_on_a_bed_is_the_target() {
    let mut app = bed_scene(true, false);
    app.update();
    assert_eq!(target(&app), Some((CHEST_REF, Source::Bounds)));
    assert_eq!(app.world().resource::<Target>().0.as_ref().unwrap().action, Action::Open);
}

#[test]
fn the_bare_mattress_is_the_bed() {
    let mut app = bed_scene(false, false);
    app.update();
    assert_eq!(target(&app), Some((BED_REF, Source::Collision)));
}

#[test]
fn a_wall_before_the_bed_hides_both() {
    let mut app = bed_scene(true, true);
    app.update();
    assert_eq!(target(&app), None);
}

/// Only a bed yields: a chair's box around a lever keeps the chair the target.
#[test]
fn a_chair_keeps_priority_over_a_lever_inside_its_box() {
    let mut app = scene(&[]);
    app.insert_resource(furniture_flags());
    visual_sized(&mut app, CHAIR_REF, [100.0, 0.0, 100.0], 40.0);
    visual_sized(&mut app, LEVER_REF, [100.0, 0.0, 100.0], 2.0);
    app.update();
    assert_eq!(target(&app), Some((CHAIR_REF, Source::Bounds)));
}

/// A wall hit at 80 with a door 14 behind it (promoted by `pick::door_behind`), a bed's
/// box from 65 to 120 and a chest's from 85 to 90, behind the wall: the cutoff for
/// setting the bed's box aside is the wall, so the bed keeps priority at 65 (with the
/// promoted door as the cutoff the chest would win).
#[test]
fn nothing_behind_a_wall_sets_a_bed_s_box_aside() {
    let mut app = scene(&[]);
    app.insert_resource(furniture_flags());
    slab(&mut app, TRIGGER_REF, 81.0);
    slab(&mut app, DOOR_REF, 95.0);
    visual_sized(&mut app, BED_REF, [92.5, 0.0, 100.0], 27.5);
    visual_sized(&mut app, CHEST_REF, [87.5, 0.0, 100.0], 2.5);
    app.update();
    let t = app.world().resource::<Target>().0.clone().unwrap();
    assert_eq!((t.form_id, t.source), (BED_REF, Source::Bounds));
    assert!((t.distance - 65.0).abs() < 0.1, "{}", t.distance);
}
