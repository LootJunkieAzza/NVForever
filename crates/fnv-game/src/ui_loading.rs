//! Files and startup loading for the tile tree [design: ui-tile-tree.md §3, §9].

use crate::Game;
use fnv_sim::{settings::Setting, World};
use fnv_ui::{MenuIndex, Source, Ui};
use fnv_vfs::Vfs;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Menus kept for the run [documented: InjectUIXML notes, ui-tile-tree.md §3.3].
pub const ALWAYS_LOADED: &[&str] = &["HUDMainMenu", "InventoryMenu", "MapMenu", "RepairMenu", "StatsMenu", "ItemModMenu"];

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
    report
}
