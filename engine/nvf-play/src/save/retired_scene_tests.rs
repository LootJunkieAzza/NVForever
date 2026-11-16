//! Actual save entry, container writer/reader and load-order refusal controls.
use super::*;
use fnv_script::vm::XVal;
use fnv_sim::FormId;

struct OwnedFile(PathBuf);
impl Drop for OwnedFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_dir(self.0.parent().unwrap());
    }
}

fn query(world: &mut World, body: &str) -> XVal {
    world.resource_scope(|world, mut sim: Mut<SimState>| {
        let mut scripts = world.resource_mut::<ScriptState>();
        let rt = scripts.rt.as_mut().unwrap();
        crate::script::with_world(rt, &mut sim, |rt| rt.game.run_query(
            &format!("ref ownBase\nref ownPlaced\nBegin Function {{}}\n{body}\nEnd"), Default::default(),
        ).unwrap())
    })
}

fn fixture() -> (World, OwnedFile) {
    use crate::script_draw::tests::{play_over, record, sub, top};
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let string = |sig: &[u8; 4], text: &str| sub(sig, [text.as_bytes(), &[0]].concat());
    let play = play_over(&[
        top(b"NPC_", &record(b"NPC_", 0, 7, &[string(b"EDID", "Player")])),
        top(b"STAT", &record(b"STAT", 0, 0xD10, &[string(b"EDID", "OwnedSceneBase"), string(b"MODL", "owned\\original.nif")])),
    ]);
    let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("nvf-retired-scene-{}-{sequence}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("retired.nvfsave");
    std::fs::write(&path, b"owned previous save").unwrap();
    let scripts = ScriptState::over_plugins(play.lo.plugins);
    let sim = SimState::new(play.lo, play.schema, play.index);
    let mut world = World::new();
    world.insert_resource(play);
    world.insert_resource(sim);
    world.insert_resource(scripts);
    world.insert_resource(SaveConfig::with_dir(Some(dir)));
    world.init_resource::<SaveHooks>();
    world.init_resource::<SaveWrites>();
    world.resource_mut::<ScriptState>().rt.as_mut().unwrap().game.run_source(
        "SetEventHandler \"SaveGame\" (begin function {string_var ownName}\nSetModLocalData \"owned.save.callback\" 1\nend)", Default::default(),
    ).unwrap();
    (world, OwnedFile(path))
}

fn retire(world: &mut World, temporary: bool) -> (FormId, FormId) {
    let prepare = if temporary { "let ownBase := TempCloneForm OwnedSceneBase 1\nSetModelPath \"owned\\saved-retired.nif\" ownBase" } else { "let ownBase := OwnedSceneBase" };
    let XVal::Ref(id) = query(world, &format!("{prepare}\nlet ownPlaced := player.PlaceAtMe ownBase\nownPlaced.MarkForDelete\nSetFunctionValue ownPlaced")) else { panic!("placed reference") };
    assert_ne!(id, 0);
    let state = world.resource::<SimState>().world.peek(FormId(id)).unwrap();
    assert!(state.deleted());
    (FormId(id), state.base)
}

#[test]
fn deleted_scene_save_runs_callback_and_roundtrips_real_container_without_revival() {
    let (mut world, file) = fixture();
    let (reference, alias) = retire(&mut world, true);
    save_now(&mut world, &SaveRequest { name: "retired".into(), kind: SaveKind::Manual })
        .expect("retired provenance must be admitted after the actual SaveGame callback");
    assert_eq!(query(&mut world, "SetFunctionValue (GetModLocalData \"owned.save.callback\")"), XVal::Num(1.0));
    assert_eq!(std::fs::read(&file.0).unwrap(), b"owned previous save", "enqueue is not an early overwrite");
    let writes = world.resource::<SaveWrites>();
    assert_eq!(writes.waiting.len(), 1);
    let write = &writes.waiting[0];
    assert_eq!(write.path, file.0);
    fnv_save::write(&file.0, &write.header, &[], &write.body, false).unwrap();
    let (_, body, order) = read_for_load(&file.0, &write.header.plugins).unwrap();
    assert!(order.is_none());
    assert_eq!(body.game.sim.deleted_created.get(&reference), Some(&alias));
    assert!(!body.game.sim.refs.contains_key(&reference));
    let sim = world.resource::<SimState>();
    let runtime = world.resource::<ScriptState>().rt.as_ref().unwrap();
    let restored_world = fnv_sim::World::restore(sim.world.forms, sim.world.placed, &body.game.sim).unwrap();
    let restored = fnv_game::Game::restore(runtime.game.data, restored_world, runtime.game.cfg, &body.game).unwrap();
    assert_eq!(restored.world.deleted_base(reference), Some(alias));
    assert!(restored.world.peek(reference).is_none());
    assert_eq!(restored.unclone(alias), alias, "load must not make the temporary base valid");
    assert_eq!(serde_json::to_value(restored.checked_snapshot().unwrap()).unwrap()["retired_scene_bases"],
        serde_json::to_value(&body.game).unwrap()["retired_scene_bases"]);
}

#[test]
fn retired_scene_container_allows_appended_plugins_but_refuses_identity_remapping() {
    let (mut world, file) = fixture();
    let (reference, alias) = retire(&mut world, true);
    save_now(&mut world, &SaveRequest { name: "retired".into(), kind: SaveKind::Manual }).unwrap();
    let writes = world.resource::<SaveWrites>(); let write = &writes.waiting[0];
    fnv_save::write(&file.0, &write.header, &[], &write.body, false).unwrap();
    let bytes = std::fs::read(&file.0).unwrap();
    let other = PluginEntry { name: "OwnedOther.esm".into(), size: 0 };
    let mut appended = write.header.plugins.clone(); appended.push(other.clone());
    let (_, body, _) = read_for_load(&file.0, &appended).unwrap();
    assert_eq!(body.game.sim.deleted_created.get(&reference), Some(&alias));
    let mut moved = vec![other]; moved.extend(write.header.plugins.clone());
    for current in [moved, Vec::new()] {
        let error = read_for_load(&file.0, &current).unwrap_err();
        assert!(error.contains("retired temporary scene identities"), "{error}");
        assert_eq!(std::fs::read(&file.0).unwrap(), bytes);
    }
}

#[test]
fn ordinary_deleted_reference_still_uses_real_plugin_remapping() {
    let (mut world, file) = fixture();
    let (reference, original) = retire(&mut world, false);
    assert_eq!(original, FormId(0xD10));
    save_now(&mut world, &SaveRequest { name: "retired".into(), kind: SaveKind::Manual }).unwrap();
    let writes = world.resource::<SaveWrites>(); let write = &writes.waiting[0];
    assert_eq!(write.header.plugins.len(), 1);
    fnv_save::write(&file.0, &write.header, &[], &write.body, false).unwrap();
    let mut current = vec![PluginEntry { name: "OwnedEarlier.esm".into(), size: 0 }];
    current.extend(write.header.plugins.clone());
    let (_, body, order) = read_for_load(&file.0, &current).unwrap();
    assert!(order.is_some());
    assert_eq!(body.game.sim.deleted_created.get(&reference), Some(&FormId(0x0100_0D10)));
    assert_eq!(body.game.format, fnv_game::game::SNAPSHOT_FORMAT);
}
