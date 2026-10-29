//! Files and startup loading for the tile tree [design: ui-tile-tree.md §3, §9].

use crate::{game::menus, Game};
use fnv_sim::{settings::Setting, World};
use fnv_ui::{
    value::{menu_class_name, menu_class_number},
    MenuIndex, Source, Ui, Value,
};
use fnv_vfs::Vfs;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

/// Menus kept for the run [documented: InjectUIXML notes, ui-tile-tree.md §3.3].
pub const ALWAYS_LOADED: &[&str] = &["HUDMainMenu", "InventoryMenu", "MapMenu", "RepairMenu", "StatsMenu", "ItemModMenu"];

/// Logging remembered until the tree is rebuilt after a load [design].
#[derive(Default)]
pub struct MenuLifetime {
    unavailable: BTreeSet<u16>,
    warnings: BTreeSet<String>,
}

#[derive(Default, Debug)]
pub struct LifetimeReport {
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
}

fn class_code(ui: &Ui, root: fnv_ui::TileId) -> Option<u16> {
    match ui.tile(root).get("class")? {
        Value::Sym(name) => menu_class_number(name),
        _ => None,
    }
}

fn warning_key(warning: &str) -> String {
    let (file, message) = warning.split_once(": ").unwrap_or(("tree", warning));
    format!("{}: {message}", file.replace('/', "\\").to_ascii_lowercase())
}

/// The engine calls before scripts, including while its own pause/main menu holds them
/// [design: ui-tile-tree.md §4.1; owner-review.md item 34]. No side menus or groups.
pub fn sync(game: &mut Game<'_>, paused: bool, main_menu: bool) -> LifetimeReport {
    let mut report = LifetimeReport::default();
    let Some(files) = game.ui_files.as_deref() else { return report };
    let mut open: BTreeSet<_> = game.menu.into_iter().chain(game.menu_under).collect();
    if game.dialogue.is_some() {
        open.insert(menus::DIALOGUE);
    }
    if paused || main_menu {
        open.insert(menus::START);
    }
    open.retain(|code| !matches!(*code, menus::PIPBOY | menus::LOADING | menus::VATS));
    let roots: Vec<_> = game.ui_tree.menus().collect();
    for root in roots {
        let tile = game.ui_tree.tile(root);
        if ALWAYS_LOADED.iter().any(|name| tile.name.eq_ignore_ascii_case(name)) {
            continue;
        }
        if let Some(code) = class_code(&game.ui_tree, root) {
            if !open.contains(&code) {
                game.ui_tree.unload_tile(root);
                report.changes.push(format!("dropped {}", menu_class_name(code).unwrap()));
            }
        }
    }
    let warning_start = game.ui_tree.warnings.len();
    for code in open {
        if game.ui_tree.menus().any(|root| class_code(&game.ui_tree, root) == Some(code)) || game.ui_lifetime.unavailable.contains(&code) {
            continue;
        }
        let name = menu_class_name(code).map_or_else(|| format!("menu {code}"), str::to_owned);
        let result = match game.ui_menus.code_path(code) {
            Some(path) => game.ui_tree.load_menu(files, path).map(|root| (path, root)),
            None => Err(format!("{name}: no file with matching menu class")),
        };
        match result {
            Ok((path, root)) => {
                let mut count = 0;
                let mut stack = vec![root];
                while let Some(id) = stack.pop() {
                    count += 1;
                    stack.extend(game.ui_tree.tile(id).children.iter().copied());
                }
                report.changes.push(format!("built {name} ({path}, {count} tiles)"));
            }
            Err(message) => {
                game.ui_lifetime.unavailable.insert(code);
                report.warnings.push(message);
            }
        }
    }
    // New menus' expressions must be readable by this frame's scripts [design: §4.1].
    if !report.changes.is_empty() {
        game.ui_tree.update();
    }
    for warning in &game.ui_tree.warnings[warning_start..] {
        if game.ui_lifetime.warnings.insert(warning_key(warning)) {
            report.warnings.push(warning.clone());
        }
    }
    report
}

pub struct UiFiles {
    vfs: Arc<Vfs>,
    strings: HashMap<String, String>,
}

impl UiFiles {
    pub fn new(vfs: Arc<Vfs>, world: &World<'_>) -> Self {
        // Own the strings: scripts keep this source after the world's borrow ends [design].
        let strings = world
            .forms
            .settings
            .keys()
            .filter_map(|name| match world.setting_value(name) {
                Some(Setting::Str(value)) => Some((name.to_ascii_lowercase(), value.clone())),
                _ => None,
            })
            .collect();
        Self { vfs, strings }
    }

    pub fn menu_paths(&self) -> Vec<String> {
        self.vfs.paths(|p| p.starts_with("menus\\") && p.ends_with(".xml") && !p.starts_with("menus\\prefabs\\"))
    }
}

impl Source for UiFiles {
    fn is_loose(&self, path: &str) -> bool {
        self.vfs.source_of(path) == Some("loose")
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.vfs.read(path)?.ok()
    }

    fn game_string(&self, name: &str) -> Option<String> {
        self.strings.get(&name.to_ascii_lowercase()).cloned()
    }
}

#[derive(Default)]
pub struct LoadReport {
    pub menus: usize,
    pub tiles: usize,
    /// One entry per file; repeated reads of an include report its mends once [design].
    pub warnings: BTreeMap<String, Vec<String>>,
}

impl LoadReport {
    pub fn warning_count(&self) -> usize {
        self.warnings.values().map(Vec::len).sum()
    }
}

pub fn load(game: &mut Game<'_>, files: Box<dyn Source + Send + Sync>, paths: impl IntoIterator<Item = String>) -> LoadReport {
    let mut ui = Ui::new();
    if !ui.load_globals(files.as_ref()) {
        ui.warnings.push("menus\\globals.xml: not found".into());
    }
    let index = MenuIndex::build(files.as_ref(), paths);
    ui.warnings.extend(index.warnings.iter().cloned());
    let mut report = LoadReport::default();
    for name in ALWAYS_LOADED {
        let Some(path) = index.path(name) else {
            ui.warnings.push(format!("{name}: menu not found"));
            continue;
        };
        match ui.load_menu(files.as_ref(), path) {
            Ok(root) => {
                report.menus += 1;
                let mut stack = vec![root];
                while let Some(id) = stack.pop() {
                    report.tiles += 1;
                    stack.extend(ui.tile(id).children.iter().copied());
                }
            }
            Err(e) => ui.warnings.push(e),
        }
    }
    ui.update();
    for warning in &ui.warnings {
        let (file, message) = warning.split_once(": ").unwrap_or(("tree", warning));
        let file = if file.to_ascii_lowercase().ends_with(".xml") { file.replace('/', "\\").to_ascii_lowercase() } else { file.into() };
        let messages = report.warnings.entry(file).or_default();
        if !messages.iter().any(|m| m == message) {
            messages.push(message.into());
        }
    }
    game.ui_tree = ui;
    game.ui_menus = index;
    game.ui_files = Some(files);
    game.ui_lifetime = MenuLifetime { unavailable: BTreeSet::new(), warnings: game.ui_tree.warnings.iter().map(|w| warning_key(w)).collect() };
    report
}
