//! The tile tree beside the drawn menus [design: ui-tile-tree.md §6.2 step 1].

use crate::{script::ScriptState, PlaySet};
use bevy::{ecs::system::SystemParam, prelude::*, window::PrimaryWindow};
use fnv_game::ui_loading::{self, UiFiles};
use fnv_ui::Value;
use nvf_assets::GameData;

#[derive(SystemParam)]
pub(crate) struct MenuFiles<'w, 's> {
    data: Option<Res<'w, GameData>>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
}

impl MenuFiles<'_, '_> {
    pub(crate) fn load(&self, game: &mut fnv_game::Game<'_>, world: &fnv_sim::World<'_>) {
        let Some(data) = self.data.as_ref() else { return };
        let files = UiFiles::new(data.vfs.clone(), world);
        let paths = files.menu_paths();
        let report = ui_loading::load(game, Box::new(files), paths);
        if let Ok(window) = self.windows.single() {
            screen(game, window);
            game.ui_tree.update();
        }
        for (file, messages) in &report.warnings {
            warn!("ui: {file}: {}", messages.join("; "));
        }
        info!("ui: {} menus loaded ({} tiles), {} warnings", report.menus, report.tiles, report.warning_count());
    }
}

fn screen(game: &mut fnv_game::Game<'_>, window: &Window) {
    // Window dimensions, as fnvtool's --screen [hypothesis: menu-xml.md §4.2, screen() row].
    // cropx, cropy and resolutionconverter stay unset until their units are known [design].
    let id = game.ui_tree.screen();
    game.ui_tree.set_trait(id, "width", Value::Num(f64::from(window.width())));
    game.ui_tree.set_trait(id, "height", Value::Num(f64::from(window.height())));
}

fn write_screen(mut script: ResMut<ScriptState>, windows: Query<&Window, With<PrimaryWindow>>) {
    let Some(rt) = script.rt.as_mut() else { return };
    if rt.game.ui_files.is_none() {
        return;
    }
    if let Ok(window) = windows.single() {
        screen(&mut rt.game, window);
    }
}

fn update(mut script: ResMut<ScriptState>) {
    let Some(rt) = script.rt.as_mut() else { return };
    if rt.game.ui_files.is_some() {
        rt.game.ui_tree.update();
    }
    // Leave ui_taps queued: tile input is task T8 [design].
}

pub struct TilesPlugin;

impl Plugin for TilesPlugin {
    fn build(&self, app: &mut App) {
        // Writes run after physics, before activation's scripts and PlaySet::Scripts;
        // evaluation runs after PlaySet::Scripts, before the menus draw [design: ui-tile-tree.md §4.1].
        app.add_systems(Update, write_screen.after(PlaySet::Physics).before(PlaySet::Activation))
            .add_systems(Update, update.after(PlaySet::Scripts).before(PlaySet::Ui));
    }
}
