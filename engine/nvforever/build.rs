//! The program icon in the Windows executable (Explorer, shortcuts, the taskbar before the
//! window opens). The running window sets the same picture itself (window_icon.rs).
#[path = "src/frame_telemetry/git_paths.rs"]
mod git_paths;

fn git(base: &std::path::Path, arguments: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git").current_dir(base).args(arguments).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_owned())
}

/// Missing Git metadata leaves revision provenance unavailable; it never fails a build.
fn embed_revision() -> Option<()> {
    let base = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR")?);
    let revision = git(&base, &["rev-parse", "HEAD"])?;
    if !matches!(revision.len(), 40 | 64) || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let git_dir = git(&base, &["rev-parse", "--git-dir"])?;
    let common_dir = git(&base, &["rev-parse", "--git-common-dir"])?;
    let symbolic = git(&base, &["rev-parse", "--symbolic-full-name", "HEAD"])?;
    let paths = git_paths::resolve(&base, &git_dir, &common_dir, &symbolic)?;
    println!("cargo:rerun-if-changed={}", paths.head.display());
    // Watch only files that exist: Cargo treats a missing watched path as always changed,
    // which would rerun this script on every build (a branch kept only in packed-refs).
    // Packing removes a watched loose ref, which triggers a fresh lookup.
    if let Some(branch) = paths.branch.filter(|path| path.is_file()) {
        println!("cargo:rerun-if-changed={}", branch.display());
    }
    for packed in paths.packed_refs.into_iter().filter(|path| path.is_file()) {
        println!("cargo:rerun-if-changed={}", packed.display());
    }
    println!("cargo:rustc-env=NVF_BUILD_GIT_SHA={revision}");
    Some(())
}

fn main() {
    let _ = embed_revision();
    println!("cargo:rerun-if-changed=assets/nvforever.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/nvforever.ico");
        res.compile().expect("embedding the program icon (needs the Windows SDK's rc.exe)");
    }
}
