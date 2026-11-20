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
    println!("cargo:rerun-if-changed=assets/nvforever.ico");
    println!("cargo:rerun-if-changed=src/ios_input.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        build_ios_input();
    }
    // Official release builds may send player reports to the project's server (reports/mod.rs).
    println!("cargo:rerun-if-env-changed=NVF_OFFICIAL_BUILD");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/nvforever.ico");
        // Common Controls 6, so the system's dialogs before the window (rfd's Task Dialog: the
        // reports popup's own button words) are the current ones; nothing else changes.
        res.set_manifest(MANIFEST);
        res.compile().expect("embedding the program icon (needs the Windows SDK's rc.exe)");
    }
}

fn build_ios_input() {
    use std::process::Command;
    let simulator = std::env::var("TARGET").unwrap().ends_with("-sim");
    let sdk = if simulator { "iphonesimulator" } else { "iphoneos" };
    let target = if simulator { "arm64-apple-ios18.0-simulator" } else { "arm64-apple-ios18.0" };
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let sdk_path = Command::new("xcrun").args(["--sdk", sdk, "--show-sdk-path"]).output().expect("finding the iOS SDK");
    assert!(sdk_path.status.success(), "finding the iOS SDK");
    let sdk_path = String::from_utf8(sdk_path.stdout).unwrap();
    let object = out.join("ios_input.o");
    assert!(Command::new("xcrun")
        .args([
            "--sdk",
            sdk,
            "clang",
            "-target",
            target,
            "-isysroot",
            sdk_path.trim(),
            "-fobjc-arc",
            "-fblocks",
            "-Wall",
            "-Werror",
            "-c",
            "src/ios_input.m",
            "-o"
        ])
        .arg(&object)
        .status()
        .expect("compiling iOS physical input")
        .success());
    assert!(Command::new("xcrun")
        .args(["libtool", "-static", "-o"])
        .arg(out.join("libnvf_ios_input.a"))
        .arg(object)
        .status()
        .expect("archiving iOS physical input")
        .success());
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=nvf_ios_input");
    for framework in ["GameController", "UIKit", "Foundation"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}
