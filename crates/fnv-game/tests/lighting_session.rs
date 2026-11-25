//! Compiled session writes reach typed lighting; all records are authored.
use crate::common as lighting_build;
#[path = "common/lighting_templates.rs"]
mod fixtures;
use fixtures::*;
use fnv_script::vm::XVal;
use fnv_world::lighting::CellLighting;

#[test]
fn cold_unset_and_missing_templates_match_the_existing_reader() {
    let f = fixture(); let g = game(&f);
    for cell in [CELL, OTHER_CELL, NO_TEMPLATE, EMPTY_CELL] {
        assert_eq!(g.session_cell_lighting(f.lo, f.schema, cell), fnv_world::lighting::cell_lighting(f.lo, f.schema, cell.0));
    }
    let cold = g.session_cell_lighting(f.lo, f.schema, CELL).unwrap();
    assert_eq!((cold.ambient, cold.fog_clip_dist), ([1, 2, 3], 10000.0));
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, NO_TEMPLATE).unwrap().fog_clip_dist, 3000.0);
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, EMPTY_CELL), None);
}

#[test]
fn selected_template_and_numeric_setter_change_the_applied_light() {
    let f = fixture(); let mut g = game(&f);
    set(&mut g, "SetCellLightingTemplate OwnedLightCell OwnedLightB\nSetLightingTemplateTraitNumeric OwnedLightB 15 30000");
    assert_eq!(query(&mut g, "SetFunctionValue (GetCellLightingTemplate OwnedLightCell)"), XVal::Ref(B.0));
    assert_eq!(query(&mut g, "SetFunctionValue (GetLightingTemplateTraitNumeric OwnedLightB 15)"), XVal::Num(30000.0));
    let applied = g.session_cell_lighting(f.lo, f.schema, CELL).unwrap();
    assert_eq!(applied.fog_clip_dist, 30000.0, "applied lighting must consume the actual compiled template selection and trait write");
    assert_eq!(applied.ambient, [1, 2, 3], "unselected inheritance fields remain the cell's");
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, OTHER_CELL).unwrap().fog_clip_dist, 10000.0);
    assert_eq!(fnv_world::lighting::cell_lighting(f.lo, f.schema, CELL.0).unwrap().fog_clip_dist, 10000.0, "the immutable records were not edited");
}

#[test]
fn inheritance_flags_and_explicit_null_template_preserve_cell_values() {
    let f = fixture(); let mut g = game(&f);
    set(&mut g, "SetCellFlag OwnedLightCell 15 0");
    assert_eq!(query(&mut g, "SetFunctionValue (GetCellFlag OwnedLightCell 15)"), XVal::Num(0.0));
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, CELL).unwrap().fog_clip_dist, 1000.0, "clearing inheritance must select the cell's own clip");
    set(&mut g, "SetCellFlag OwnedLightCell 15 1\nSetCellLightingTemplate OwnedLightCell 0");
    assert_eq!(query(&mut g, "SetFunctionValue (GetCellLightingTemplate OwnedLightCell)"), XVal::Ref(0));
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, CELL).unwrap().fog_clip_dist, 1000.0, "explicit null is distinct from no override");
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, OTHER_CELL).unwrap().fog_clip_dist, 10000.0);
}

#[test]
fn all_template_traits_follow_existing_typed_fields_only_when_inherited() {
    let f = fixture(); let mut g = game(&f);
    set(&mut g, "SetCellLightingTemplate OwnedLightCell OwnedLightB");
    let values = [101.0, 102.0, 103.0, 111.0, 112.0, 113.0, 121.0, 122.0, 123.0, 31.5, 63.25, 135.0, 270.0, 0.75, 30000.0, 3.5];
    for (i, value) in values.into_iter().enumerate() {
        set(&mut g, &format!("SetLightingTemplateTraitNumeric OwnedLightB {} {value}", i + 1));
    }
    for flag in 8..=16 { set(&mut g, &format!("SetCellFlag OwnedLightCell {flag} 1")); }
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, CELL), Some(CellLighting {
        ambient: [101, 102, 103], directional: [111, 112, 113], fog_color: [121, 122, 123],
        fog_near: 31.5, fog_far: 63.25, directional_rotation_xy: 135, directional_rotation_z: 270,
        directional_fade: 0.75, fog_clip_dist: 30000.0, fog_power: 3.5,
    }), "documented trait IDs must reach their actual typed lighting fields");
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, OTHER_CELL).unwrap().fog_clip_dist, 10000.0);
}

#[test]
fn restore_discards_session_lighting_without_changing_snapshot_policy() {
    let f = fixture(); let mut g = game(&f);
    set(&mut g, "SetCellLightingTemplate OwnedLightCell OwnedLightB\nSetLightingTemplateTraitNumeric OwnedLightB 15 30000");
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, CELL).unwrap().fog_clip_dist, 30000.0);
    let snapshot = g.snapshot();
    let mut loaded = restore(&f, &snapshot);
    assert_eq!(query(&mut loaded, "SetFunctionValue (GetCellLightingTemplate OwnedLightCell)"), XVal::Ref(A.0));
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, CELL).unwrap().fog_clip_dist, 30000.0, "the old Game remains separate");
    assert_eq!(loaded.session_cell_lighting(f.lo, f.schema, CELL).unwrap().fog_clip_dist, 10000.0, "existing Game::restore does not persist generic session record overrides");
    assert_eq!(loaded.session_cell_lighting(f.lo, f.schema, OTHER_CELL), fnv_world::lighting::cell_lighting(f.lo, f.schema, OTHER_CELL.0));
}

#[test]
fn a_shifted_plugin_namespace_keeps_the_cold_template_identity() {
    let f = shifted_fixture(); let g = game(&f);
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, SHIFTED_CELL), fnv_world::lighting::cell_lighting(f.lo, f.schema, SHIFTED_CELL.0));
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, SHIFTED_CELL).unwrap().fog_clip_dist, 45000.0);
}

#[test]
fn interior_lighting_traits_reach_own_xcll_and_respect_template_inheritance() {
    let f = fixture(); let mut g = game(&f);
    // Clear all inheritance bits (8..=16) so every field resolves from CELL XCLL.
    for flag in 8..=16 { set(&mut g, &format!("SetCellFlag OwnedLightCell {flag} 0")); }
    let values = [
        41.0, 42.0, 43.0, 51.0, 52.0, 53.0, 120.0, 240.0, 0.85,
        61.0, 62.0, 63.0, 125.5, 4500.25, 2.25, 18000.0,
    ];
    for (id, value) in values.into_iter().enumerate() {
        set(&mut g, &format!("SetInteriorLightingTraitNumeric OwnedLightCell {id} {value}"));
    }
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, CELL), Some(CellLighting {
        ambient: [41, 42, 43],
        directional: [51, 52, 53],
        directional_rotation_xy: 120,
        directional_rotation_z: 240,
        directional_fade: 0.85,
        fog_color: [61, 62, 63],
        fog_near: 125.5,
        fog_far: 4500.25,
        fog_power: 2.25,
        fog_clip_dist: 18000.0,
    }), "SetInteriorLightingTraitNumeric traits 0..=15 must reach CELL XCLL fields");

    // Re-enabling clip inheritance (flag 15) lets template A's clip win over own XCLL clip.
    set(&mut g, "SetCellFlag OwnedLightCell 15 1");
    let merged = g.session_cell_lighting(f.lo, f.schema, CELL).unwrap();
    assert_eq!(merged.fog_clip_dist, 10000.0);
    assert_eq!(merged.ambient, [41, 42, 43], "uninherited fields keep the edited XCLL values");
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, OTHER_CELL).unwrap().ambient, [1, 2, 3]);
}

#[test]
fn lighting_token_tracks_only_lighting_edits_and_distinguishes_restored_games() {
    let f = fixture(); let mut g = game(&f);
    let t0 = g.lighting_token();
    // Non-lighting cell flag (0..=7) does not bump the lighting revision.
    set(&mut g, "SetCellFlag OwnedLightCell 0 1");
    assert_eq!(g.lighting_token(), t0, "DATA cell flags 0..=7 must not trigger lighting refresh");

    // Each of the four lighting setters bumps the monotonic revision within the same Game instance.
    set(&mut g, "SetCellLightingTemplate OwnedLightCell OwnedLightB");
    let t1 = g.lighting_token();
    assert_eq!(t1.0, t0.0);
    assert!(t1.1 > t0.1);

    set(&mut g, "SetCellFlag OwnedLightCell 8 1");
    let t2 = g.lighting_token();
    assert!(t2.1 > t1.1);

    set(&mut g, "SetLightingTemplateTraitNumeric OwnedLightB 1 99");
    let t3 = g.lighting_token();
    assert!(t3.1 > t2.1);

    set(&mut g, "SetInteriorLightingTraitNumeric OwnedLightCell 0 55");
    let t4 = g.lighting_token();
    assert!(t4.1 > t3.1);

    // A restored Game has a distinct instance ID so engine systems cannot confuse revisions across runs.
    let loaded = restore(&f, &g.snapshot());
    assert_ne!(loaded.lighting_token().0, g.lighting_token().0);
    assert_eq!(loaded.lighting_token().1, 0);
}

#[test]
fn xcll_less_interior_receives_set_interior_lighting_traits_and_idempotent_writes_do_not_churn() {
    let f = fixture();
    let mut g = game(&f);

    // Cold EMPTY_CELL (no authored XCLL or LTMP) remains None.
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, EMPTY_CELL), None);

    // Writing SetInteriorLightingTraitNumeric on an XCLL-less interior synthesizes default XCLL.
    set(
        &mut g,
        "SetInteriorLightingTraitNumeric OwnedEmptyCell 0 80\n\
         SetInteriorLightingTraitNumeric OwnedEmptyCell 12 200\n\
         SetInteriorLightingTraitNumeric OwnedEmptyCell 13 4000\n\
         SetInteriorLightingTraitNumeric OwnedEmptyCell 15 9000",
    );
    let synth = g.session_cell_lighting(f.lo, f.schema, EMPTY_CELL).expect("synthesized XCLL");
    assert_eq!(synth.ambient, [80, 0, 0]);
    assert_eq!((synth.fog_near, synth.fog_far, synth.fog_clip_dist), (200.0, 4000.0, 9000.0));
    assert_eq!((synth.directional_fade, synth.fog_power), (1.0, 1.0));

    // Idempotent repeats across all four lighting setters do not churn lighting_revision.
    set(
        &mut g,
        "SetCellLightingTemplate OwnedLightCell OwnedLightB\n\
         SetCellFlag OwnedLightCell 8 1\n\
         SetLightingTemplateTraitNumeric OwnedLightB 15 24000",
    );
    let rev = g.lighting_token().1;
    set(
        &mut g,
        "SetCellLightingTemplate OwnedLightCell OwnedLightB\n\
         SetCellFlag OwnedLightCell 8 1\n\
         SetLightingTemplateTraitNumeric OwnedLightB 15 24000\n\
         SetInteriorLightingTraitNumeric OwnedEmptyCell 15 9000",
    );
    assert_eq!(g.lighting_token().1, rev, "identical lighting writes must not bump revision");

    // Non-finite f32 overflow (1e300) must not poison float lighting fields.
    set(
        &mut g,
        "let float e10 := 10000000000.0\n\
         let float e50 := e10 * e10 * e10 * e10 * e10\n\
         let float e150 := e50 * e50 * e50\n\
         let float e300 := e150 * e150\n\
         SetInteriorLightingTraitNumeric OwnedEmptyCell 15 e300\n\
         SetLightingTemplateTraitNumeric OwnedLightB 15 e300",
    );
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, EMPTY_CELL).unwrap().fog_clip_dist, 0.0);
    assert_eq!(g.session_cell_lighting(f.lo, f.schema, CELL).unwrap().fog_clip_dist, 20000.0);
}

/// `SetLightTraitNumeric` validates `LIGH`, wraps RGB channels `1..=3` modulo 256, and
/// preserves signed radius (`0`) [observed: `w6d2_22_light_grass_txst_traits_and_flags`].
#[test]
fn set_light_trait_numeric_wraps_rgb_and_validates_ligh() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(query(&mut g, "SetFunctionValue (GetLightTraitNumeric OwnedLightForm 0)"), XVal::Num(200.0));
    assert_eq!(query(&mut g, "SetFunctionValue (GetLightTraitNumeric OwnedLightForm 1)"), XVal::Num(177.0));
    set(&mut g, "SetLightTraitNumeric OwnedLightForm 0 -50\nSetLightTraitNumeric OwnedLightForm 1 300");
    assert_eq!(query(&mut g, "SetFunctionValue (GetLightTraitNumeric OwnedLightForm 0)"), XVal::Num(-50.0));
    assert_eq!(query(&mut g, "SetFunctionValue (GetLightTraitNumeric OwnedLightForm 1)"), XVal::Num(44.0));
    set(&mut g, "SetLightTraitNumeric OwnedLightForm 1 -10");
    assert_eq!(query(&mut g, "SetFunctionValue (GetLightTraitNumeric OwnedLightForm 1)"), XVal::Num(246.0));
    // Non-LIGH form (LGTM) is ignored without mutation.
    set(&mut g, "SetLightTraitNumeric OwnedLightA 0 99");
    assert_eq!(query(&mut g, "SetFunctionValue (GetLightTraitNumeric OwnedLightA 0)"), XVal::Num(0.0));
}
