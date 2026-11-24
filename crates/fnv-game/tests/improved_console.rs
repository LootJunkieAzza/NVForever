//! Improved Console parity tests (`docs/design/console.md`):
//! - Persistent console locals (`int`/`short`/`long`/`float`/`ref`/`string_var`/`array_var`,
//!   inline `let` declarations, re-declarations, error atomicity, and new-game / save-reload reset)
//! - Transparent result printing without an `eval` prefix (numbers, forms, strings, arrays,
//!   compound expressions, extender calls, and temporary-array sweeping)
//! - `"Improved Console"` plugin registration (`IsPluginInstalled` / `GetPluginVersion`).
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    game::{Game, GameConfig, RunCtx},
    GameData,
};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, World};

const GLOBAL_ID: u32 = 0x0000_0800;
const BOX_ID: u32 = 0x0000_0810;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let mut bytes = tes4();
    bytes.extend(top(
        b"GLOB",
        &record(
            b"GLOB",
            0,
            GLOBAL_ID,
            &[
                sub(b"EDID", z("ConsoleTestGlobal")),
                sub(b"FNAM", vec![b'f']),
                sub(b"FLTV", 12.0f32.to_le_bytes().to_vec()),
            ],
        ),
    ));
    bytes.extend(top(
        b"MISC",
        &record(
            b"MISC",
            0,
            BOX_ID,
            &[
                sub(b"EDID", z("ConsoleTestBox")),
                sub(b"FULL", z("Test Box")),
                sub(b"MODL", z(r"Test\Box.nif")),
            ],
        ),
    ));
    bytes.extend(top(
        b"NPC_",
        &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]),
    ));
    let plugins = vec![Plugin::parse(&bytes, "ImprovedConsoleTest.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(
        &lo,
        &schema,
        FunctionTable::load(&path).unwrap(),
        &json,
        &refs,
    );
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    Game::new(
        &f.data,
        World::new(&f.forms, &f.refs, Default::default()),
        GameConfig {
            strict: true,
            ..Default::default()
        },
    )
}

#[test]
fn persistent_console_locals_span_all_types_and_transparently_print_results() {
    let f = fixture();
    let mut g = game(&f);
    let run = |g: &mut Game<'_>, line: &str| g.run_console_line(line, RunCtx::default()).unwrap().value;

    // Bare declarations persist without emitting a spurious value.
    assert_eq!(run(&mut g, "int x"), None);
    assert!(g.has_console_local("x"));
    assert_eq!(run(&mut g, "x"), Some("0".into()));

    // Classic `set`, `let`, compound assignment, and bare expression reading.
    assert_eq!(run(&mut g, "set x to 10"), Some("10".into()));
    assert_eq!(run(&mut g, "let x := x + 5"), Some("15".into()));
    assert_eq!(run(&mut g, "x += 2"), Some("17".into()));
    assert_eq!(run(&mut g, "x"), Some("17".into()));

    // Float declaration with initializer + bare arithmetic (including decimals and negatives).
    assert_eq!(run(&mut g, "float f = 2.5"), Some("2.5".into()));
    assert_eq!(run(&mut g, "f + 1.25"), Some("3.75".into()));
    assert_eq!(run(&mut g, "-5 + 10"), Some("5".into()));
    assert_eq!(run(&mut g, "1.5 + 2.5"), Some("4".into()));

    // Ref local persists and works as calling receiver and form expression.
    assert_eq!(run(&mut g, "ref r = player"), Some("0x00000014 0x00000014".into()));
    assert_eq!(g.console_local_ref("r"), Some(FormId::PLAYER_REF));
    assert_eq!(run(&mut g, "r"), Some("0x00000014 0x00000014".into()));
    assert_eq!(run(&mut g, "r.SetAV Health 85"), Some("0".into()));
    assert_eq!(run(&mut g, "r.GetAV Health"), Some("85".into()));
    assert_eq!(run(&mut g, "r.GetAV Health + x"), Some("102".into()));

    // Inline declaration in `let` and reading via `eval_console_expression`.
    assert_eq!(run(&mut g, "let int bonus := 3"), Some("3".into()));
    assert_eq!(g.eval_console_expression("x + bonus", RunCtx::default()).unwrap(), "20");

    // String local persists and mutates in place.
    assert_eq!(run(&mut g, "string_var s = \"hello\""), Some("\"hello\"".into()));
    assert_eq!(run(&mut g, "let s += \" world\""), Some("\"hello world\"".into()));
    assert_eq!(run(&mut g, "s"), Some("\"hello world\"".into()));
    assert_eq!(run(&mut g, "sv_Length s"), Some("11".into()));

    // Array local persists across lines and stays retained in ExtStore.
    let arrays_before = g.ext.arrays.len();
    assert_eq!(run(&mut g, "array_var a = ar_List 1, 9"), Some("[0: 1, 1: 9]".into()));
    assert_eq!(g.ext.arrays.len(), arrays_before + 1, "bound array_var stays retained");
    assert_eq!(run(&mut g, "let a[0] := 42"), Some("42".into()));
    assert_eq!(run(&mut g, "a"), Some("[0: 42, 1: 9]".into()));
    assert_eq!(run(&mut g, "a[0] + a[1]"), Some("51".into()));
    assert_eq!(run(&mut g, "ar_Size a"), Some("2".into()));

    // Temporary array printed transparently without `eval` is swept after formatting.
    assert_eq!(run(&mut g, "ar_List 7, 8"), Some("[0: 7, 1: 8]".into()));
    assert_eq!(g.ext.arrays.len(), arrays_before + 1, "temporary array swept after printing");

    // Releasing the bound array_var frees it cleanly.
    assert_eq!(run(&mut g, "let a := ar_Null"), Some("0".into()));
    assert_eq!(g.ext.arrays.len(), arrays_before);
}

#[test]
fn transparent_result_printing_handles_extender_calls_globals_and_plugin_checks() {
    let f = fixture();
    let mut g = game(&f);
    let run = |g: &mut Game<'_>, line: &str| g.run_console_line(line, RunCtx::default()).unwrap().value;

    assert_eq!(run(&mut g, "5 + 5"), Some("10".into()));
    assert_eq!(run(&mut g, "ConsoleTestGlobal"), Some("12".into()));
    assert_eq!(run(&mut g, "ConsoleTestGlobal + 8"), Some("20".into()));
    assert_eq!(run(&mut g, "GetModelPath ConsoleTestBox"), Some(r#""Test\\Box.nif""#.into()));
    assert_eq!(run(&mut g, "IsPluginInstalled \"Improved Console\""), Some("1".into()));
    assert_eq!(run(&mut g, "GetPluginVersion \"Improved Console\""), Some("3".into()));
    assert_eq!(run(&mut g, "GetNVSEVersion + 1"), Some("7".into()));
    // Selected reference flows into bare calls and compound expressions.
    let on_player = RunCtx { self_ref: Some(FormId::PLAYER_REF), ..Default::default() };
    g.run_console_line("SetAV Health 60", on_player).unwrap();
    assert_eq!(g.run_console_line("GetAV Health + 4", on_player).unwrap().value, Some("64".into()));
}

#[test]
fn redeclaration_errors_and_new_game_or_reload_reset_console_locals() {
    let f = fixture();
    let mut g = game(&f);
    let run = |g: &mut Game<'_>, line: &str| g.run_console_line(line, RunCtx::default());

    assert_eq!(run(&mut g, "string_var v = \"keep\"").unwrap().value, Some("\"keep\"".into()));
    let strings_with_v = g.ext.strings.len();
    assert!(strings_with_v > 0);

    // Same-type bare redeclaration preserves the existing value.
    assert_eq!(run(&mut g, "string_var v").unwrap().value, None);
    assert_eq!(run(&mut g, "v").unwrap().value, Some("\"keep\"".into()));

    // Compile error must not disturb existing locals.
    assert!(run(&mut g, "int broken = 1 +").is_err());
    assert!(!g.has_console_local("broken"));
    assert_eq!(run(&mut g, "v").unwrap().value, Some("\"keep\"".into()));

    // Runtime error returns Err cleanly without leaking temporary arrays.
    let arrays_before = g.ext.arrays.len();
    assert!(run(&mut g, "1 / 0").is_err());
    assert_eq!(g.ext.arrays.len(), arrays_before);

    // Different-type redeclaration destroys the old string handle and resets the slot.
    assert_eq!(run(&mut g, "int v = 99").unwrap().value, Some("99".into()));
    assert_eq!(g.ext.strings.len(), strings_with_v - 1, "old string_var handle destroyed");
    assert_eq!(run(&mut g, "v").unwrap().value, Some("99".into()));

    // Ref -> float redeclaration clears ref-ness cleanly.
    assert_eq!(run(&mut g, "ref r = player").unwrap().value, Some("0x00000014 0x00000014".into()));
    assert_eq!(g.console_local_ref("r"), Some(FormId::PLAYER_REF));
    assert_eq!(run(&mut g, "float r = 3.5").unwrap().value, Some("3.5".into()));
    assert_eq!(g.console_local_ref("r"), None);
    assert_eq!(run(&mut g, "r").unwrap().value, Some("3.5".into()));

    // Array held by a console local is freed on new_game().
    assert_eq!(run(&mut g, "array_var a = ar_List 1, 2").unwrap().value, Some("[0: 1, 1: 2]".into()));
    assert_eq!(g.ext.arrays.len(), arrays_before + 1);
    g.new_game();
    assert!(!g.has_console_local("v"));
    assert!(!g.has_console_local("a"));
    assert_eq!(g.ext.arrays.len(), arrays_before, "new_game releases console array_var handles");
    assert!(run(&mut g, "v").is_err());

    // Save reload + take_session/put_session does not carry console locals across loads.
    assert_eq!(run(&mut g, "int tempLocal = 7").unwrap().value, Some("7".into()));
    let snap = g.snapshot();
    let session = g.take_session();
    let mut restored = Game::restore(
        &f.data,
        World::new(&f.forms, &f.refs, Default::default()),
        GameConfig { strict: true, ..Default::default() },
        &snap,
    )
    .unwrap();
    restored.put_session(session);
    assert!(!restored.has_console_local("tempLocal"));
    assert!(restored.run_console_line("tempLocal", RunCtx::default()).is_err());
}
