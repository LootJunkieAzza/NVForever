use crate::apart::{self, Body, RADIUS};
use fnv_nav::Space;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

#[test]
fn placed_presence_log_omits_dynamic_refs_and_handles_full_rescan_triggers() {
    let forms = FormStore::default();
    let placed = RefTable::default();
    let mut world = World::new(&forms, &placed, SimConfig::default());
    let s0 = world.presence_serial();
    assert_eq!(world.placed_presence_changes_since(s0), Some(&[][..]));

    // Replacing enable-parent topology forces a full rescan for older cursors.
    world.set_enable_parent(FormId(0x0000_1000), Some(FormId(0x0000_2000)));
    assert_eq!(world.placed_presence_changes_since(s0), None);
    let s1 = world.presence_serial();
    assert_eq!(world.placed_presence_changes_since(s1), Some(&[][..]));

    // keep_own_enable on a placed reference records that placed ID.
    world.keep_own_enable(FormId(0x0000_1000));
    let changes = world.placed_presence_changes_since(s1).expect("incremental changes");
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].1, FormId(0x0000_1000));

    // A separate World has a distinct presence_epoch so cached AI cursors never alias.
    let other = World::new(&forms, &placed, SimConfig::default());
    assert_ne!(world.presence_epoch(), other.presence_epoch());
}

#[test]
fn apart_separate_aabb_and_dist_sq_matches_exact_hypot_oracle() {
    let diameter = 2.0 * RADIUS;
    // Test each boundary independently against body 0 at origin.
    let cases = [
        ("within diameter on X", [diameter - 0.5, 0.0, 0.0], Space::World(10), true, true),
        ("exactly at diameter", [diameter, 0.0, 0.0], Space::World(10), true, false),
        ("inside XY box but outside circle", [diameter * 0.75, diameter * 0.75, 0.0], Space::World(10), true, false),
        ("inside circle with immovable", [diameter * 0.5, diameter * 0.5, 0.0], Space::World(10), false, true),
        ("different floor (>100 Z)", [0.0, 0.0, 105.0], Space::World(10), true, false),
        ("different space", [0.0, 0.0, 0.0], Space::World(11), true, false),
    ];
    for (label, feet, space, movable, expect_touch) in cases {
        let bodies = vec![
            Body { id: 1, space: Space::World(10), feet: [0.0, 0.0, 0.0], movable: true },
            Body { id: 2, space, feet, movable },
        ];
        let (moves, touched) = apart::separate(&bodies, 0.016);
        assert_eq!(touched[0], expect_touch, "{label}: touched[0]");
        assert_eq!(touched[1], expect_touch, "{label}: touched[1]");
        if !movable {
            assert_eq!(moves[1], [0.0, 0.0], "{label}: immovable body must not move");
        }
        if expect_touch {
            assert!(moves[0][0] != 0.0 || moves[0][1] != 0.0, "{label}: movable body must separate");
        } else {
            assert_eq!(moves, vec![[0.0, 0.0], [0.0, 0.0]], "{label}: non-overlapping bodies must not move");
        }
    }
}
