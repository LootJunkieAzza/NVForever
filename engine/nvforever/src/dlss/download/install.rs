//! The staging folder and the install: files unpacked into `<target's parent>\.dlss-download-
//! <process id>\` (beside the DLSS folder, so on its volume and a rename away), checked there
//! again, and moved into the DLSS folder only when every check passed.
//!
//! [design] A target that does not exist is made by renaming the staging folder, one step. A
//! target that exists is refused when it holds `nvngx_update.exe` (in any case: the loader
//! refuses such a folder, `preflight.rs`) or a listed file with other bytes (the player's own
//! file, never overwritten); otherwise each listed file it lacks is moved in, and the ones
//! moved are removed again if a later move fails. The staging folder is removed when its guard
//! drops: on a failure, on a cancel and after the install. Staging folders an earlier run left
//! (a crash, a power cut) are removed when a new one is made.

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

/// The staging folder, removed when dropped unless it became the DLSS folder.
pub(super) struct Staging {
    dir: PathBuf,
    moved: bool,
}

impl Staging {
    /// A new, empty staging folder for `target`, older ones beside it removed.
    pub(super) fn create(target: &Path) -> Result<Self, Failure> {
        let parent = target.parent().filter(|p| !p.as_os_str().is_empty()).ok_or_else(|| Failure::Write(format!("{} has no parent folder", target.display())))?;
        fs::create_dir_all(parent).map_err(|e| write_error(parent, &e))?;
        let name = format!("{STAGING_PREFIX}{}", std::process::id());
        remove_stale(parent);
        let dir = parent.join(name);
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
        File::options().write(true).create_new(true).open(&path).map_err(|e| write_error(&path, &e))
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if !self.moved {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

/// Every staging folder in `parent` (a real folder named [`STAGING_PREFIX`]...), including one
/// of this process's from an earlier download, removed; one still in use stays (its files are
/// open, so the removal fails and is ignored).
fn remove_stale(parent: &Path) {
    let Ok(entries) = fs::read_dir(parent) else { return };
    for entry in entries.flatten() {
        let stale = entry.file_name().to_string_lossy().starts_with(STAGING_PREFIX);
        let real_dir = fs::symlink_metadata(entry.path()).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink());
        if stale && real_dir {
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
    let meta = match fs::symlink_metadata(target) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            mv(&staging.dir, target).map_err(|e| write_error(target, &e))?;
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
            let _ = fs::remove_file(path);
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
            return Err(write_error(&dest, &e));
        }
        moved.push(dest);
    }
    Ok(())
}

/// A rename, tried again a few times when the file is held for a moment (a virus scanner
/// reading a new DLL) [design].
pub(super) fn rename(from: &Path, to: &Path) -> io::Result<()> {
    let mut tries = 0;
    loop {
        match fs::rename(from, to) {
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied && tries < 5 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            other => return other,
        }
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
    fn an_absent_folder_is_made_by_renaming_the_staging_folder() {
        let root = scratch("absent");
        let target = root.join("dlss");
        let files = listed(&[("a.dll", A), ("b.dll", B), ("c.dll", C)]);
        let mut staging = staged(&target, &files, &[A, B, C]);
        let staging_path = staging.path().to_path_buf();
        assert!(staging_path.file_name().unwrap().to_string_lossy().starts_with(STAGING_PREFIX));
        assert_eq!(staging_path.parent(), Some(root.as_path()), "beside the target");
        verify_staged(&staging, &files, &|_| Ok(())).unwrap();
        place(&mut staging, &files, &target, &mut |a, b| rename(a, b)).unwrap();
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
            rename(a, b)
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
            let result = place(&mut staging, &files, &target, &mut |a, b| rename(a, b));
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
            if n == 3 { Err(io::Error::other("disk full")) } else { rename(a, b) }
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
    fn staging_folders_left_by_earlier_runs_are_removed() {
        let root = scratch("stale");
        let old = root.join(format!("{STAGING_PREFIX}1"));
        fs::create_dir(&old).unwrap();
        fs::write(old.join("sl.common.dll"), b"half").unwrap();
        fs::write(root.join(format!("{STAGING_PREFIX}file")), b"not a folder: kept").unwrap();
        let target = root.join("dlss");
        let staging = Staging::create(&target).unwrap();
        assert!(!old.exists());
        assert!(root.join(format!("{STAGING_PREFIX}file")).is_file());
        assert!(staging.path().is_dir());
        drop(staging);
        let _ = fs::remove_dir_all(root);
    }
}
