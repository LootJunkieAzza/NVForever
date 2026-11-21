//! `fnvtool pbr ...`: authored PBR material maps under `textures\pbr\` (#2981,
//! docs/spec-notes/pbr-materials.md). `list` mounts the VFS as the engine does and names
//! every set it finds and, per member, the archive or loose folder that supplies it.
//! `check` validates a pack folder on disk and `selftest` runs the same checks on files
//! built in memory (`pbr_check`).

use crate::pbr_check;
use anyhow::Context;
use clap::Subcommand;
use fnv_pbr::{member_path, PbrIndex, PBR_DIR};
use fnv_vfs::Vfs;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum PbrCmd {
    /// The sets under `textures\pbr\` a load order supplies: per set its diffuse, state and
    /// members, the source that wins for each member, then totals and what was left out
    /// (fnv.pbr.listing/1).
    List {
        #[command(flatten)]
        mount: MountArgs,
    },
    /// Validate a pack folder on disk (a mod folder holding `textures\pbr\`, its
    /// `textures` folder or the `pbr` folder itself): DDS headers and sizes, formats per
    /// member, mip chains, scalar files, incomplete sets (fnv.pbr.check/1). Exits 1 when
    /// any error is found; warnings do not fail it.
    Check {
        /// The pack folder.
        folder: PathBuf,
    },
    /// Run the checks of `check` on a small pack built in memory (a clean set and one
    /// broken file per finding it must raise) and print fnv.pbr.check/1, then a last line
    /// `pbr selftest: PASS (...)`; exits 1 when a finding was missed or another raised.
    Selftest,
}

/// The VFS as the engine mounts it: `--data` with the plugins' archives, and either
/// NVForever's mods folder (`--mods`, the home `mods`) or a Mod Organizer 2 profile
/// (`--mo2`, `--profile`) over it.
#[derive(clap::Args)]
pub struct MountArgs {
    /// The game's Data folder.
    #[arg(long)]
    data: PathBuf,
    /// Plugins in load order (default: the base game and its add-on masters; with `--mo2`,
    /// the profile's active plugins).
    plugins: Vec<String>,
    /// NVForever's mods folder: its loose files over Data's and the archives beside its
    /// plugins, as the engine mounts its home `mods` folder.
    #[arg(long, conflicts_with = "mo2")]
    mods: Option<PathBuf>,
    /// A Mod Organizer 2 instance folder: the profile's enabled mods layered over Data, its
    /// archive list and plugins, as `nvforever --mo2` mounts them (docs/design/mod-folders.md).
    #[arg(long)]
    mo2: Option<PathBuf>,
    /// The Mod Organizer 2 profile (default: the instance's selected profile).
    #[arg(long, requires = "mo2")]
    profile: Option<String>,
    /// A base archive list in place of the shipped `SArchiveList` (comma-separated, lowest
    /// precedence first), as NVForever.ini's `[Archive] SArchiveList` sets it.
    #[arg(long)]
    archive_list: Option<String>,
}

/// A command's output: the JSON document, lines printed after it, and why the command
/// still fails (a pack with errors, a self-test that missed), if it does.
pub struct Outcome {
    pub json: Value,
    pub lines: Vec<String>,
    pub failure: Option<String>,
}

impl From<Value> for Outcome {
    fn from(json: Value) -> Outcome {
        Outcome { json, lines: Vec::new(), failure: None }
    }
}

pub fn run(cmd: PbrCmd) -> anyhow::Result<Outcome> {
    match cmd {
        PbrCmd::List { mount } => list(&mount, &std::env::current_dir().context("the current folder")?).map(Outcome::from),
        PbrCmd::Check { folder } => check(&folder),
        PbrCmd::Selftest => {
            let t = pbr_check::selftest();
            let failure = (!t.pass()).then(|| "the self-test's findings or formats differ from those expected".to_string());
            Ok(Outcome { json: t.json(), lines: vec![t.line()], failure })
        }
    }
}

/// `fnvtool pbr ...` from `main`: the document on stdout, then the extra lines; exit 1 on
/// an error or a failing outcome (the document is still printed).
pub fn main(cmd: PbrCmd) -> std::process::ExitCode {
    match run(cmd) {
        Ok(o) => {
            crate::emit(&o.json);
            for l in &o.lines {
                println!("{l}");
            }
            match o.failure {
                None => std::process::ExitCode::SUCCESS,
                Some(why) => {
                    eprintln!("error: {why}");
                    std::process::ExitCode::from(1)
                }
            }
        }
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::ExitCode::from(1)
        }
    }
}

/// Which folder `check` read, and how it was taken.
struct PackRoot {
    /// `mod` (holds `textures\pbr\`), `textures` (holds `pbr\`) or `pbr`.
    form: &'static str,
    /// The `pbr` folder itself.
    pbr: PathBuf,
    /// The game path of the `pbr` folder, spelled as the pack's folders are where the pack
    /// has them (`textures\pbr\`, `Textures\PBR\`).
    prefix: String,
}

/// The child folder of `dir` named `name`, ASCII case ignored (paths match that way in
/// the game, so `Textures\PBR` on a case-sensitive disk counts).
fn child_dir(dir: &Path, name: &str) -> Option<(PathBuf, String)> {
    // Listed rather than joined, so the name is spelled as on disk also where the disk
    // ignores case.
    std::fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let n = e.file_name().to_string_lossy().into_owned();
        (n.eq_ignore_ascii_case(name) && e.path().is_dir()).then(|| (e.path(), n))
    })
}

/// The pack root a folder stands for. A folder that names itself first: a `pbr` folder
/// inside a `textures` folder, then a `textures` folder with `pbr\` in it, so a pack's
/// own subfolder named `pbr` or `textures` is never taken for the root. Then by what it
/// holds: a mod folder with `textures\pbr\` in it, else a folder with `pbr\` in it (the
/// `textures` folder), else any folder named `pbr`.
fn pack_root(folder: &Path) -> anyhow::Result<PackRoot> {
    anyhow::ensure!(folder.is_dir(), "{}: not a folder", folder.display());
    let canonical = folder.canonicalize().ok();
    let name_of = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned());
    let own = canonical.as_deref().and_then(name_of);
    let parent = canonical.as_deref().and_then(Path::parent).and_then(name_of);
    if let (Some(n), Some(t)) = (&own, &parent) {
        if n.eq_ignore_ascii_case("pbr") && t.eq_ignore_ascii_case("textures") {
            return Ok(PackRoot { form: "pbr", pbr: folder.to_path_buf(), prefix: format!("{t}\\{n}\\") });
        }
    }
    if let Some(t) = own.as_ref().filter(|t| t.eq_ignore_ascii_case("textures")) {
        if let Some((pbr, pbr_name)) = child_dir(folder, "pbr") {
            return Ok(PackRoot { form: "textures", pbr, prefix: format!("{t}\\{pbr_name}\\") });
        }
    }
    if let Some((tex, tex_name)) = child_dir(folder, "textures") {
        if let Some((pbr, pbr_name)) = child_dir(&tex, "pbr") {
            return Ok(PackRoot { form: "mod", pbr, prefix: format!("{tex_name}\\{pbr_name}\\") });
        }
    }
    if let Some((pbr, pbr_name)) = child_dir(folder, "pbr") {
        return Ok(PackRoot { form: "textures", pbr, prefix: format!("textures\\{pbr_name}\\") });
    }
    match own {
        Some(n) if n.eq_ignore_ascii_case("pbr") => Ok(PackRoot { form: "pbr", pbr: folder.to_path_buf(), prefix: format!("textures\\{n}\\") }),
        _ => anyhow::bail!("{}: has no textures\\pbr\\ or pbr\\ folder in it and is not itself a folder named pbr", folder.display()),
    }
}

/// Every file under `dir`, with its path below `dir` joined by backslashes.
fn walk(dir: &Path, rel: &str, out: &mut Vec<(PathBuf, String)>) -> anyhow::Result<()> {
    for e in std::fs::read_dir(dir).with_context(|| dir.display().to_string())? {
        let e = e.with_context(|| dir.display().to_string())?;
        let name = e.file_name().to_string_lossy().into_owned();
        let r = if rel.is_empty() { name } else { format!("{rel}\\{name}") };
        let t = e.file_type().with_context(|| e.path().display().to_string())?;
        if t.is_dir() {
            walk(&e.path(), &r, out)?;
        } else if e.path().is_file() {
            out.push((e.path(), r));
        }
    }
    Ok(())
}

/// `pbr check`: each file's head (`pbr_check::head_len`) and length read, the pack judged.
fn check(folder: &Path) -> anyhow::Result<Outcome> {
    use std::io::Read;
    let root = pack_root(folder)?;
    let mut found = Vec::new();
    walk(&root.pbr, "", &mut found)?;
    let mut inputs = Vec::with_capacity(found.len());
    for (file, rel) in found {
        let path = format!("{}{rel}", root.prefix);
        let at = || file.display().to_string();
        let len = std::fs::metadata(&file).with_context(at)?.len();
        let head = match pbr_check::head_len(&path) {
            None => std::fs::read(&file).with_context(at)?,
            Some(0) => Vec::new(),
            Some(n) => {
                let mut buf = Vec::with_capacity(n);
                std::fs::File::open(&file).with_context(at)?.take(n as u64).read_to_end(&mut buf).with_context(at)?;
                buf
            }
        };
        inputs.push((path, head, len));
    }
    let pack = pbr_check::check_pack(inputs);
    let json = pack.json(json!({"folder": folder.display().to_string(), "form": root.form, "pbr_folder": root.pbr.display().to_string()}));
    let errors = pack.errors();
    let failure = (errors > 0).then(|| format!("{errors} error(s) in the pack (see `findings`)"));
    Ok(Outcome { json, lines: Vec::new(), failure })
}

/// `pbr list`, relative folders taken from `cwd`. The engine refuses to start with a
/// damaged archive (engine `main.rs`, `damaged_archives`), and a listing without its files
/// would mislead, so a damaged archive fails the command, as `fnvtool bsa check` does.
fn list(a: &MountArgs, cwd: &Path) -> anyhow::Result<Value> {
    let m = mount_vfs(a, cwd)?;
    let broken: Vec<String> = m.vfs.broken_archives().iter().map(|(n, why)| format!("{n}: {why}")).collect();
    anyhow::ensure!(broken.is_empty(), "damaged archive(s), which the engine refuses to start with: {}", broken.join("; "));
    Ok(listing(&m))
}

/// A path as given, made absolute against `cwd`: a relative mods root would otherwise
/// reach `Vfs::open` as an archive name relative to Data, and its archives would be lost.
fn absolute(cwd: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    }
}

/// The VFS mounted, and the exact inputs it was mounted from.
struct Mount {
    vfs: Vfs,
    /// Where the plugin list came from.
    plugins_from: &'static str,
    plugins_asked: Vec<String>,
    /// The plugins opened and loadable, as the VFS was given them (file names).
    plugins: Vec<String>,
    /// (plugin, why) for each left out.
    dropped: Vec<(String, String)>,
    /// The base archive list set for the run and where it came from; `None`: the shipped one.
    archive_list: Option<(Vec<String>, String)>,
}

/// Mount the VFS as the engine does at a developer start (`--data`), with the root crates'
/// own steps (engine `main.rs` and `world::open_plugins`): plugins looked up through the
/// mods folders (`fnv_install::plugins::plugin_path_in`), opened, and those unreadable or
/// with an unmet master left out (`fnv_esm::loadorder::drop_unloadable`), then
/// `Vfs::open_for_plugins_layered` under `--mo2`, else `Vfs::open_for_plugins_in` with the
/// mods folder. Relative folders are taken from `cwd`.
fn mount_vfs(a: &MountArgs, cwd: &Path) -> anyhow::Result<Mount> {
    let data = absolute(cwd, &a.data);
    anyhow::ensure!(data.is_dir(), "{}: not a folder", data.display());
    let mut archive_list = None;
    if let Some(l) = &a.archive_list {
        let list = fnv_vfs::parse_archive_list(l);
        fnv_vfs::set_archive_list(list.clone()).map_err(|_| anyhow::anyhow!("--archive-list: no archive named"))?;
        archive_list = Some((list, "--archive-list".to_string()));
    }
    let masters = || fnv_vfs::DEFAULT_PLUGINS.iter().map(|p| p.to_string()).collect::<Vec<_>>();
    // The VFS's layers over Data, lowest first (the plugin lookup takes them highest first).
    let (layout, layers): (Option<fnv_install::mo2::Layout>, Vec<PathBuf>) = match &a.mo2 {
        Some(instance) => {
            let layout = fnv_install::mo2::Layout::open(&absolute(cwd, instance), a.profile.as_deref()).map_err(|e| anyhow::anyhow!(e))?;
            let roots = layout.mod_roots().iter().map(|r| absolute(cwd, r)).collect();
            (Some(layout), roots)
        }
        None => (None, a.mods.iter().map(|m| absolute(cwd, m)).collect()),
    };
    let (plugins_from, asked) = match (&layout, a.plugins.is_empty()) {
        (_, false) => ("arguments", a.plugins.clone()),
        (Some(l), true) => match l.profile_plugins().map_err(|e| anyhow::anyhow!(e))? {
            p if p.is_empty() => {
                eprintln!("warning: the Mod Organizer 2 profile has no active plugins; loading the masters only");
                ("the base game and add-on masters (the profile has no active plugins)", masters())
            }
            p => ("the Mod Organizer 2 profile", p),
        },
        // Without plugins the engine takes the masters that are in Data (the add-ons are
        // optional, engine `main.rs`).
        (None, true) => {
            let present = masters().into_iter().enumerate().filter(|(i, n)| *i == 0 || fnv_install::plugins::plugin_path(&data, None, n).is_file()).map(|(_, n)| n).collect();
            ("the base game and the add-on masters in Data", present)
        }
    };
    if let (Some(l), None) = (&layout, &archive_list) {
        if let Some((list, from)) = l.profile_archives() {
            fnv_vfs::set_archive_list(list.clone()).map_err(|_| anyhow::anyhow!("the profile's archive list names no archive"))?;
            archive_list = Some((list, from.display().to_string()));
        }
    }
    let lookup: Vec<PathBuf> = layers.iter().rev().cloned().collect();
    let mut dropped = Vec::new();
    let mut opened = Vec::new();
    for n in &asked {
        // A full path (`--mo2-profile`) is opened as is; the VFS is given its file name, the
        // name the engine's load order holds.
        let name = Path::new(n).file_name().map_or_else(|| n.clone(), |f| f.to_string_lossy().into_owned());
        let path = if Path::new(n).is_absolute() { PathBuf::from(n) } else { fnv_install::plugins::plugin_path_in(&data, &lookup, n) };
        match fnv_esm::Plugin::open(&path) {
            Ok(mut p) => {
                p.name = name;
                opened.push(p);
            }
            Err(e) => dropped.push((name, format!("cannot be read: {e}"))),
        }
    }
    let (kept, refused) = fnv_esm::loadorder::drop_unloadable(opened);
    dropped.extend(refused.into_iter().map(|d| (d.plugin, d.reason.to_string())));
    for (p, why) in &dropped {
        eprintln!("warning: plugin {p} left out: {why}");
    }
    let plugins: Vec<String> = kept.into_iter().map(|p| p.name).collect();
    let vfs = match &layout {
        Some(_) => Vfs::open_for_plugins_layered(&data, &layers, &plugins),
        None => Vfs::open_for_plugins_in(&data, layers.first().map(PathBuf::as_path), &plugins),
    }
    .with_context(|| data.display().to_string())?;
    Ok(Mount { vfs, plugins_from, plugins_asked: asked, plugins, dropped, archive_list })
}

/// The loose folder that indexed `file` under `key`: the root whose path to the file,
/// normalised, is the key (a prefix alone names the wrong root when roots nest).
fn loose_root<'a>(vfs: &'a Vfs, key: &str, file: &Path) -> Option<&'a PathBuf> {
    vfs.loose_roots().iter().rev().find(|r| file.strip_prefix(r).is_ok_and(|rel| fnv_vfs::normalize(&rel.to_string_lossy()) == key))
}

/// Where the file `path` that wins in `vfs` comes from: an archive (its name and file) or a
/// loose folder (the root that indexed it and the file).
fn source(vfs: &Vfs, path: &str) -> Value {
    match vfs.source_of(path) {
        Some("loose") => {
            let file = vfs.loose_path(path);
            let key = fnv_vfs::normalize(path);
            let root = file.as_ref().and_then(|f| loose_root(vfs, &key, f));
            json!({
                "kind": "loose",
                "root": root.map(|r| r.display().to_string()),
                "file": file.map(|f| f.display().to_string()),
            })
        }
        Some(archive) => json!({
            "kind": "archive",
            "archive": archive,
            "file": vfs.archive_path(archive).map(|f| f.display().to_string()),
        }),
        None => Value::Null,
    }
}

fn listing(m: &Mount) -> Value {
    let vfs = &m.vfs;
    let index = PbrIndex::build(vfs.paths(|k| k.starts_with(PBR_DIR)));
    let sets: Vec<Value> = index
        .sets()
        .map(|(stem, files)| {
            let mut members = serde_json::Map::new();
            for m in files.members() {
                let path = member_path(stem, m);
                members.insert(m.name().to_string(), json!({"path": path, "source": source(vfs, &path)}));
            }
            json!({
                "stem": stem,
                "diffuse": fnv_pbr::diffuse_stem(stem),
                "state": files.state().name(),
                "members": members,
            })
        })
        .collect();
    let c = index.counts();
    json!({
        "schema": "fnv.pbr.listing/1",
        "mount": {
            "data": vfs.data_dir().display().to_string(),
            "loose_roots": vfs.loose_roots().iter().map(|r| r.display().to_string()).collect::<Vec<_>>(),
            "plugins_from": m.plugins_from,
            "plugins_asked": m.plugins_asked,
            "plugins": m.plugins,
            "plugins_dropped": m.dropped.iter().map(|(p, why)| json!({"plugin": p, "why": why})).collect::<Vec<_>>(),
            "archive_list": m.archive_list.as_ref().map(|(names, from)| json!({"names": names, "from": from})),
            "archive_order_env": std::env::var(fnv_vfs::ARCHIVE_ORDER_ENV).ok().filter(|v| !v.trim().is_empty()),
            "archives": vfs.archives().map(|n| json!({"name": n, "file": vfs.archive_path(n).map(|f| f.display().to_string())})).collect::<Vec<_>>(),
        },
        "sets": sets,
        "unmatched": index.unmatched(),
        "totals": {
            "files": c.files,
            "sets": c.sets,
            "active": c.active(),
            "authored": c.authored,
            "height_only": c.height_only,
            "incomplete": c.incomplete,
            "unmatched": c.unmatched,
            "members": {"orm": c.orm, "normal": c.normal, "height": c.height, "scalars": c.scalars, "albedo": c.albedo},
        },
        "summary": index.summary(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(data: &str, plugins: &[&str], mods: Option<&str>) -> MountArgs {
        MountArgs {
            data: PathBuf::from(data),
            plugins: plugins.iter().map(|p| p.to_string()).collect(),
            mods: mods.map(PathBuf::from),
            mo2: None,
            profile: None,
            archive_list: None,
        }
    }

    fn file(dir: &Path, rel: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, rel).unwrap();
    }

    /// A plugin with only its TES4 header, needing `masters`.
    fn plugin(dir: &Path, name: &str, masters: &[&str]) {
        let sub = |sig: &[u8; 4], data: &[u8]| [&sig[..], &(data.len() as u16).to_le_bytes(), data].concat();
        let mut payload = sub(b"HEDR", &[1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat());
        for m in masters {
            payload.extend(sub(b"MAST", &[m.as_bytes(), &[0]].concat()));
            payload.extend(sub(b"DATA", &[0; 8]));
        }
        let record = [&b"TES4"[..], &(payload.len() as u32).to_le_bytes(), &[0; 12], &15u16.to_le_bytes(), &[0; 2], &payload].concat();
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), record).unwrap();
    }

    /// An archive holding one file `folder\file`.
    fn archive(dir: &Path, name: &str, folder: &str, file: &str) {
        use fnv_bsa::{encode_block, flags, write_archive, WriteFile, WriteFolder};
        let files = vec![WriteFile { name: file.as_bytes().to_vec(), size_flag_bits: 0, data: encode_block(b"x", false, None) }];
        let mut bytes = Vec::new();
        write_archive(&mut bytes, flags::DIR_NAMES | flags::FILE_NAMES, 0x1, 0, &[WriteFolder { name: folder.as_bytes().to_vec(), files }]).unwrap();
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), bytes).unwrap();
    }

    fn fresh(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("nvf-fnvtool-pbr-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn list_names_sets_states_and_the_source_of_each_member() {
        let root = fresh("list");
        let (data, mods) = (root.join("Data"), root.join("mods"));
        file(&data, "textures/pbr/walls/brick07_orm.dds");
        file(&data, "textures/pbr/walls/brick07_n.dds");
        file(&mods, "textures/pbr/walls/brick07_orm.dds");
        file(&mods, "textures/pbr/walls/brick07.toml");
        file(&mods, "textures/pbr/props/crate_p.dds");
        file(&mods, "textures/pbr/props/lamp_n.dds");
        file(&mods, "textures/pbr/props/lamp.dds");
        file(&data, "textures/walls/brick07.dds");

        let v = list(&args(&data.to_string_lossy(), &[], Some(&mods.to_string_lossy())), &root).unwrap();
        assert_eq!(v["schema"], "fnv.pbr.listing/1");
        // No plugin in this Data: the base game's master is asked for and left out.
        assert_eq!(v["mount"]["plugins_asked"], json!(["FalloutNV.esm"]));
        assert_eq!(v["mount"]["plugins"], json!([]));
        assert_eq!(v["mount"]["plugins_dropped"][0]["plugin"], "FalloutNV.esm");
        let sets = v["sets"].as_array().unwrap();
        let stems: Vec<&str> = sets.iter().map(|s| s["stem"].as_str().unwrap()).collect();
        assert_eq!(stems, [r"textures\pbr\props\crate", r"textures\pbr\props\lamp", r"textures\pbr\walls\brick07"]);
        assert_eq!(sets[0]["state"], "height-only");
        assert_eq!(sets[1]["state"], "incomplete");
        let brick = &sets[2];
        assert_eq!(brick["state"], "authored");
        assert_eq!(brick["diffuse"], r"textures\walls\brick07");
        let root_of = |m: &str| brick["members"][m]["source"]["root"].as_str().map(PathBuf::from);
        // Each member resolves on its own: the mod's ORM and scalars, Data's normal.
        assert_eq!(root_of("orm"), Some(mods.clone()));
        assert_eq!(root_of("scalars"), Some(mods.clone()));
        assert_eq!(root_of("normal"), Some(data.clone()));
        assert_eq!(brick["members"]["orm"]["source"]["kind"], "loose");
        assert_eq!(brick["members"]["orm"]["path"], r"textures\pbr\walls\brick07_orm.dds");
        assert!(brick["members"].get("height").is_none());
        assert_eq!(v["unmatched"], json!([r"textures\pbr\props\lamp.dds"]));
        let t = &v["totals"];
        assert_eq!((t["files"].as_u64(), t["sets"].as_u64(), t["active"].as_u64()), (Some(6), Some(3), Some(2)));
        assert_eq!((t["authored"].as_u64(), t["height_only"].as_u64(), t["incomplete"].as_u64(), t["unmatched"].as_u64()), (Some(1), Some(1), Some(1), Some(1)));
        assert_eq!(t["members"], json!({"orm": 1, "normal": 2, "height": 1, "scalars": 1, "albedo": 0}));
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Sibling `Data` and `mods` given as relative folders: the mods folder's plugin-owned
    /// archive is mounted (a relative root once reached `Vfs::open` joined onto Data and was
    /// lost), beside its loose files.
    #[test]
    fn a_relative_mods_root_keeps_its_archives() {
        let root = fresh("relative");
        let (data, mods) = (root.join("Data"), root.join("mods"));
        plugin(&data, "Example.esp", &[]);
        archive(&mods, "Example.bsa", r"textures\pbr\walls", "brick_orm.dds");
        file(&mods, "textures/pbr/walls/brick_n.dds");

        let v = list(&args("Data", &["Example.esp"], Some("mods")), &root).unwrap();
        assert_eq!(v["mount"]["plugins"], json!(["Example.esp"]));
        assert_eq!(v["mount"]["data"], data.display().to_string());
        let brick = &v["sets"][0];
        assert_eq!(brick["stem"], r"textures\pbr\walls\brick");
        assert_eq!(brick["state"], "authored");
        let orm = &brick["members"]["orm"]["source"];
        assert_eq!((orm["kind"].as_str(), orm["archive"].as_str()), (Some("archive"), Some(mods.join("Example.bsa").to_string_lossy().as_ref())));
        assert_eq!(orm["file"].as_str().map(PathBuf::from), Some(mods.join("Example.bsa")));
        assert_eq!(brick["members"]["normal"]["source"]["root"].as_str().map(PathBuf::from), Some(mods.clone()));
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Plugins are opened first, as the engine does: one missing, one whose master is
    /// missing, are left out with why, and the second's archive is not mounted.
    #[test]
    fn unloadable_plugins_are_dropped_and_named() {
        let root = fresh("dropped");
        let data = root.join("Data");
        plugin(&data, "Base.esm", &[]);
        plugin(&data, "Needs.esp", &["Missing.esm"]);
        archive(&data, "Needs.bsa", r"textures\pbr\walls", "lost_orm.dds");
        archive(&data, "Base.bsa", r"textures\pbr\walls", "kept_orm.dds");

        let v = list(&args("Data", &["Base.esm", "Gone.esp", "Needs.esp"], None), &root).unwrap();
        let m = &v["mount"];
        assert_eq!(m["plugins_asked"], json!(["Base.esm", "Gone.esp", "Needs.esp"]));
        assert_eq!(m["plugins"], json!(["Base.esm"]));
        let dropped: Vec<&str> = m["plugins_dropped"].as_array().unwrap().iter().map(|d| d["plugin"].as_str().unwrap()).collect();
        assert_eq!(dropped, ["Gone.esp", "Needs.esp"]);
        assert!(m["plugins_dropped"][0]["why"].as_str().unwrap().starts_with("cannot be read"));
        let archives: Vec<&str> = m["archives"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert_eq!(archives, ["Base.bsa"]);
        let stems: Vec<&str> = v["sets"].as_array().unwrap().iter().map(|s| s["stem"].as_str().unwrap()).collect();
        assert_eq!(stems, [r"textures\pbr\walls\kept"]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A damaged archive fails the listing, as it stops the engine.
    #[test]
    fn a_damaged_archive_fails_the_listing() {
        let root = fresh("broken");
        let data = root.join("Data");
        plugin(&data, "Example.esp", &[]);
        std::fs::write(data.join("Example.bsa"), b"not an archive").unwrap();
        let e = list(&args("Data", &["Example.esp"], None), &root).unwrap_err().to_string();
        assert!(e.contains("damaged archive") && e.contains("Example.bsa"), "{e}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The loose root named is the one that indexed the file under its key, also when a
    /// mods folder lies inside Data (a path prefix alone would name the mods folder for
    /// Data's file).
    #[test]
    fn nested_roots_name_the_root_that_indexed_the_file() {
        let root = fresh("nested");
        let data = root.join("Data");
        let mods = data.join("textures");
        file(&data, "textures/pbr/walls/brick_orm.dds");
        let v = list(&args("Data", &[], Some("Data/textures")), &root).unwrap();
        let orm = &v["sets"][0]["members"]["orm"]["source"];
        assert_eq!(v["sets"][0]["stem"], r"textures\pbr\walls\brick");
        assert_eq!(orm["root"].as_str().map(PathBuf::from), Some(data.clone()));
        // The mods folder indexed the same file under `pbr\…`, outside textures\pbr\.
        assert!(mods.join("pbr").is_dir());
        std::fs::remove_dir_all(&root).unwrap();
    }

    use crate::pbr_check::dx10_dds;

    fn put(dir: &Path, rel: &str, bytes: &[u8]) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    /// `dx10_dds` with one of the DX10 extension's words (at `ext_word`) set to `v`.
    fn dx10_with(dxgi: u32, w: u32, h: u32, mips: u32, ext_word: usize, v: u32) -> Vec<u8> {
        let mut d = dx10_dds(dxgi, w, h, mips, 0);
        let o = 128 + 4 * ext_word;
        d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        d
    }

    /// A legacy DXT1 file (no DX10 extension), built here, with the payload its layout needs.
    fn legacy_dxt1(w: u32, h: u32, mips: u32) -> Vec<u8> {
        let mut d = dx10_dds(71, w, h, mips, 0);
        d.truncate(128);
        d[84..88].copy_from_slice(b"DXT1");
        let need: usize = (0..mips).map(|i| fnv_dds::Format::Dxt1.level_size((w >> i).max(1), (h >> i).max(1))).sum();
        d.resize(128 + need, 0);
        d
    }

    fn clean_set(pbr: &Path) {
        put(pbr, "walls/brick_orm.dds", &dx10_dds(98, 8, 8, 4, 0));
        put(pbr, "walls/brick_n.dds", &dx10_dds(83, 16, 16, 5, 0));
        put(pbr, "walls/brick_p.dds", &dx10_dds(80, 4, 4, 3, 0));
        put(pbr, "walls/brick.toml", b"roughness_scale = 0.9\nreflectance = 0.35\n");
    }

    fn found(v: &Value) -> Vec<(String, String, String)> {
        v["findings"].as_array().unwrap().iter().map(|f| (f["level"].as_str().unwrap().to_string(), f["code"].as_str().unwrap().to_string(), f["path"].as_str().unwrap().to_string())).collect()
    }

    /// A clean set passes with no finding; every error and warning is raised by the one
    /// file made to raise it, and nothing else is.
    #[test]
    fn check_raises_each_finding_on_its_file_only() {
        let root = fresh("check");
        let pbr = root.join("mod").join("textures").join("pbr");
        clean_set(&pbr);
        let clean = check(&root.join("mod")).unwrap();
        assert!(clean.failure.is_none());
        assert_eq!(clean.json["ok"], true);
        assert_eq!(found(&clean.json), Vec::<(String, String, String)>::new());

        put(&pbr, "legacy/old_orm.dds", &legacy_dxt1(4, 4, 3));
        // Errors.
        put(&pbr, "e/unreadable_orm.dds", b"not a texture");
        put(&pbr, "e/format_orm.dds", &dx10_dds(83, 4, 4, 3, 0));
        put(&pbr, "e/unaligned_orm.dds", &dx10_dds(98, 8, 2, 4, 0));
        put(&pbr, "e/cube_orm.dds", &dx10_with(98, 4, 4, 3, 2, 0x4));
        put(&pbr, "e/array_orm.dds", &dx10_with(98, 4, 4, 3, 3, 2));
        put(&pbr, "e/volume_orm.dds", &dx10_with(98, 4, 4, 3, 1, 4));
        put(&pbr, "e/zero_orm.dds", &{
            let mut d = dx10_dds(98, 4, 4, 1, 0);
            d[16..20].copy_from_slice(&0u32.to_le_bytes());
            d
        });
        put(&pbr, "e/truncated_orm.dds", &dx10_dds(98, 8, 8, 4, -1));
        put(&pbr, "e/extra_orm.dds", &dx10_dds(98, 4, 4, 4, 0));
        put(&pbr, "e/badheader_orm.dds", &{
            let mut d = dx10_dds(98, 4, 4, 3, 0);
            d[76..80].copy_from_slice(&0u32.to_le_bytes());
            d
        });
        put(&pbr, "e/broken.toml", b"reflectance = = 1\n");
        put(&pbr, "e/broken_p.dds", &dx10_dds(80, 4, 4, 3, 0));
        // Warnings.
        put(&pbr, "w/nomips_orm.dds", &dx10_dds(98, 8, 8, 1, 0));
        put(&pbr, "w/srgb_orm.dds", &dx10_dds(99, 4, 4, 3, 0));
        put(&pbr, "w/lonely_n.dds", &dx10_dds(83, 4, 4, 3, 0));
        put(&pbr, "w/readme.txt", b"notes");
        put(&pbr, "w/odd.toml", b"normal_strength = 2\nreflectance = 3\n");
        put(&pbr, "w/odd_p.dds", &dx10_dds(80, 4, 4, 3, 0));
        put(&pbr, "w/future_albedo.dds", b"reserved");
        put(&pbr, "w/future_orm.dds", &dx10_dds(98, 4, 4, 3, 0));
        put(&pbr, "w/trailing_orm.dds", &dx10_dds(98, 4, 4, 3, 5));
        put(&pbr, "w/npot_orm.dds", &dx10_dds(98, 12, 12, 4, 0));

        let o = check(&root.join("mod")).unwrap();
        let v = &o.json;
        assert_eq!(v["schema"], "fnv.pbr.check/1");
        let p = |rel: &str| format!("textures\\pbr\\{rel}");
        let (e, w) = ("error", "warning");
        let mut want: Vec<(String, String, String)> = [
            (e, "toml-invalid", p("e\\broken.toml")),
            (e, "not-single-2d", p("e\\array_orm.dds")),
            (e, "not-single-2d", p("e\\cube_orm.dds")),
            (e, "format-not-allowed", p("e\\format_orm.dds")),
            (e, "truncated", p("e\\truncated_orm.dds")),
            (e, "not-block-aligned", p("e\\unaligned_orm.dds")),
            (e, "dds-unreadable", p("e\\unreadable_orm.dds")),
            (e, "not-single-2d", p("e\\volume_orm.dds")),
            (e, "bad-size", p("e\\zero_orm.dds")),
            (e, "extra-mips", p("e\\extra_orm.dds")),
            (e, "bad-header-size", p("e\\badheader_orm.dds")),
            (w, "albedo-reserved", p("w\\future_albedo.dds")),
            (w, "incomplete-set", p("w\\lonely")),
            (w, "incomplete-mips", p("w\\nomips_orm.dds")),
            (w, "not-power-of-two", p("w\\npot_orm.dds")),
            (w, "scalar-ignored-or-clamped", p("w\\odd.toml")),
            (w, "scalar-ignored-or-clamped", p("w\\odd.toml")),
            (w, "unmatched-file", p("w\\readme.txt")),
            (w, "srgb-linear-member", p("w\\srgb_orm.dds")),
            (w, "trailing-bytes", p("w\\trailing_orm.dds")),
        ]
        .into_iter()
        .map(|(l, c, p)| (l.to_string(), c.to_string(), p))
        .collect();
        let mut got = found(v);
        got.sort();
        want.sort();
        assert_eq!(got, want);
        assert_eq!(v["ok"], false);
        assert_eq!(o.failure.as_deref(), Some("11 error(s) in the pack (see `findings`)"));
        let t = &v["totals"];
        assert_eq!((t["errors"].as_u64(), t["warnings"].as_u64()), (Some(11), Some(9)));
        assert_eq!(t["sets"], json!({"total": 20, "authored": 17, "height_only": 2, "incomplete": 1}));
        assert_eq!(t["unmatched"], 1);
        assert_eq!(t["members"]["normal"], json!({"files": 2, "formats": {"BC5_UNORM": 2}, "largest_side": 16}));
        assert_eq!(t["members"]["height"], json!({"files": 3, "formats": {"BC4_UNORM": 3}, "largest_side": 4}));
        assert_eq!(t["members"]["orm"]["formats"]["DXT1"], 1);
        assert_eq!(t["members"]["orm"]["formats"]["BC7_UNORM_SRGB"], 1);
        assert_eq!(t["members"]["orm"]["largest_side"], 12);
        assert_eq!((t["members"]["scalars"]["files"].as_u64(), t["members"]["albedo"]["files"].as_u64()), (Some(3), Some(1)));
        // Payloads of the accepted members of active sets: brick 112 + 368 + 24, old 24,
        // broken_p 24, nomips 64, srgb 48, odd_p 24, future 48, trailing 48, npot 240.
        // Not lonely_n (its set is incomplete) nor any file with an error.
        assert_eq!(t["gpu_bytes_estimate"], 1024);
        let row = |path: &str| v["files"].as_array().unwrap().iter().find(|f| f["path"] == path).unwrap().clone();
        let brick = row(&p("walls\\brick_n.dds"));
        assert_eq!((brick["format"].clone(), brick["width"].clone(), brick["levels"].clone(), brick["payload_bytes"].clone(), brick["accepted"].clone()), (json!("BC5_UNORM"), json!(16), json!(5), json!(368), json!(true)));
        assert_eq!(row(&p("w\\readme.txt"))["member"], json!(null));
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The mod folder, its `textures` folder and the `pbr` folder give the same report,
    /// paths spelled as the pack's folders are; another folder is refused.
    #[test]
    fn check_takes_three_root_forms() {
        let root = fresh("roots");
        let pbr = root.join("mod").join("Textures").join("PBR");
        clean_set(&pbr);
        let forms = [(root.join("mod"), "mod"), (root.join("mod").join("Textures"), "textures"), (pbr.clone(), "pbr")];
        for (folder, form) in forms {
            let o = check(&folder).unwrap();
            assert!(o.failure.is_none(), "{form}");
            assert_eq!(o.json["root"]["form"], form);
            assert_eq!(o.json["root"]["pbr_folder"].as_str().map(PathBuf::from), Some(pbr.clone()), "{form}");
            let paths: Vec<&str> = o.json["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap()).collect();
            assert_eq!(paths, ["brick.toml", "brick_n.dds", "brick_orm.dds", "brick_p.dds"].map(|n| format!("Textures\\PBR\\walls\\{n}")), "{form}");
            assert_eq!(o.json["totals"]["sets"]["authored"], 1);
        }
        let Err(e) = check(&root) else { panic!("a folder with no pack was taken") };
        let e = e.to_string();
        assert!(e.contains("is not itself a folder named pbr"), "{e}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A `textures\pbr` folder given directly is the root also when the pack has a
    /// subfolder named `pbr` (and a `textures` folder when it has one named `textures`):
    /// the sets beside that subfolder are read, and the subfolder's files too.
    #[test]
    fn a_named_root_wins_over_a_child_of_the_same_name() {
        let root = fresh("named-root");
        let pbr = root.join("mod").join("textures").join("pbr");
        clean_set(&pbr);
        put(&pbr, "pbr/rock_orm.dds", &dx10_dds(98, 4, 4, 3, 0));
        put(&pbr, "textures/pbr/sign_orm.dds", &dx10_dds(98, 4, 4, 3, 0));
        let names = ["pbr\\rock_orm.dds", "textures\\pbr\\sign_orm.dds", "walls\\brick.toml", "walls\\brick_n.dds", "walls\\brick_orm.dds", "walls\\brick_p.dds"];
        let want: Vec<String> = names.iter().map(|n| format!("textures\\pbr\\{n}")).collect();
        for (folder, form) in [(pbr.clone(), "pbr"), (root.join("mod").join("textures"), "textures"), (root.join("mod"), "mod")] {
            let o = check(&folder).unwrap();
            assert_eq!(o.json["root"]["form"], form);
            assert_eq!(o.json["root"]["pbr_folder"].as_str().map(PathBuf::from), Some(pbr.clone()), "{form}");
            let paths: Vec<&str> = o.json["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap()).collect();
            assert_eq!(paths, want, "{form}");
            assert_eq!(o.json["totals"]["sets"]["authored"], 3, "{form}");
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The self-test raises exactly its expected findings and reads the clean set's three
    /// formats.
    #[test]
    fn selftest_passes() {
        let t = pbr_check::selftest();
        assert!(t.pass(), "{}", t.line());
        assert_eq!(t.line(), "pbr selftest: PASS (3 formats, 7 expected findings)");
        let v = t.json();
        assert_eq!(v["selftest"]["formats"], json!(["BC4_UNORM", "BC5_UNORM", "BC7_UNORM"]));
        assert_eq!(v["root"]["form"], "selftest");
        assert_eq!(v["selftest"]["verdict"], t.line());
        assert!(!v.to_string().contains("FAIL"));
    }
}
