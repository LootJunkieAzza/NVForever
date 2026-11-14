//! The staging folder and the install: files unpacked into `<target's parent>\.dlss-download-
//! <process id>\` (beside the DLSS folder, so on its volume and a rename away), checked there
//! again, and moved into the DLSS folder only when every check passed.
//!
//! [design] A target that does not exist is made by renaming the staging folder, one step. A
//! target that exists is refused when it holds `nvngx_update.exe` (in any case: the loader
//! refuses such a folder, `preflight.rs`) or a listed file with other bytes (the player's own
//! file, never overwritten); otherwise each listed file it lacks is moved in, and the ones
//! moved are removed again if a later move fails. Every move refuses to replace what is at its
//! destination ([`move_new`]), so a file that appears there meanwhile is never overwritten,
//! nor removed by the rollback. The staging folder is removed when its guard drops: on a
//! failure, on a cancel and after the install. Staging folders an earlier run left (a crash, a
//! power cut) are removed at the start of a played game and when a new one is made, unless the
//! run that made them is still going ([`remove_stale`]).

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use super::super::preflight::UPDATER_NAME;
use super::decode::pin_matches;
use super::{DownloadFile, Failure};

/// The start of a staging folder's name.
pub(super) const STAGING_PREFIX: &str = ".dlss-download-";

fn write_error(what: &Path, e: &io::Error) -> Failure {
    Failure::Write(format!("{}: {e}", what.display()))
}

/// [design] Uses the game install remembered at startup, as the other engine writers do.
fn guard_write(path: &Path) -> Result<(), Failure> {
    fnv_install::guard_write(path, None).map_err(|e| Failure::Write(e.to_string()))
}

/// The staging folder, removed when dropped unless it became the DLSS folder.
pub(super) struct Staging {
    dir: PathBuf,
    moved: bool,
}

impl Staging {
    /// A new, empty staging folder for `target`, older ones beside it removed.
    pub(super) fn create(target: &Path) -> Result<Self, Failure> {
        let parent = target.parent().filter(|p| !p.as_os_str().is_empty()).ok_or_else(|| Failure::Write(format!("{} has no parent folder", target.display())))?;
        let own = std::process::id();
        let dir = parent.join(format!("{STAGING_PREFIX}{own}"));
        // [design] Refuse both destinations before creating or cleaning up anything.
        guard_write(target)?;
        guard_write(parent)?;
        guard_write(&dir)?;
        fs::create_dir_all(parent).map_err(|e| write_error(parent, &e))?;
        remove_stale(parent, own, &crate::reports::upload::still_running);
        // This process's own leftover (one download runs at a time: `start`) is removed here.
        if fs::symlink_metadata(&dir).is_ok() {
            fs::remove_dir_all(&dir).map_err(|e| write_error(&dir, &e))?;
        }
        fs::create_dir(&dir).map_err(|e| write_error(&dir, &e))?;
        Ok(Self { dir, moved: false })
    }

    pub(super) fn path(&self) -> &Path {
        &self.dir
    }

    /// A new file of the staging folder, named from the list.
    pub(super) fn create_file(&self, file: &DownloadFile) -> Result<File, Failure> {
        let path = self.dir.join(file.name);
        guard_write(&path)?;
        File::options().write(true).create_new(true).open(&path).map_err(|e| write_error(&path, &e))
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if !self.moved && guard_write(&self.dir).is_ok() {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

/// The staging folders other runs left beside `target` removed ([`remove_stale`]), without
/// making anything: the start's clean-up (`super::clean_up_at_start`).
pub(super) fn remove_stale_beside(target: &Path) {
    if let Err(why) = guard_write(target) {
        bevy::log::warn!("DLSS download: staging clean-up refused: {why}");
        return;
    }
    if let Some(parent) = target.parent().filter(|p| !p.as_os_str().is_empty()) {
        remove_stale(parent, std::process::id(), &crate::reports::upload::still_running);
    }
}

/// The process id in a staging folder's name, `.dlss-download-<digits>`; `None` for any other
/// name.
fn staging_pid(name: &str) -> Option<u32> {
    let digits = name.strip_prefix(STAGING_PREFIX)?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The staging folders in `parent` that other runs left, removed: real folders named
/// `.dlss-download-<process id>` whose process is not `own` and not `running` (an NVForever
/// run still going, `reports::upload::still_running`: its download may be under way). Open
/// files do not protect a folder in use (Rust opens files sharing deletion on Windows), so
/// the process is asked instead. Anything else in `parent` is left alone.
fn remove_stale(parent: &Path, own: u32, running: &dyn Fn(u32) -> bool) {
    let Ok(entries) = fs::read_dir(parent) else { return };
    for entry in entries.flatten() {
        let Some(pid) = staging_pid(&entry.file_name().to_string_lossy()) else { continue };
        let real_dir = fs::symlink_metadata(entry.path()).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink());
        if real_dir && pid != own && !running(pid) && guard_write(&entry.path()).is_ok() {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// The SHA-256 of the file at `path`, read in pieces.
fn file_sha256(path: &Path) -> io::Result<[u8; 32]> {
    let mut file = File::open(path)?;
    let mut hash = fnv_util::sha256::Stream::new();
    let mut buf = vec![0u8; super::decode::CHUNK];
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => hash.update(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(hash.finish())
}

/// The staged files checked once more before they are moved: each one's SHA-256 read back
/// from the disk, then `checks` (on Windows the signature checks, [`signature_checks`]).
pub(super) fn verify_staged(staging: &Staging, files: &[DownloadFile], checks: &dyn Fn(&Path) -> Result<(), Failure>) -> Result<(), Failure> {
    for file in files {
        let path = staging.dir.join(file.name);
        let digest = file_sha256(&path).map_err(|e| write_error(&path, &e))?;
        if !pin_matches(&digest, file) {
            return Err(Failure::Mismatch { name: file.name, sha256: fnv_util::sha256::to_hex(&digest) });
        }
    }
    checks(&staging.dir)
}

/// Whether two names are one on Windows: compared in full Unicode upper case, as the loader
/// compares them (`preflight.rs`).
fn same_name(a: &str, b: &str) -> bool {
    a.to_uppercase() == b.to_uppercase()
}

/// The staged files moved into `target` (the module notes), `mv` doing each rename. Nothing
/// of `target` is changed on an error.
pub(super) fn place(staging: &mut Staging, files: &[DownloadFile], target: &Path, mv: &mut dyn FnMut(&Path, &Path) -> io::Result<()>) -> Result<(), Failure> {
    guard_write(&staging.dir)?;
    guard_write(target)?;
    for file in files {
        guard_write(&target.join(file.name))?;
    }
    let meta = match fs::symlink_metadata(target) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            mv(&staging.dir, target).map_err(|e| {
                if e.kind() == io::ErrorKind::AlreadyExists {
                    Failure::Write(format!("{} appeared while the files were checked", target.display()))
                } else {
                    write_error(target, &e)
                }
            })?;
            staging.moved = true;
            return Ok(());
        }
        Err(e) => return Err(write_error(target, &e)),
        Ok(meta) => meta,
    };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(Failure::Write(format!("{} is not a folder", target.display())));
    }
    let names: Vec<String> = fs::read_dir(target)
        .and_then(|entries| entries.map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned())).collect())
        .map_err(|e| write_error(target, &e))?;
    if let Some(updater) = names.iter().find(|n| same_name(n, UPDATER_NAME)) {
        return Err(Failure::FolderHolds(updater.clone()));
    }
    let mut missing = Vec::new();
    for file in files {
        match names.iter().find(|n| same_name(n, file.name)) {
            Some(existing) => {
                let path = target.join(existing);
                let digest = file_sha256(&path).map_err(|e| write_error(&path, &e))?;
                if !pin_matches(&digest, file) {
                    return Err(Failure::FolderHolds(existing.clone()));
                }
            }
            None => missing.push(file),
        }
    }
    let mut moved: Vec<PathBuf> = Vec::new();
    let rollback = |moved: &[PathBuf]| {
        for path in moved.iter().rev() {
            if guard_write(path).is_ok() {
                let _ = fs::remove_file(path);
            }
        }
    };
    for file in missing {
        let dest = target.join(file.name);
        if fs::symlink_metadata(&dest).is_ok() {
            rollback(&moved);
            return Err(Failure::FolderHolds(file.name.to_string()));
        }
        if let Err(e) = mv(&staging.dir.join(file.name), &dest) {
            rollback(&moved);
            // A file that appeared since the listing is the player's: left as it is.
            if e.kind() == io::ErrorKind::AlreadyExists {
                return Err(Failure::FolderHolds(file.name.to_string()));
            }
            return Err(write_error(&dest, &e));
        }
        moved.push(dest);
    }
    Ok(())
}

/// `from` (a file or a folder) moved to `to` on the same volume, never replacing anything at
/// `to` (an error of kind `AlreadyExists` then), tried again a few times when the file is held
/// for a moment (a virus scanner reading a new DLL) [design].
pub(super) fn move_new(from: &Path, to: &Path) -> io::Result<()> {
    let mut tries = 0;
    loop {
        match move_no_replace(from, to) {
            Err(e) if matches!(e.kind(), io::ErrorKind::PermissionDenied | io::ErrorKind::ResourceBusy) && tries < 5 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            other => return other,
        }
    }
}

/// `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING` (nor `MOVEFILE_COPY_ALLOWED`): it fails
/// when `to` exists [documented: Microsoft Learn, "MoveFileExW function (winbase.h)", dwFlags].
/// `fs::rename` would replace an existing file (it passes the replace flag).
#[cfg(windows)]
fn move_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;
    let wide = |p: &Path| p.as_os_str().encode_wide().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (from_w, to_w) = (wide(from), wide(to));
    // SAFETY: both strings are zero-terminated and outlive the call.
    if unsafe { MoveFileExW(from_w.as_ptr(), to_w.as_ptr(), 0) } != 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

/// Off Windows (the tests only: the download never runs there): a file by a hard link, which
/// fails when `to` exists, and the staged name removed; a folder by a rename after an existence
/// check (`rename` would replace an empty folder).
#[cfg(not(windows))]
fn move_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    if fs::symlink_metadata(from)?.is_dir() {
        if fs::symlink_metadata(to).is_ok() {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        fs::rename(from, to)
    } else {
        fs::hard_link(from, to)?;
        fs::remove_file(from)
    }
}

/// The signature checks on the staged files: the interposer's (`security::check_interposer`:
/// its signature and its tested hash) and each `nvngx_*.dll`'s NVIDIA signature
/// (`security::check_ngx`). Every handle is closed before returning, so the files can be moved.
#[cfg(windows)]
pub(super) fn signature_checks(dir: &Path, files: &[DownloadFile]) -> Result<(), Failure> {
    use super::super::{preflight, security};
    for file in files {
        let path = dir.join(file.name);
        if file.name.eq_ignore_ascii_case("sl.interposer.dll") {
            let accepted = security::check_interposer(&path).map_err(|refused| Failure::Signature { name: file.name, why: refused.why })?;
            drop(accepted);
        } else if file.name.to_ascii_lowercase().starts_with("nvngx_") {
            let handle = preflight::open_shared_read(&path).map_err(|e| write_error(&path, &e))?;
            security::check_ngx(&path, &handle).map_err(|why| Failure::Signature { name: file.name, why })?;
        }
    }
    Ok(())
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// A new empty folder for one test.
    pub(in super::super) fn scratch(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("nvf-dlss-download-{label}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn hex(data: &[u8]) -> &'static str {
        Box::leak(fnv_util::sha256::hex(data).into_boxed_str())
    }

    fn listed(names_and_data: &[(&'static str, &[u8])]) -> Vec<DownloadFile> {
        names_and_data.iter().map(|(name, data)| DownloadFile { name, zip_path: name, size: data.len() as u64, sha256: hex(data) }).collect()
    }

    const A: &[u8] = b"first file";
    const B: &[u8] = b"second file";
    const C: &[u8] = b"third file";

    /// A staging folder for `target` holding the three files.
    fn staged(target: &Path, files: &[DownloadFile], data: &[&[u8]]) -> Staging {
        let staging = Staging::create(target).unwrap();
        for (file, bytes) in files.iter().zip(data) {
            std::io::Write::write_all(&mut staging.create_file(file).unwrap(), bytes).unwrap();
        }
        staging
    }

    fn listing(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn guarded_destinations_are_refused_before_any_write() {
        let root = scratch("guard");
        // [design] An artificial remembered install also covers a forbidden staging folder
        // beside an otherwise allowed target. No real game install or environment changes.
        let guarded = root.join(format!("{STAGING_PREFIX}{}", std::process::id()));
        fnv_install::remember_game(&fnv_install::GameInstall { root: guarded.clone(), data: guarded.join("Data") });
        let target = guarded.join("missing-parent/dlss");
        assert!(matches!(Staging::create(&target), Err(Failure::Write(_))));
        assert!(listing(&root).is_empty(), "neither the parent nor staging was created");
        assert!(matches!(Staging::create(&root.join("dlss")), Err(Failure::Write(_))));
        assert!(listing(&root).is_empty(), "a forbidden staging folder prevents all writes");
        assert!(crate::dlss_download::create_dlss_folder(&target).is_err());
        assert!(listing(&root).is_empty(), "Open dlss folder creates nothing");

        let allowed = scratch("guard-source");
        let files = listed(&[("a.dll", A)]);
        let mut staging = staged(&allowed.join("dlss"), &files, &[A]);
        let mut moves = 0;
        let result = place(&mut staging, &files, &target, &mut |_, _| {
            moves += 1;
            Ok(())
        });
        assert!(matches!(result, Err(Failure::Write(_))));
        assert_eq!(moves, 0, "guard refusal precedes every move");
        assert_eq!(fs::read(staging.path().join("a.dll")).unwrap(), A);
        assert!(listing(&root).is_empty());
        drop(staging);
        let _ = fs::remove_dir_all(allowed);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_absent_folder_is_made_by_renaming_the_staging_folder() {
        let root = scratch("absent");
        let target = root.join("dlss");
        let files = listed(&[("a.dll", A), ("b.dll", B), ("c.dll", C)]);
        let mut staging = staged(&target, &files, &[A, B, C]);
        let staging_path = staging.path().to_path_buf();
        assert!(staging_path.file_name().unwrap().to_string_lossy().starts_with(STAGING_PREFIX));
        assert_eq!(staging_path.parent(), Some(root.as_path()), "beside the target");
        verify_staged(&staging, &files, &|_| Ok(())).unwrap();
        place(&mut staging, &files, &target, &mut move_new).unwrap();
        drop(staging);
        assert_eq!(listing(&target), ["a.dll", "b.dll", "c.dll"]);
        assert_eq!(fs::read(target.join("b.dll")).unwrap(), B);
        assert_eq!(listing(&root), ["dlss"], "no staging folder left");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_present_folder_gets_only_the_files_it_lacks() {
        let root = scratch("present");
        let target = root.join("dlss");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("A.DLL"), A).unwrap(); // the same bytes, spelled otherwise
        fs::write(target.join("readme.txt"), b"the player's").unwrap();
        let files = listed(&[("a.dll", A), ("b.dll", B), ("c.dll", C)]);
        let mut staging = staged(&target, &files, &[A, B, C]);
        let mut moves = Vec::new();
        place(&mut staging, &files, &target, &mut |a, b| {
            moves.push(b.file_name().unwrap().to_string_lossy().into_owned());
            move_new(a, b)
        })
        .unwrap();
        drop(staging);
        assert_eq!(moves, ["b.dll", "c.dll"]);
        assert_eq!(listing(&target), ["A.DLL", "b.dll", "c.dll", "readme.txt"]);
        assert_eq!(listing(&root), ["dlss"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_folder_with_other_bytes_or_the_updater_is_left_as_it_is() {
        let files = listed(&[("a.dll", A), ("b.dll", B)]);
        for (label, name, bytes) in [("conflict", "b.dll", &b"the player's own build"[..]), ("updater", "NVNGX_Update.EXE", &b"exe"[..])] {
            let root = scratch(label);
            let target = root.join("dlss");
            fs::create_dir(&target).unwrap();
            fs::write(target.join(name), bytes).unwrap();
            let mut staging = staged(&target, &files, &[A, B]);
            let result = place(&mut staging, &files, &target, &mut move_new);
            assert_eq!(result, Err(Failure::FolderHolds(name.to_string())), "{label}");
            drop(staging);
            assert_eq!(listing(&target), [name], "{label}: nothing written");
            assert_eq!(fs::read(target.join(name)).unwrap(), bytes);
            assert_eq!(listing(&root), ["dlss"], "{label}: staging removed");
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn a_failed_move_takes_back_the_files_moved_before_it() {
        let root = scratch("rollback");
        let target = root.join("dlss");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep.txt"), b"x").unwrap();
        let files = listed(&[("a.dll", A), ("b.dll", B), ("c.dll", C)]);
        let mut staging = staged(&target, &files, &[A, B, C]);
        let mut n = 0;
        let result = place(&mut staging, &files, &target, &mut |a, b| {
            n += 1;
            if n == 3 { Err(io::Error::other("disk full")) } else { move_new(a, b) }
        });
        assert!(matches!(result, Err(Failure::Write(why)) if why.contains("disk full")));
        drop(staging);
        assert_eq!(listing(&target), ["keep.txt"], "the two files moved are taken back");
        assert_eq!(listing(&root), ["dlss"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn staged_files_are_hashed_again_and_checked_before_any_move() {
        let root = scratch("verify");
        let target = root.join("dlss");
        let files = listed(&[("a.dll", A), ("b.dll", B)]);
        let staging = staged(&target, &files, &[A, b"changed on the disk"]);
        assert_eq!(verify_staged(&staging, &files, &|_| Ok(())), Err(Failure::Mismatch { name: "b.dll", sha256: fnv_util::sha256::hex(b"changed on the disk") }));
        drop(staging);
        let staging = staged(&target, &files, &[A, B]);
        let refused = verify_staged(&staging, &files, &|dir| {
            assert!(dir.join("a.dll").is_file());
            Err(Failure::Signature { name: "a.dll", why: "test".into() })
        });
        assert_eq!(refused, Err(Failure::Signature { name: "a.dll", why: "test".into() }));
        drop(staging);
        assert!(!target.exists());
        assert!(listing(&root).is_empty(), "staging removed when dropped");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn staging_folders_of_ended_runs_are_removed_and_nothing_else() {
        let root = scratch("stale");
        let folder = |name: &str| {
            let dir = root.join(name);
            fs::create_dir(&dir).unwrap();
            fs::write(dir.join("sl.common.dll"), b"half").unwrap();
            dir
        };
        let ended = folder(&format!("{STAGING_PREFIX}4294967294"));
        let running = folder(&format!("{STAGING_PREFIX}77"));
        let own = folder(&format!("{STAGING_PREFIX}12"));
        let other_name = folder(&format!("{STAGING_PREFIX}backup"));
        let plain = folder(".dlss-download");
        fs::write(root.join(format!("{STAGING_PREFIX}5")), b"not a folder: kept").unwrap();
        remove_stale(&root, 12, &|pid| pid == 77);
        assert!(!ended.exists(), "an ended run's folder is removed");
        assert!(running.exists(), "a running NVForever's folder is kept");
        assert!(own.exists(), "this process's own is left to Staging::create");
        assert!(other_name.exists() && plain.exists(), "other names are not staging folders");
        assert!(root.join(format!("{STAGING_PREFIX}5")).is_file());
        assert_eq!(staging_pid(".dlss-download-123"), Some(123));
        assert_eq!(staging_pid(".dlss-download-"), None);
        assert_eq!(staging_pid(".dlss-download-1a"), None);
        assert_eq!(staging_pid(".dlss-download-+1"), None);
        // Staging::create: an ended run's folder and this process's own leftover go.
        let target = root.join("dlss");
        let leftover = folder(&format!("{STAGING_PREFIX}{}", std::process::id()));
        let ended = folder(&format!("{STAGING_PREFIX}4294967294"));
        let staging = Staging::create(&target).unwrap();
        assert!(!ended.exists());
        assert_eq!(staging.path(), leftover.as_path());
        assert!(listing(staging.path()).is_empty(), "this process's leftover made new");
        drop(staging);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_move_never_replaces_what_is_at_its_destination() {
        let root = scratch("move");
        fs::write(root.join("staged.dll"), b"ours").unwrap();
        fs::write(root.join("there.dll"), b"the player's").unwrap();
        let refused = move_new(&root.join("staged.dll"), &root.join("there.dll")).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(root.join("there.dll")).unwrap(), b"the player's");
        assert_eq!(fs::read(root.join("staged.dll")).unwrap(), b"ours");
        move_new(&root.join("staged.dll"), &root.join("new.dll")).unwrap();
        assert_eq!(listing(&root), ["new.dll", "there.dll"]);
        fs::create_dir(root.join("staging")).unwrap();
        fs::write(root.join("staging").join("a.dll"), b"a").unwrap();
        fs::create_dir(root.join("dlss")).unwrap();
        assert_eq!(move_new(&root.join("staging"), &root.join("dlss")).unwrap_err().kind(), io::ErrorKind::AlreadyExists, "an empty folder is not replaced");
        move_new(&root.join("staging"), &root.join("dlss2")).unwrap();
        assert_eq!(listing(&root.join("dlss2")), ["a.dll"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_file_appearing_before_its_move_is_left_and_the_moves_taken_back() {
        let root = scratch("appears");
        let target = root.join("dlss");
        fs::create_dir(&target).unwrap();
        let files = listed(&[("a.dll", A), ("b.dll", B)]);
        let mut staging = staged(&target, &files, &[A, B]);
        let mut first = true;
        let result = place(&mut staging, &files, &target, &mut |from, to| {
            if !first {
                // The player's file lands just before the second move.
                fs::write(to, b"the player's").unwrap();
            }
            first = false;
            move_new(from, to)
        });
        assert_eq!(result, Err(Failure::FolderHolds("b.dll".into())));
        drop(staging);
        assert_eq!(listing(&target), ["b.dll"], "a.dll taken back, the player's b.dll kept");
        assert_eq!(fs::read(target.join("b.dll")).unwrap(), b"the player's");
        let _ = fs::remove_dir_all(root);
    }
}
