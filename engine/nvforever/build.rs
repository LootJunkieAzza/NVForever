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
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <heapType xmlns="http://schemas.microsoft.com/SMI/2020/WindowsSettings">SegmentHeap</heapType>
    </windowsSettings>
  </application>
</assembly>
"#;

fn main() {
    println!("cargo:rerun-if-changed=assets/nvforever.ico");
    // Official release builds may send player reports to the project's server (reports/mod.rs).
    println!("cargo:rerun-if-env-changed=NVF_OFFICIAL_BUILD");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/nvforever.ico");
        // Common Controls 6, so the system's dialogs before the window (rfd's Task Dialog: the
        // reports popup's own button words) are the current ones. `heapType` SegmentHeap: the
        // default heap gave memory back to the system in one go under its lock, a 17-21 ms
        // stall of every allocating thread at some cell crossings
        // (docs/design/streaming-hitches.md section 3.6). Nothing else changes.
        res.set_manifest(MANIFEST);
        res.compile().expect("embedding the program icon (needs the Windows SDK's rc.exe)");
    }
    // Co-op's Steam (private fork; nvf-coop/src/steam.rs): steam_api64.dll is loaded when
    // co-op first asks for Steam, not at start, so a game without the file still runs.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bin=nvforever=/DELAYLOAD:steam_api64.dll");
        println!("cargo:rustc-link-arg-bin=nvforever=delayimp.lib");
    }
}
