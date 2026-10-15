//! The program icon in the Windows executable (Explorer, shortcuts, the taskbar before the
//! window opens). The running window sets the same picture itself (window_icon.rs).
fn main() {
    println!("cargo:rerun-if-changed=assets/nvforever.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/nvforever.ico");
        res.compile().expect("embedding the program icon (needs the Windows SDK's rc.exe)");
    }
}
