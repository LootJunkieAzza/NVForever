//! Join directories that differ only by case inside NVForever's own folders.
//!
//! Runs from `index_loose` and returns immediately unless the directory is
//! under the NVForever home, so the game Data folder is never written.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub fn fold_mod_tree(root: &Path) {
    if std::env::var_os("NVFOREVER_NO_CASEFOLD").is_some() {
        return;
    }
    if !in_nvforever_home(root) {
        return;
    }
    fold_dir(root);
}

fn in_nvforever_home(root: &Path) -> bool {
    let Ok(canon) = root.canonicalize() else { return false };
    for home in home_dirs() {
        let Ok(home) = home.canonicalize() else { continue };
        if canon.starts_with(&home) {
            return true;
        }
    }
    false
}

fn home_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(h) = std::env::var_os("NVFOREVER_HOME") {
        dirs.push(PathBuf::from(h));
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(xdg).join("nvforever"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/nvforever"));
    }
    dirs
}

fn preferred(folded: &str) -> Option<&'static str> {
    Some(match folded {
        "meshes" => "meshes",
        "textures" => "textures",
        "sound" => "sound",
        "music" => "music",
        "menus" => "menus",
        "shaders" => "shaders",
        "video" => "video",
        "nvse" => "NVSE",
        "config" => "Config",
        "uio" => "uio",
        "facegen" => "facegen",
        "distantlod" => "distantlod",
        "lsdata" => "LSData",
        "plugins" => "Plugins",
        "scripts" => "Scripts",
        _ => return None,
    })
}

fn fold_dir(dir: &Path) {
    if dir.is_symlink() || !dir.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && !path.is_symlink() {
            fold_dir(&path);
        }
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut groups: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        groups.entry(name.to_ascii_lowercase()).or_default().push(path);
    }
    for (folded, group) in groups {
        if group.len() < 2 {
            continue;
        }
        let want = preferred(&folded);
        let survivor = group
            .iter()
            .find(|p| want.is_some_and(|w| p.file_name().and_then(|n| n.to_str()) == Some(w)) && p.is_dir() && !p.is_symlink())
            .cloned()
            .or_else(|| group.iter().find(|p| p.is_dir() && !p.is_symlink()).cloned())
            .unwrap_or_else(|| group[0].clone());
        for other in &group {
            if other != &survivor {
                let _ = absorb(other, &survivor);
            }
        }
        if let Some(want) = want {
            if survivor.file_name().and_then(|n| n.to_str()) != Some(want) {
                let dest = dir.join(want);
                if !dest.exists() && !dest.is_symlink() {
                    let _ = fs::rename(&survivor, dest);
                }
            }
        }
    }
}

fn absorb(src: &Path, dst: &Path) -> io::Result<()> {
    let src_dir = src.is_dir() && !src.is_symlink();
    let dst_dir = dst.is_dir() && !dst.is_symlink();
    if src_dir && dst_dir {
        for entry in fs::read_dir(src)?.flatten() {
            let child = entry.path();
            let Some(name) = child.file_name() else { continue };
            let target = dst.join(name);
            if !target.exists() && !target.is_symlink() {
                fs::rename(&child, target)?;
            } else {
                absorb(&child, &target)?;
            }
        }
        fs::remove_dir(src)?;
        return Ok(());
    }
    if same_entry(src, dst) {
        if src_dir {
            fs::remove_dir_all(src)?;
        } else {
            fs::remove_file(src)?;
        }
        return Ok(());
    }
    let mut n = 2u32;
    let mut kept = dst.with_file_name(format!(
        "{}.case-conflict",
        dst.file_name().and_then(|s| s.to_str()).unwrap_or("file")
    ));
    while kept.exists() || kept.is_symlink() {
        kept = dst.with_file_name(format!(
            "{}.case-conflict-{n}",
            dst.file_name().and_then(|s| s.to_str()).unwrap_or("file")
        ));
        n += 1;
    }
    fs::rename(src, kept)
}

fn same_entry(a: &Path, b: &Path) -> bool {
    if a.is_symlink() || b.is_symlink() {
        return a.is_symlink() && b.is_symlink() && fs::read_link(a).ok() == fs::read_link(b).ok();
    }
    if !(a.is_file() && b.is_file()) {
        return false;
    }
    let Ok(ma) = a.metadata() else { return false };
    let Ok(mb) = b.metadata() else { return false };
    if ma.len() != mb.len() {
        return false;
    }
    let Ok(mut fa) = File::open(a) else { return false };
    let Ok(mut fb) = File::open(b) else { return false };
    let mut ba = [0u8; 8192];
    let mut bb = [0u8; 8192];
    loop {
        let Ok(na) = fa.read(&mut ba) else { return false };
        let Ok(nb) = fb.read(&mut bb) else { return false };
        if na != nb || ba[..na] != bb[..nb] {
            return false;
        }
        if na == 0 {
            return true;
        }
    }
}
