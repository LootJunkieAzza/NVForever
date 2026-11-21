//! Authored process-boundary controls, not a rendered window or native-save test.
use super::*;
use clap::Parser;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("nvf-death-handoff-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("owned-handoff-test"), b"authored recovery handoff").unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

fn bounded_output(mut child: std::process::Child) -> std::process::Output {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if child.try_wait().unwrap().is_some() { return child.wait_with_output().unwrap(); }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!("owned handoff child timed out: {output:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn authored_save(path: &Path) -> Vec<u8> {
    let schema_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let lo = fnv_esm::LoadOrder::new(&[]).unwrap();
    let schema = fnv_esm::Schema::load_dir(schema_root.join("fnv")).unwrap();
    let functions = schema_root.join("script/functions.json");
    let json = serde_json::from_slice(&std::fs::read(&functions).unwrap()).unwrap();
    let forms = Box::leak(Box::new(fnv_sim::FormStore::build(&lo, &schema)));
    let refs = Box::leak(Box::new(fnv_sim::RefTable::build(&lo, &schema)));
    let data = Box::leak(Box::new(fnv_game::GameData::build(&lo, &schema, fnv_script::FunctionTable::load(&functions).unwrap(), &json, refs)));
    let game = fnv_game::Game::new(data, fnv_sim::World::new(forms, refs, fnv_sim::SimConfig::default()), fnv_game::GameConfig::default());
    let body = nvf_play::save::SaveBody {
        format: nvf_play::save::BODY_FORMAT.into(), game: game.snapshot(),
        engine: nvf_play::save::EngineSnapshot { format: nvf_play::save::ENGINE_FORMAT.into(), ..Default::default() },
    };
    let mut header = fnv_save::SaveHeader::new(fnv_save::SaveKind::Auto);
    header.player_name = "Authored living recovery sentinel".into();
    header.place.feet = [123.0, -45.0, 67.0];
    header.place.heading_deg = 81.0;
    header.game_hour = 13.0;
    fnv_save::write(path, &header, &[], &serde_json::to_vec(&body).unwrap(), false).unwrap();
    let (checked, _, order) = nvf_play::save::read_for_load(path, &[]).unwrap();
    assert_eq!(checked.player_name, header.player_name);
    assert!(order.is_none());
    std::fs::read(path).unwrap()
}

#[test]
fn recovery_handoff_delivers_exact_owned_save_and_unaided_profile_to_a_fresh_process() {
    let root = Scratch::new();
    let home = root.0.join("death-recovery-home");
    std::fs::create_dir_all(home.join("saves")).unwrap();
    let save = home.join("saves/authored-living.nvfsave");
    let original = authored_save(&save);
    let keep = crate::restart_args(&["--play", "--data", "authored-read-only-data", "--run-seconds", "90", "--cell", "OldCell", "--load", "old"].map(String::from));
    let request = LoadRequest { path: save.clone() };
    let ready = root.0.join("ready");
    let args = new_run_args(&keep, Some(&request), None, &ready);
    // Record the real OS-delivered game argv, then wait for a bounded fresh Rust child
    // in the same delivered environment. That child uses the actual Args parser,
    // SaveConfig::default and start_from_save without starting a renderer.
    let recorder = r#"
import json, os, subprocess, sys
exe, child, report = sys.argv[1:4]
game_argv = sys.argv[4:]
with open(report, 'w', encoding='utf8') as output:
    json.dump({'argv': game_argv, 'home': os.environ.get('NVFOREVER_HOME'),
               'driving': {k:v for k,v in os.environ.items() if k.startswith('NVF_PLAY_TEST_')},
               'clock': os.environ.get('NVF_PLAY_AIDS_FROM_SCRIPTS')}, output)
os.environ['NVF_DEATH_DELIVERED_ARGV'] = json.dumps(game_argv)
completed = subprocess.run([exe, '--ignored', '--exact', child, '--nocapture'],
                           env=os.environ, timeout=15, check=False)
raise SystemExit(completed.returncode)
"#;
    let mut environment: std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString> = std::env::vars_os()
        .filter(|(key, _)| !key.to_str().is_some_and(|key| {
            let key = key.to_ascii_uppercase();
            ["NVF_PLAY_TEST_", "NVF_PLAY_DEBUG_", "NVF_PLAY_AIDS_"].iter().any(|prefix| key.starts_with(prefix))
        })).collect();
    for (key, value) in [
        ("NVF_PLAY_TEST_CONSOLE", "24:player.killactor"),
        ("NVF_PLAY_TEST_HIT", "8:player:500:WeapKnife"),
        ("NVF_PLAY_TEST_DEATH_RECOVERY", "1"),
        ("NVF_PLAY_AIDS_FROM_SCRIPTS", "1"),
    ] { environment.insert(key.into(), value.into()); }
    environment.insert("NVFOREVER_HOME".into(), home.as_os_str().to_owned());
    environment.insert("NVF_DEATH_HANDOFF_ROOT".into(), root.0.as_os_str().to_owned());
    environment.insert("NVF_DEATH_HANDOFF_SAVE".into(), save.as_os_str().to_owned());
    let report = root.0.join("delivered.json");
    let Some(python) = python3() else {
        println!("skipped: no Python 3 runs here (set NVF_PYTHON to one)");
        return;
    };
    let mut command = std::process::Command::new(&python[0]);
    command.args(&python[1..]).arg("-c").arg(recorder).arg(std::env::current_exe().unwrap())
        .arg("handoff::death_smoke_tests::unaided_successor_child").arg(&report).args(&args)
        .current_dir(&root.0).env_clear().envs(&environment)
        .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    remove_driving_aids(&mut command, environment.keys().cloned());
    let output = bounded_output(command.spawn().unwrap());
    assert!(output.status.success(), "fresh successor: {output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("UNAIDED_RECOVERY_SUCCESSOR_COMPLETE"), "{output:?}");
    let delivered: serde_json::Value = serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    assert_eq!(delivered["argv"], serde_json::to_value(&args).unwrap());
    assert_eq!(delivered["home"], home.to_str().unwrap());
    assert_eq!(delivered["driving"], serde_json::json!({}));
    assert!(delivered["clock"].is_null());
    assert_eq!(std::fs::read(save).unwrap(), original, "the staged original is read only");
}

#[test]
#[ignore = "entered only by the owned real-process handoff control"]
fn unaided_successor_child() {
    let root = PathBuf::from(std::env::var_os("NVF_DEATH_HANDOFF_ROOT").expect("owned parent only")).canonicalize().unwrap();
    assert_eq!(std::env::current_dir().unwrap().canonicalize().unwrap(), root);
    assert_eq!(std::fs::read(root.join("owned-handoff-test")).unwrap(), b"authored recovery handoff");
    assert!(!std::env::vars_os().any(|(key, _)| key.to_str().is_some_and(nvf_play::drives_run)));
    assert!(std::env::var_os("NVF_PLAY_AIDS_FROM_SCRIPTS").is_none());
    let home = root.join("death-recovery-home").canonicalize().unwrap();
    let config = nvf_play::save::SaveConfig::default();
    assert_eq!(config.dir.as_ref().unwrap().canonicalize().unwrap(), home.join("saves"));
    assert!(config.autosave_travel && config.autosave_fast_travel && config.autosave_wait && config.autosave_sleep && config.autosave_scripts);
    let delivered: Vec<String> = serde_json::from_str(&std::env::var("NVF_DEATH_DELIVERED_ARGV").unwrap()).unwrap();
    let mut parsed = crate::Args::try_parse_from(std::iter::once("nvforever".to_owned()).chain(delivered)).unwrap();
    let selected = PathBuf::from(std::env::var_os("NVF_DEATH_HANDOFF_SAVE").unwrap()).canonicalize().unwrap();
    assert!(selected.starts_with(home.join("saves")));
    assert!(parsed.play && !parsed.menu && !parsed.new_game);
    assert_eq!(parsed.run_seconds, Some(90.0));
    assert_eq!(parsed.data.as_deref(), Some(Path::new("authored-read-only-data")));
    assert_eq!(Path::new(parsed.load.as_ref().unwrap()).canonicalize().unwrap(), selected);
    let before = std::fs::read(&selected).unwrap();
    let opened = crate::world::OpenedPlugins { plugins: Vec::new(), dropped: Vec::new() };
    let pending = crate::start_from_save(&parsed.load.clone().unwrap(), &opened, &mut parsed, None).unwrap();
    assert_eq!(pending.path.canonicalize().unwrap(), selected);
    assert_eq!(parsed.heading, 81.0);
    assert_eq!(parsed.hour, 13.0);
    assert_eq!(parsed.pos.as_deref(), Some(format!("123,-45,{}", 67.0 + nvf_play::player::EYE_HEIGHT).as_str()));
    let (header, body, _) = nvf_play::save::read_for_load(&selected, &[]).unwrap();
    assert_eq!(header.player_name, "Authored living recovery sentinel");
    assert_eq!(serde_json::to_value(pending.body).unwrap(), serde_json::to_value(body).unwrap());
    assert_eq!(std::fs::read(selected).unwrap(), before);
    println!("UNAIDED_RECOVERY_SUCCESSOR_COMPLETE");
}

#[test]
fn handoff_retains_existing_exit_and_measuring_exceptions() {
    let mut command = std::process::Command::new("owned-recorder-not-spawned");
    for key in [EXIT_AID, "NVF_PLAY_TEST_SAMPLE", "NVFOREVER_HOME", "NVF_PLAY_TEST_CONSOLE", "NVF_PLAY_AIDS_FROM_SCRIPTS"] { command.env(key, "sentinel"); }
    remove_driving_aids(&mut command, [EXIT_AID, "NVF_PLAY_TEST_SAMPLE", "NVFOREVER_HOME", "NVF_PLAY_TEST_CONSOLE", "NVF_PLAY_AIDS_FROM_SCRIPTS"].map(std::ffi::OsString::from));
    let values: std::collections::BTreeMap<_, _> = command.get_envs().collect();
    for key in [EXIT_AID, "NVF_PLAY_TEST_SAMPLE", "NVFOREVER_HOME"] { assert_eq!(values[std::ffi::OsStr::new(key)], Some(std::ffi::OsStr::new("sentinel"))); }
    for key in ["NVF_PLAY_TEST_CONSOLE", "NVF_PLAY_AIDS_FROM_SCRIPTS"] { assert_eq!(values[std::ffi::OsStr::new(key)], None); }
}

/// A Python 3 that runs, as a command and its first arguments: `NVF_PYTHON` (or `PYTHON`)
/// when set, else `py -3` (Windows), `python`, `python3`. Each is asked for its major
/// version, so Windows' App Execution Alias stub (`python` that prints "Python was not
/// found" and fails) is passed over (#3275). `None`: none runs.
fn python3() -> Option<Vec<std::ffi::OsString>> {
    let mut tries: Vec<Vec<std::ffi::OsString>> = Vec::new();
    for key in ["NVF_PYTHON", "PYTHON"] {
        if let Some(p) = std::env::var_os(key).filter(|p| !p.is_empty()) {
            tries.push(vec![p]);
        }
    }
    if cfg!(windows) {
        tries.push(vec!["py".into(), "-3".into()]);
    }
    tries.push(vec!["python".into()]);
    tries.push(vec!["python3".into()]);
    tries.into_iter().find(|t| {
        std::process::Command::new(&t[0])
            .args(&t[1..])
            .args(["-c", "import sys; print(sys.version_info[0])"])
            .stdin(std::process::Stdio::null())
            .output()
            .is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "3")
    })
}
