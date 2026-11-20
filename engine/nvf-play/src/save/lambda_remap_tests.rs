//! Actual container reader controls for saves carrying runtime lambda handles.
use super::*;
use fnv_game::game::InstKey;
use fnv_script::vm::{Value, XVal};
use fnv_sim::FormId;

const PLACED_BASE: u32 = 0xD10;

struct OwnedDir(PathBuf);
impl Drop for OwnedDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A real snapshot over one plugin: a script variable holding a plugin record and,
/// with `lambda`, a second one holding a lambda the runtime returned.
fn saved(lambda: bool) -> (Vec<PluginEntry>, SaveBody, Option<u32>) {
    use crate::script_draw::tests::{play_over, record, sub, top};
    let play = play_over(&[top(b"STAT", &record(b"STAT", 0, PLACED_BASE, &[sub(b"EDID", b"OwnedRemapBase\0".to_vec())]))]);
    let mut state = ScriptState::over_plugins(play.lo.plugins);
    let g = &mut state.rt.as_mut().unwrap().game;
    let owner = InstKey::Ref { reference: FormId::PLAYER_REF };
    g.store(owner, None, 1, Value::Ref(PLACED_BASE), "owned remap fixture", false);
    let token = lambda.then(|| {
        let made = g.run_query("Begin Function {}\nSetFunctionValue (Begin Function {}\nSetFunctionValue 17\nEnd)\nEnd", Default::default()).unwrap();
        let XVal::Ref(token) = made else { panic!("the runtime returns a lambda: {made:?}") };
        g.store(owner, None, 2, Value::Ref(token), "owned remap fixture", false);
        token
    });
    let body = SaveBody { format: BODY_FORMAT.into(), game: g.checked_snapshot().unwrap(), engine: EngineSnapshot { format: ENGINE_FORMAT.into(), ..default() } };
    assert_eq!(body.game.lambdas.iter().map(|l| l.handle).collect::<Vec<_>>(), token.map(Some).into_iter().collect::<Vec<_>>());
    (vec![PluginEntry { name: "Test.esm".into(), size: 0 }], body, token)
}

fn write(label: &str, plugins: &[PluginEntry], body: &SaveBody) -> (OwnedDir, PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("nvf-lambda-remap-{label}-{}-{sequence}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("lambda.nvfsave");
    let mut header = SaveHeader::new(SaveKind::Manual);
    header.plugins = plugins.to_vec();
    fnv_save::write(&path, &header, &[], &serde_json::to_vec(body).unwrap(), false).unwrap();
    (OwnedDir(dir), path)
}

/// Every saved script-variable reference token (`{"ref": n}`) in the game snapshot.
fn ref_tokens(body: &SaveBody) -> Vec<u32> {
    fn walk(v: &serde_json::Value, out: &mut Vec<u32>) {
        match v {
            serde_json::Value::Object(o) if o.len() == 1 && o.get("ref").is_some_and(|n| n.is_u64()) => out.push(o["ref"].as_u64().unwrap() as u32),
            serde_json::Value::Object(o) => o.values().for_each(|e| walk(e, out)),
            serde_json::Value::Array(a) => a.iter().for_each(|e| walk(e, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&serde_json::to_value(&body.game).unwrap(), &mut out);
    out.sort_unstable();
    out
}

fn other() -> PluginEntry {
    PluginEntry { name: "OwnedOther.esm".into(), size: 0 }
}

/// The saved plugin first, then empty plugins up to a full 255-entry list, so the save has
/// a plugin at index FE (the lambda index); and the current order with that last plugin
/// moved to the front (index FE is also the gone slot, so nothing real is moved into it).
/// Only the header carries the extra names: the body is the one-plugin snapshot.
fn full_and_rotated(saved: &[PluginEntry]) -> (Vec<PluginEntry>, Vec<PluginEntry>) {
    let mut full = saved.to_vec();
    full.extend((1..=0xFE).map(|i| PluginEntry { name: format!("OwnedFill{i:03}.esp"), size: 0 }));
    assert_eq!(full.len(), 255);
    let mut rotated = full.clone();
    rotated.rotate_right(1);
    (full, rotated)
}

#[test]
fn a_lambda_save_keeps_tokens_and_handle_when_its_lambda_index_does_not_move() {
    let (plugins, body, token) = saved(true);
    let token = token.unwrap();
    assert!((fnv_game::game::LAMBDA_BASE..FormId::FIRST_DYNAMIC.0).contains(&token));
    assert_eq!(body.game.format, "fnv.game.state/2-lambda-handles");
    let (_dir, path) = write("kept", &plugins, &body);
    let mut appended = plugins.clone();
    appended.push(other());
    for current in [plugins.clone(), appended] {
        let (_, read, order) = read_for_load(&path, &current).unwrap();
        assert!(order.is_none());
        assert_eq!(read.game.format, "fnv.game.state/2-lambda-handles");
        assert_eq!(read.game.lambdas.iter().map(|l| l.handle).collect::<Vec<_>>(), vec![Some(token)]);
        assert_eq!(ref_tokens(&read), vec![PLACED_BASE, token]);
    }
    // Reordered, but the save had no plugin at FE: records move, lambda numbers do not.
    let mut moved = vec![other()];
    moved.extend(plugins);
    let (_, read, order) = read_for_load(&path, &moved).unwrap();
    assert!(order.is_some());
    assert_eq!(read.game.lambdas.iter().map(|l| l.handle).collect::<Vec<_>>(), vec![Some(token)]);
    assert_eq!(ref_tokens(&read), vec![0x0100_0000 | PLACED_BASE, token]);
}

#[test]
fn legacy_positional_lambda_loads_and_the_restored_token_calls_its_body() {
    use crate::script_draw::tests::{play_over, record, sub, top};
    use fnv_game::game::LAMBDA_BASE;

    const MAKER: FormId = FormId(0xD11);
    let source = "scn OwnedLegacyMaker\nstring_var sourceMarker\nBegin Function {}\nSetFunctionValue (Begin Function {}\nSetFunctionValue 17\nEnd)\nEnd\n";
    let mut header = vec![0; 20];
    header[18..20].copy_from_slice(&1u16.to_le_bytes()); // enabled SCPT
    let play = play_over(&[top(b"SCPT", &record(b"SCPT", 0, MAKER.0, &[
        sub(b"EDID", b"OwnedLegacyMaker\0".to_vec()),
        sub(b"SCHR", header),
        sub(b"SCDA", vec![]),
        sub(b"SCTX", source.as_bytes().to_vec()),
    ]))]);
    let mut state = ScriptState::over_plugins(play.lo.plugins);
    let game = &mut state.rt.as_mut().unwrap().game;
    let unit = game.data.script(MAKER).expect("the owned maker must load as an enabled SCPT");
    assert!(game.data.source_units.iter().any(|s| s.key == unit.key() && s.error.is_none()), "the maker must compile from SCTX");
    let callback = game.run_function(&XVal::Ref(MAKER.0), None, vec![], None, Default::default()).unwrap();
    assert_eq!(callback, XVal::Ref(LAMBDA_BASE), "legacy position zero names this lambda");
    let owner = InstKey::Ref { reference: FormId::PLAYER_REF };
    game.store(owner, None, 2, Value::Ref(LAMBDA_BASE), "owned legacy fixture", false);

    let mut legacy = SaveBody { format: BODY_FORMAT.into(), game: game.checked_snapshot().unwrap(),
        engine: EngineSnapshot { format: ENGINE_FORMAT.into(), ..default() } };
    assert_eq!(legacy.game.lambdas.len(), 1);
    assert_eq!(legacy.game.lambdas[0].handle, Some(LAMBDA_BASE));
    legacy.game.format = "fnv.game.state/1".into();
    legacy.game.lambdas[0].handle = None;
    let plugins = [PluginEntry { name: "Test.esm".into(), size: 0 }];
    let (_dir, path) = write("legacy-restore", &plugins, &legacy);
    let (_, body, order) = read_for_load(&path, &plugins).unwrap();
    assert!(order.is_none());
    assert_eq!(body.game.format, "fnv.game.state/1");
    assert_eq!(body.game.lambdas[0].handle, None);

    let world = fnv_sim::World::restore(game.world.forms, game.world.placed, &body.game.sim).unwrap();
    let mut restored = fnv_game::Game::restore(game.data, world, game.cfg, &body.game).unwrap();
    let Value::Ref(token) = restored.instances[&owner].vars[&2] else { panic!("the legacy token must restore as a reference") };
    assert_eq!(token, LAMBDA_BASE);
    assert_eq!(restored.run_function(&XVal::Ref(token), None, vec![], None, Default::default()).unwrap(), XVal::Num(17.0));
}

#[test]
fn a_lambda_save_is_refused_when_its_lambda_index_plugin_moves() {
    let (plugins, body, _) = saved(true);
    let (full, rotated) = full_and_rotated(&plugins);
    let mut legacy = body.clone();
    legacy.game.format = "fnv.game.state/1".into();
    legacy.game.lambdas[0].handle = None;
    for (label, body) in [("explicit", body), ("legacy", legacy)] {
        let (_dir, path) = write(label, &full, &body);
        let bytes = std::fs::read(&path).unwrap();
        let Err(error) = read_for_load(&path, &rotated) else { panic!("{label}: moving index FE must refuse a lambda save") };
        assert!(error.contains("runtime lambda definitions") && error.contains("lambda.nvfsave"), "{label}: {error}");
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let (_, read, order) = read_for_load(&path, &full).unwrap();
        assert!(order.is_none(), "{label}: the identical full order still loads");
        assert_eq!(read.game.lambdas.len(), 1);
    }
}

#[test]
fn a_lambda_free_save_still_remaps_with_a_reordered_plugin_list() {
    let (plugins, body, token) = saved(false);
    assert!(token.is_none());
    assert_eq!(body.game.format, "fnv.game.state/1");
    let (_dir, path) = write("plain", &plugins, &body);
    let (_, read, order) = read_for_load(&path, &plugins).unwrap();
    assert!(order.is_none());
    assert_eq!(ref_tokens(&read), vec![PLACED_BASE]);
    let mut moved = vec![other()];
    moved.extend(plugins.clone());
    let (_, read, order) = read_for_load(&path, &moved).unwrap();
    assert!(order.is_some());
    assert!(read.game.lambdas.is_empty());
    assert_eq!(ref_tokens(&read), vec![0x0100_0000 | PLACED_BASE]);
    let (full, rotated) = full_and_rotated(&plugins);
    let (_dir, path) = write("plain-full", &full, &body);
    let (_, read, order) = read_for_load(&path, &rotated).unwrap();
    assert!(order.is_some());
    assert_eq!(ref_tokens(&read), vec![0x0100_0000 | PLACED_BASE]);
}
