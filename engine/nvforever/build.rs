//! The program icon in the Windows executable (Explorer, shortcuts, the taskbar before the
//! window opens). The running window sets the same picture itself (window_icon.rs).
//! Nothing here depends on the checkout's commit, so a commit does not rerun it
//! (frame telemetry reads the commit at run time: frame_telemetry/git_paths.rs, #1312).
const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
"#;

fn main() {
    compile_touch_icons();
    println!("cargo:rerun-if-changed=assets/nvforever.ico");
    // Official release builds may send player reports to the project's server (reports/mod.rs).
    println!("cargo:rerun-if-env-changed=NVF_OFFICIAL_BUILD");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/nvforever.ico");
        // Common Controls 6, so the system's dialogs before the window (rfd's Task Dialog: the
        // reports popup's own button words) are the current ones; nothing else changes.
        res.set_manifest(MANIFEST);
        res.compile()
            .expect("embedding the program icon (needs the Windows SDK's rc.exe)");
    }
}

// Python 3.11+ is already required by project tooling. NVF_PYTHON is supplied by
// tools/android/build.py; direct Cargo callers can select the same interpreter.
fn compile_touch_icons() {
    println!("cargo:rerun-if-changed=../../tools/android/svg_icons.py");
    println!("cargo:rerun-if-changed=assets/touch");
    println!("cargo:rerun-if-env-changed=NVF_PYTHON");
    let python = std::env::var_os("NVF_PYTHON").unwrap_or_else(|| {
        if cfg!(windows) {
            "python".into()
        } else {
            "python3".into()
        }
    });
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"))
        .join("touch_icons.rs");
    let status = std::process::Command::new(python)
        .arg("../../tools/android/svg_icons.py")
        .arg("--manifest")
        .arg("assets/touch/manifest.json")
        .arg("--out")
        .arg(output)
        .status()
        .expect("owned SVG compiler needs Python 3.11+; set NVF_PYTHON to its executable");
    assert!(
        status.success(),
        "owned SVG validation failed; no unsupported SVG is rendered"
    );
}
