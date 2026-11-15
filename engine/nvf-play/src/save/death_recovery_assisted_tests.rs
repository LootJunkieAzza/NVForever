// Included inside the existing death-recovery test module; its ten controls are
// unchanged. Environment-sensitive startup is tested only in owned children.

fn child_environment() -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    std::env::vars_os().filter(|(key, _)| !key.to_str().is_some_and(|key|
        key == "NVF_PLAY_AIDS_FROM_SCRIPTS" || crate::drives_run(key))).collect()
}

fn bounded_child(mut child: std::process::Child) -> std::process::Output {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if child.try_wait().unwrap().is_some() { return child.wait_with_output().unwrap(); }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!("owned startup-policy child timed out: {output:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn assisted_default_and_owned_opt_in_reach_actual_recovery_in_fresh_processes() {
    for mode in ["ordinary", "measuring", "clock-one", "clock-zero", "driving", "owned", "foreign", "wrong-dir", "bad-opt-in", "missing-home"] {
        let root = Scratch::new();
        let home = root.0.join("death-recovery-home");
        std::fs::create_dir_all(home.join("saves")).unwrap();
        std::fs::write(root.0.join("owned-policy-test"), b"authored death recovery fixture").unwrap();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args(["--ignored", "--exact", "save::death_recovery::tests::assisted_policy_child", "--nocapture"])
            .current_dir(&root.0).env_clear().envs(child_environment())
            .env("NVFOREVER_HOME", &home).env("NVF_DEATH_POLICY_MODE", mode)
            .env("NVF_DEATH_POLICY_ROOT", &root.0)
            .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        match mode {
            "ordinary" => {},
            "measuring" => { command.env("NVF_PLAY_TEST_SAMPLE", "1"); },
            "clock-zero" => { command.env("NVF_PLAY_AIDS_FROM_SCRIPTS", "0"); },
            "driving" => { command.env("NVF_PLAY_TEST_HIT", "8:player:500:WeapKnife"); },
            _ => { command.env("NVF_PLAY_AIDS_FROM_SCRIPTS", "1"); },
        }
        if matches!(mode, "owned" | "foreign" | "wrong-dir" | "bad-opt-in" | "missing-home") {
            command.env("NVF_PLAY_TEST_DEATH_RECOVERY", if mode == "bad-opt-in" { "yes" } else { "1" });
        }
        if mode == "foreign" {
            let foreign = root.0.join("other-home");
            std::fs::create_dir_all(foreign.join("saves")).unwrap();
            command.env("NVFOREVER_HOME", foreign);
        }
        if mode == "missing-home" { command.env_remove("NVFOREVER_HOME"); }
        let output = bounded_child(command.spawn().unwrap());
        assert!(output.status.success(), "{mode}: {output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains("OWNED_DEATH_POLICY_COMPLETE"), "{mode}: {output:?}");
    }
}

#[test]
#[ignore = "entered by the owned process-boundary control, never against a user profile"]
fn assisted_policy_child() {
    let root = PathBuf::from(std::env::var_os("NVF_DEATH_POLICY_ROOT").expect("owned parent only")).canonicalize().unwrap();
    assert_eq!(std::env::current_dir().unwrap().canonicalize().unwrap(), root);
    assert_eq!(std::fs::read(root.join("owned-policy-test")).unwrap(), b"authored death recovery fixture");
    let mode = std::env::var("NVF_DEATH_POLICY_MODE").unwrap();
    // Never call the real default home resolver when the negative intentionally
    // omits HOME: even a test must not touch the user's ordinary save directory.
    let owned = root.join("death-recovery-home/saves");
    let config = if mode == "missing-home" || mode == "wrong-dir" {
        SaveConfig::with_dir(Some(owned.clone()))
    } else { SaveConfig::default() };
    let dir = config.dir.clone().unwrap();
    assert!(dir.canonicalize().unwrap().starts_with(&root));
    if mode != "foreign" { assert_eq!(dir.canonicalize().unwrap(), owned.canonicalize().unwrap()); }
    let mut app = app(Some(dir.clone()), Some(5.0));
    app.insert_resource(config);
    if mode == "wrong-dir" {
        let other = root.join("wrong-saves");
        std::fs::create_dir(&other).unwrap();
        app.world_mut().resource_mut::<SaveConfig>().dir = Some(other);
    }
    app.init_resource::<RecoveryPolicy>(); // actual SavePlugin startup resource
    let should_recover = matches!(mode.as_str(), "ordinary" | "measuring" | "owned");
    assert_eq!(app.world().resource::<RecoveryPolicy>().enabled, should_recover, "{mode}");
    let original = body(&app);
    let selected = save(&app, &dir, "authored-living", 10, SaveKind::Auto, &original);
    let before = std::fs::read(&selected).unwrap();
    // Real format/plugin/body admission, not a fake JSON or a filename sentinel.
    let plugins = crate::save::plugin_list(&app.world().resource::<PlayWorld>().0);
    let (_, checked, _) = crate::save::read_for_load(&selected, &plugins).unwrap();
    assert_eq!(serde_json::to_value(checked).unwrap(), serde_json::to_value(&original).unwrap());
    kill(&mut app);
    tick(&mut app, 6.0);
    if should_recover {
        assert_eq!(requests(&app).as_slice(), std::slice::from_ref(&selected), "{mode}");
        assert!(app.world().resource::<Paused>().0);
        assert!(matches!(app.world().resource::<PageRequest>().0, Some(Page::Load { .. })));
        app.world_mut().resource_mut::<Messages<LoadRequest>>().clear();
        app.world_mut().resource_mut::<Paused>().0 = false;
        tick(&mut app, 60.0);
        assert!(requests(&app).is_empty(), "consumed request must not be dispatched again");
        assert!(app.world().resource::<DeathRecovery>().handled);
    } else {
        assert!(requests(&app).is_empty(), "assisted run must not load {selected:?}");
        assert!(!app.world().resource::<Paused>().0);
        assert!(app.world().resource::<PageRequest>().0.is_none());
        assert!(!app.world().resource::<DeathRecovery>().handled);
        assert!(!app.world().resource::<DeathRecovery>().dead);
        tick(&mut app, 60.0);
        assert!(requests(&app).is_empty());
    }
    assert_eq!(std::fs::read(&selected).unwrap(), before, "never overwrite the original save");
    println!("OWNED_DEATH_POLICY_COMPLETE {mode}");
}

#[test]
fn owned_opt_in_rejects_missing_or_escaping_directories_before_recovery() {
    use std::ffi::OsString;
    let root = Scratch::new();
    let home = root.0.join("death-recovery-home");
    let saves = home.join("saves");
    std::fs::create_dir_all(&saves).unwrap();
    let environment = vec![(OsString::from("NVF_PLAY_AIDS_FROM_SCRIPTS"), OsString::from("1")),
        (OsString::from("NVF_PLAY_TEST_DEATH_RECOVERY"), OsString::from("1")),
        (OsString::from("NVFOREVER_HOME"), home.into_os_string())];
    assert_eq!(RecoveryPolicy::for_run(&environment, Some(&root.0), Some(&saves)), Ok(true));
    assert!(RecoveryPolicy::for_run(&environment, None, Some(&saves)).is_err());
    assert!(RecoveryPolicy::for_run(&environment, Some(&root.0), None).is_err());
    assert!(RecoveryPolicy::for_run(&environment, Some(&root.0.join("absent")), Some(&saves)).is_err());
    assert!(RecoveryPolicy::for_run(&environment, Some(&root.0), Some(&root.0)).is_err());
}
