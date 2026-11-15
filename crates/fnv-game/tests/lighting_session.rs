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
