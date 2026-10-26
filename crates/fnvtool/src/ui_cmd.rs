//! `fnvtool ui ...`: the menu files read into the tile tree (fnv-ui;
//! docs/spec-notes/menu-xml.md, docs/design/ui-tile-tree.md). Reads the owner's
//! archives and loose files; prints counts, mended spots and trait values, never the
//! files' text.

use clap::Subcommand;
use fnv_ui::{Source, TileId, Ui, Value as UiValue};
use fnv_vfs::Vfs;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum UiCmd {
    /// Every menu file under `menus\` built into the tree and worked out: tiles, traits,
    /// expressions, what the reader mended, includes not found (fnv.ui.menus/1).
    Menus {
        #[command(flatten)]
        src: UiSourceArgs,
    },
    /// One menu's tree with its traits' values (fnv.ui.tree/1).
    Tree {
        #[command(flatten)]
        src: UiSourceArgs,
        /// The menu file, e.g. `menus\main\hud_main_menu.xml`.
        #[arg(long)]
        menu: String,
        /// Only the subtree a path names, e.g. `HUDMainMenu/HitPoints`.
        #[arg(long)]
        at: Option<String>,
    },
}

#[derive(clap::Args)]
pub struct UiSourceArgs {
    /// The game's Data folder (its archives and loose files).
    #[arg(long)]
    data: PathBuf,
    /// A mod's folder laid over Data, e.g. an MO2 mod folder (repeatable; later ones win).
    #[arg(long)]
    overlay: Vec<PathBuf>,
    /// The view's size for `screen()` (width x height).
    #[arg(long, default_value = "1920x1080")]
    screen: String,
}

/// The VFS with loose overlay folders over it.
struct Files {
    vfs: Vfs,
    overlays: Vec<PathBuf>,
}

impl Files {
    fn loose(&self, path: &str) -> Option<PathBuf> {
        let rel = path.replace('\\', "/");
        self.overlays.iter().rev().find_map(|d| find_ci(d, &rel))
    }
}

/// A file under `dir` by a relative path in any case.
fn find_ci(dir: &Path, rel: &str) -> Option<PathBuf> {
    let mut at = dir.to_path_buf();
    for part in rel.split('/').filter(|p| !p.is_empty()) {
        let exact = at.join(part);
        at = if exact.exists() {
            exact
        } else {
            std::fs::read_dir(&at).ok()?.flatten().map(|e| e.path()).find(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(part))
            })?
        };
    }
    at.is_file().then_some(at)
}

impl Source for Files {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        if let Some(p) = self.loose(path) {
            return std::fs::read(p).ok();
        }
        self.vfs.read(path)?.ok()
    }
}

fn open(a: &UiSourceArgs) -> anyhow::Result<(Files, Ui)> {
    let vfs = Vfs::open_default(&a.data)?;
    let files = Files { vfs, overlays: a.overlay.clone() };
    let mut ui = Ui::new();
    ui.load_globals(&files);
    let (w, h) = a.screen.split_once(['x', 'X']).ok_or_else(|| anyhow::anyhow!("--screen: WxH"))?;
    let s = ui.screen();
    ui.set_trait(s, "width", UiValue::Num(w.trim().parse()?));
    ui.set_trait(s, "height", UiValue::Num(h.trim().parse()?));
    Ok((files, ui))
}

/// The menu files: the archives' and the overlays' `menus\*.xml` but the prefabs.
fn menu_files(files: &Files) -> Vec<String> {
    let mut v = files.vfs.paths(|p| p.starts_with("menus\\") && p.ends_with(".xml") && !p.starts_with("menus\\prefabs\\"));
    for d in &files.overlays {
        let mut stack = vec![d.clone()];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if let Ok(rel) = p.strip_prefix(d) {
                    let rel = rel.to_string_lossy().replace('/', "\\").to_ascii_lowercase();
                    if rel.starts_with("menus\\") && rel.ends_with(".xml") && !rel.starts_with("menus\\prefabs\\") {
                        v.push(rel);
                    }
                }
            }
        }
    }
    v.sort();
    v.dedup();
    v
}

fn counts(ui: &Ui, root: TileId) -> (usize, usize, usize) {
    let (mut tiles, mut traits, mut exprs) = (0, 0, 0);
    let mut stack = vec![root];
    while let Some(t) = stack.pop() {
        let tile = ui.tile(t);
        tiles += 1;
        traits += tile.traits.len();
        exprs += tile.traits.iter().filter(|tr| tr.expr.is_some()).count();
        stack.extend(tile.children.iter().copied());
    }
    (tiles, traits, exprs)
}

fn value_json(v: &UiValue) -> Value {
    match v {
        UiValue::Num(n) => json!(n),
        UiValue::Str(s) => json!(s),
        UiValue::Sym(s) => json!(format!("&{s};")),
    }
}

fn tree_json(ui: &Ui, t: TileId) -> Value {
    let tile = ui.tile(t);
    let mut traits = Map::new();
    for tr in &tile.traits {
        traits.insert(tr.name.clone(), value_json(&tr.value));
    }
    json!({
        "kind": tile.kind,
        "name": tile.name,
        "traits": traits,
        "templates": tile.templates.iter().map(|tp| tp.name.clone()).collect::<Vec<_>>(),
        "children": tile.children.iter().map(|&c| tree_json(ui, c)).collect::<Vec<_>>(),
    })
}

pub fn run(cmd: UiCmd) -> anyhow::Result<Value> {
    match cmd {
        UiCmd::Menus { src } => {
            let (files, mut ui) = open(&src)?;
            let globals_warnings = ui.warnings.len();
            let mut menus = Vec::new();
            for path in menu_files(&files) {
                ui.warnings.clear();
                match ui.load_menu(&files, &path) {
                    Ok(id) => {
                        let changed = ui.update();
                        let (tiles, traits, exprs) = counts(&ui, id);
                        menus.push(json!({
                            "file": path, "menu": ui.tile(id).name, "tiles": tiles, "traits": traits,
                            "expressions": exprs, "changed_on_update": changed,
                            "warnings": ui.warnings.clone(),
                        }));
                    }
                    Err(e) => menus.push(json!({ "file": path, "error": e, "warnings": ui.warnings.clone() })),
                }
            }
            let built = menus.iter().filter(|m| m.get("menu").is_some()).count();
            Ok(json!({ "schema": "fnv.ui.menus/1", "globals_warnings": globals_warnings, "built": built, "menus": menus }))
        }
        UiCmd::Tree { src, menu, at } => {
            let (files, mut ui) = open(&src)?;
            let id = ui.load_menu(&files, &menu).map_err(anyhow::Error::msg)?;
            ui.update();
            let root = match &at {
                Some(p) => ui.find_tile(p).ok_or_else(|| anyhow::anyhow!("{p}: no such tile"))?,
                None => id,
            };
            Ok(json!({ "schema": "fnv.ui.tree/1", "path": ui.path_of(root), "warnings": ui.warnings, "tree": tree_json(&ui, root) }))
        }
    }
}
