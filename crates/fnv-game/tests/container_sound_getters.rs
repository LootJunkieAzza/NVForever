//! Owned record/instance/session fixture; no process-wide roots, environment or modes
//! are changed, so this belongs in the combined integration binary.
use crate::common;
use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{GameData, game::{Game, GameConfig}};
use fnv_script::{FunctionTable, vm::XVal};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

#[test]
fn container_sound_getters_share_base_reference_and_live_overrides() {
    let sound = |id, name: &str| record(b"SOUN",0,id,&[sub(b"EDID",z(name))]);
    let base = record(b"CONT",0,0x800,&[sub(b"EDID",z("OwnContainer")),sub(b"SNAM",0x801u32.to_le_bytes().to_vec()),sub(b"QNAM",0x802u32.to_le_bytes().to_vec())]);
    let reference = record(b"REFR",0x400,0x804,&[sub(b"EDID",z("OwnContainerRef")),sub(b"NAME",0x800u32.to_le_bytes().to_vec()),sub(b"DATA",vec![0;24])]);
    let cell = record(b"CELL",0,0x805,&[sub(b"EDID",z("OwnCell")),sub(b"DATA",vec![1])]);
    let placed = group(0x805u32.to_le_bytes(),6,&group(0x805u32.to_le_bytes(),9,&reference));
    let bytes = [tes4(),top(b"CONT",&[base, record(b"CONT",0,0x806,&[sub(b"EDID",z("OwnEmptyContainer"))]), record(b"CONT",0,0x807,&[sub(b"EDID",z("OwnOtherContainer")),sub(b"SNAM",0x803u32.to_le_bytes().to_vec())])].concat()),top(b"SOUN",&[sound(0x801,"OwnOpen"),sound(0x802,"OwnClose"),sound(0x803,"OwnReplacement")].concat()),top(b"CELL",&group(0i32.to_le_bytes(),2,&group(0i32.to_le_bytes(),3,&[cell,placed].concat())))].concat();
    let plugins = vec![Plugin::parse(&bytes,"Own.esm".into()).unwrap()];
    let order = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"),"/../../schemas/fnv")).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&order,&schema);
    let refs = RefTable::build(&order,&schema);
    let data = GameData::build(&order,&schema,FunctionTable::load(&path).unwrap(),&json,&refs);
    let mut game = Game::new(&data,World::new(&forms,&refs,SimConfig::default()),GameConfig {strict:true,..Default::default()});
    let query = |game: &mut Game<'_>, body: &str| game.run_query(&format!("ref absent\nBegin Function {{}}\n{body}\nEnd"),Default::default()).unwrap();
    assert_eq!(query(&mut game,"SetFunctionValue (GetContainerOpenSound OwnContainer)"),XVal::Ref(0x801));
    assert_eq!(query(&mut game,"SetFunctionValue (GetContainerCloseSound OwnContainer)"),XVal::Ref(0x802));
    assert_eq!(query(&mut game,"SetFunctionValue OwnContainerRef.GetContainerOpenSound"),XVal::Ref(0x801));
    assert_eq!(query(&mut game,"SetFunctionValue OwnContainerRef.GetContainerCloseSound"),XVal::Ref(0x802));
    assert_eq!(query(&mut game,"SetFunctionValue (GetContainerOpenSound)"),XVal::Ref(0));
    assert_eq!(query(&mut game,"SetFunctionValue (GetContainerOpenSound absent)"),XVal::Ref(0));
    assert_eq!(query(&mut game,"SetFunctionValue (GetContainerCloseSound OwnEmptyContainer)"),XVal::Ref(0));
    assert_eq!(query(&mut game,"SetFunctionValue (OwnContainerRef.GetContainerOpenSound OwnOtherContainer)"),XVal::Ref(0x803));
    assert_eq!(query(&mut game,"SetFunctionValue (OwnContainerRef.GetContainerOpenSound absent)"),XVal::Ref(0));
    assert_eq!(query(&mut game,"SetFunctionValue (OwnContainerRef.GetContainerOpenSound OwnOpen)"),XVal::Ref(0));
    query(&mut game,"SetContainerSound OwnContainer 0 OwnReplacement\nSetContainerSound OwnContainer 1 OwnOpen");
    assert_eq!(query(&mut game,"SetFunctionValue OwnContainerRef.GetContainerOpenSound"),XVal::Ref(0x803));
    assert_eq!(query(&mut game,"SetFunctionValue OwnContainerRef.GetContainerCloseSound"),XVal::Ref(0x801));
    assert_eq!(query(&mut game,"SetFunctionValue (GetContainerCloseSound OwnOpen)"),XVal::Ref(0));
}
