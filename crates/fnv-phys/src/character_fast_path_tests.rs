use super::*;
use crate::{PhysParams, RefPose};
use fnv_collide::{CollisionObject, Iso, Motion, Shape};

fn static_box(half_extents: [f32; 3]) -> CollisionObject {
    CollisionObject {
        node: None,
        node_block: 0,
        node_transform: Iso::IDENTITY,
        body_transform: Iso::IDENTITY,
        motion: Motion::Static,
        layer: 1,
        mass: 0.0,
        friction: 0.5,
        restitution: 0.0,
        shape: Shape::Box { half_extents }.into(),
        shell: 0.0,
        material: None,
        wind: false,
    }
}

fn pose(id: u32, position: [f32; 3]) -> RefPose {
    RefPose { form_id: id, position, rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], scale: 1.0 }
}

#[test]
fn idle_static_npc_holds_on_fixed_floor_and_breaks_on_input_or_floor_removal() {
    let mut w = PhysWorld::new(PhysParams::default());
    w.add_ref(&pose(10, [0.0, 0.0, -50.0]), &[static_box([500.0, 500.0, 50.0])]);
    let mut npc = w.add_character(1, CharacterParams::default(), [0.0, 0.0, 1.0]);
    npc.hold_when_stuck = true;
    npc.hold_when_idle_static = true;

    // Settle onto the fixed floor.
    let mut last_step = CharacterStep { feet: npc.feet, grounded: false, sliding: false, landed_speed: None, gap: 0.0, swimming: false, touched: Vec::new() };
    for _ in 0..10 {
        last_step = w.move_character(&mut npc, CharacterInput::default());
    }
    assert!(npc.grounded);
    assert!(
        npc.held.as_ref().is_some_and(|h| h.idle_static),
        "stationary grounded NPC enters idle_static hold (feet={:?}, grounded={}, sliding={}, gap={:?}, vel={:?}, vz={}, landing={:?}, perch={:?}, held={:?})",
        npc.feet, npc.grounded, last_step.sliding, npc.ground_gap, npc.velocity, npc.vertical_speed, npc.since_landing, npc.perch_base, npc.held
    );
    assert_eq!(npc.held_steps, 0, "idle_static must not inflate stuck held_steps");

    // Staying idle keeps the hold without incrementing held_steps.
    w.move_character(&mut npc, CharacterInput::default());
    assert!(npc.held.as_ref().is_some_and(|h| h.idle_static));
    assert_eq!(npc.held_steps, 0);

    // Any movement input breaks out immediately and moves the NPC.
    let start_x = npc.feet[0];
    let step = w.move_character(&mut npc, CharacterInput { velocity: [200.0, 0.0], ..Default::default() });
    assert!(step.feet[0] > start_x + 1.0, "movement input must break idle_static immediately");
    assert!(npc.held.is_none());

    // Settle again, then remove the floor: idle_static must break immediately and start falling.
    for _ in 0..10 {
        w.move_character(&mut npc, CharacterInput::default());
    }
    assert!(npc.held.as_ref().is_some_and(|h| h.idle_static));
    w.remove_ref(10);
    let step = w.move_character(&mut npc, CharacterInput::default());
    assert!(!step.grounded, "removing fixed support must break idle_static hold immediately");
}

#[test]
fn player_same_input_hold_releases_immediately_on_turn() {
    let mut w = PhysWorld::new(PhysParams::default());
    // Floor + 90-degree corner at x=30, y=30.
    w.add_ref(&pose(10, [0.0, 0.0, -50.0]), &[static_box([500.0, 500.0, 50.0])]);
    w.add_ref(&pose(11, [80.0, 0.0, 100.0]), &[static_box([50.0, 500.0, 100.0])]);
    w.add_ref(&pose(12, [0.0, 80.0, 100.0]), &[static_box([500.0, 50.0, 100.0])]);
    let mut player = w.add_character(0, CharacterParams::default(), [0.0, 0.0, 1.0]);
    player.hold_when_stuck_same_input = true;

    // Settle onto the floor, then push diagonally into the corner until wedged and held.
    for _ in 0..5 {
        w.move_character(&mut player, CharacterInput::default());
    }
    let mut ever_held = false;
    for _ in 0..30 {
        w.move_character(&mut player, CharacterInput { velocity: [200.0, 200.0], ..Default::default() });
        if player.held.as_ref().is_some_and(|h| !h.idle_static && h.dir.is_some()) {
            ever_held = true;
            break;
        }
    }
    assert!(ever_held, "player wedged into corner should enter same-input hold (feet={:?}, grounded={}, held={:?})", player.feet, player.grounded, player.held);
    let corner_feet = player.feet;
    let held_before = player.held_steps;
    w.move_character(&mut player, CharacterInput { velocity: [200.0, 200.0], ..Default::default() });
    assert_eq!(player.held_steps, held_before + 1, "same direction stays held");

    // Turning away from the corner breaks out immediately on the very next step.
    let step = w.move_character(&mut player, CharacterInput { velocity: [-200.0, -200.0], ..Default::default() });
    assert!(step.feet[0] < corner_feet[0] - 0.5 && step.feet[1] < corner_feet[1] - 0.5, "turning away must break same-input hold immediately");
    assert!(player.held.is_none());
}
