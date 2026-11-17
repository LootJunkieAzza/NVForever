//! The settings page written beside `NVForever.ini` for the launcher
//! (`nvf_settings_layout::menu`, [`write_menu`]): every tab, category and row as this run's
//! page lays them out (the registry through the table, `crate::settings_layout`), with each
//! row's label and note as the page shows them and how it changes (switch, slider and the
//! words of its steps, list and its choices) [design: owner 2026-11-16: the launcher's
//! settings must always be the game's, so it never names a menu the game does not have].
//!
//! Written once every row is registered (`PostStartup`, after `crate::settings::check_notes`),
//! only when it differs from the file there; the log says
//! `settings: menu for the launcher written: <path> (<tabs> tabs, <rows> rows)` (`unchanged`
//! when the file there is the same). A run with no
//! settings file (`SettingsFile(None)`) writes none.

use bevy::prelude::*;
use fnv_util::words::tr;
use nvf_settings_layout::menu::{self, Menu};

use crate::pause::on_page;
use crate::settings::{Settings, SettingsFile, Slider};
use crate::settings_layout::resolve_layout;

/// The most steps a slider's words are listed for (the field of view's are some 100).
const MOST_STEPS: usize = 1000;

/// The page as `settings` lays it out in `world` (the Controls tab's own rows left out: they
/// are not `[Settings]` rows).
pub fn menu_of(world: &World, settings: &Settings) -> Menu {
    let layout = resolve_layout(settings.list());
    let mut tabs = Vec::new();
    for t in &layout.tabs {
        let categories: Vec<menu::Category> = t
            .sections
            .iter()
            .filter(|s| !s.rows.is_empty())
            .map(|s| menu::Category {
                group: s.placement.subgroup.map(on_page),
                heading: s.placement.heading.map(on_page),
                rows: s.rows.iter().map(|&i| row(world, settings, i)).collect(),
            })
            .collect();
        if !categories.is_empty() {
            tabs.push(menu::Tab { name: t.name.to_string(), label: on_page(t.name), categories });
        }
    }
    Menu { version: env!("CARGO_PKG_VERSION").to_string(), tabs }
}

fn row(world: &World, settings: &Settings, i: usize) -> menu::Row {
    let s = &settings.list()[i];
    let kind = if let Some(list) = settings.choices(i) {
        // A choice that does something instead of being kept (Download DLSS… on the
        // Upscaler row) is not a value: the INI never holds it.
        let action = settings.choice_looks(i).and_then(|l| l.action);
        let choices = (list.names)(world)
            .into_iter()
            .filter(|name| !action.is_some_and(|is_action| is_action(name)))
            .map(|name| {
                let label = on_page(&settings.choice_label(world, i, &name));
                menu::Choice { name, label }
            })
            .collect();
        menu::Kind::List { choices }
    } else if let Some(slider) = s.slider {
        menu::Kind::Slider { min: slider.min, step: slider.step, steps: steps(&slider) }
    } else {
        menu::Kind::Switch
    };
    // What it needs of other rows (the shared tables the page goes by), its reason translated.
    let words = |w: &'static str| tr(w).to_string();
    let shown_with = nvf_settings_layout::shown_with(s.key).map(|r| menu::Rule::of(r, words));
    let needs = nvf_settings_layout::needs(s.key).into_iter().map(|r| menu::Rule::of(r, words)).collect();
    menu::Row { key: s.key.to_string(), label: on_page(s.label), note: tr(s.note).to_string(), kind, shown_with, needs }
}

/// The values `slider` reaches (`Slider::fit`) and the words the page shows for each, when
/// it shows words (none for a percentage, or past [`MOST_STEPS`]).
fn steps(slider: &Slider) -> Vec<menu::Step> {
    if slider.shown.is_none() {
        return Vec::new();
    }
    let step = slider.step.max(1e-3);
    let (first, last) = ((slider.min / step - 1e-4).ceil() as i64, (1.0 / step).round() as i64);
    if last - first + 1 > MOST_STEPS as i64 {
        return Vec::new();
    }
    let mut out: Vec<menu::Step> = Vec::new();
    for k in first..=last {
        let value = slider.fit(k as f32 * step);
        if out.last().is_none_or(|s| s.value != value) {
            out.push(menu::Step { value, label: on_page(&slider.text(value)) });
        }
    }
    out
}

/// Write the page beside the settings file when it differs from the one there.
pub fn write_menu(world: &World) {
    let Some(path) = world.get_resource::<SettingsFile>().and_then(|f| f.0.clone()) else { return };
    let Some(settings) = world.get_resource::<Settings>() else { return };
    let m = menu_of(world, settings);
    let text = m.to_text();
    let file = path.with_file_name(menu::MENU_FILE);
    let (tabs, rows) = (m.tabs.len(), m.rows().count());
    if std::fs::read_to_string(&file).is_ok_and(|t| t == text) {
        info!("settings: menu for the launcher unchanged: {} ({tabs} tabs, {rows} rows)", file.display());
        return;
    }
    // Through the same gatekeeper as the INI beside it (`settings::write`): never into the
    // game folder or the original's user folder, whatever the home points at.
    if let Err(e) = fnv_install::guard_write(&file, None) {
        warn!("settings: menu for the launcher not written: {e}");
        return;
    }
    match std::fs::write(&file, &text) {
        Ok(()) => info!("settings: menu for the launcher written: {} ({tabs} tabs, {rows} rows)", file.display()),
        Err(e) => warn!("settings: menu for the launcher not written: {}: {e}", file.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{add_choice_list, add_setting, set_choice_action, ChoiceList, Setting};

    #[derive(Resource, Default)]
    struct Part(f32);

    fn setting(key: &'static str, group: &'static str, label: &'static str) -> Setting {
        Setting { key, group, label, note: "What it does.", switches: "", get: |_| false, set: |_, _| {}, slider: None }
    }

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<Part>();
        add_setting(&mut app, setting("hardcore", "Gameplay", "Hardcore mode"));
        let words = |v: f32| ["Very easy", "Easy", "Normal", "Hard", "Very hard"][(v * 4.0).round() as usize].to_string();
        add_setting(&mut app, Setting { slider: Some(Slider::choice(5, |w| w.resource::<Part>().0, |w, v| w.resource_mut::<Part>().0 = v, words)), ..setting("difficulty", "Gameplay", "Difficulty") });
        add_setting(&mut app, Setting { slider: Some(Slider { get: |_| 1.0, set: |_, _| {}, min: 0.1, step: 0.05, shown: None }), ..setting("volume-master", "Audio", "Master volume") });
        let list = ChoiceList { names: |_| vec!["Classic".into(), "Mark and Fire".into()], chosen: |_| "Classic".into(), pick: |_, _| true, save: |_| "Classic".into(), load: |_, _| true, ask_keep: false };
        add_choice_list(&mut app, setting("vats-style", "Gameplay", "V.A.T.S. style"), list);
        let upscaler = ChoiceList { names: |_| vec!["Off".into(), "DLSS".into(), "Download DLSS...".into()], chosen: |_| "Off".into(), pick: |_, _| true, save: |_| "Off".into(), load: |_, _| true, ask_keep: false };
        add_choice_list(&mut app, setting("upscaler", "Display", "Upscaler"), upscaler);
        set_choice_action(&mut app, "upscaler", |name| name.starts_with("Download"));
        add_setting(&mut app, setting("a-later-row", "Gameplay", "A later row"));
        add_setting(&mut app, setting("vats-slowdown", "Gameplay", "Time slowdown"));
        app
    }

    #[test]
    fn the_menu_is_the_pages_tabs_categories_and_rows() {
        let app = app();
        let w = app.world();
        let m = menu_of(w, w.resource::<Settings>());
        assert_eq!(m.version, env!("CARGO_PKG_VERSION"));
        let tabs: Vec<&str> = m.tabs.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(tabs, ["Gameplay", "Display", "Audio"], "in the table's order, a tab with no rows left out");
        let g = &m.tabs[0];
        let heads: Vec<Option<&str>> = g.categories.iter().map(|c| c.heading.as_deref()).collect();
        assert_eq!(heads, [Some("Game"), Some("V.A.T.S."), None], "the table's categories, then the row it does not place");
        assert_eq!(g.categories[0].rows[1].label, "Hardcore Mode", "as the page shows it");
        assert_eq!(g.categories[0].rows[1].kind, menu::Kind::Switch);
        let diff = &g.categories[0].rows[0];
        assert_eq!(diff.key, "difficulty");
        let menu::Kind::Slider { min, step, steps } = &diff.kind else { panic!("{diff:?}") };
        assert_eq!((*min, *step), (0.0, 0.25));
        assert_eq!(steps.iter().map(|s| (s.value, s.label.as_str())).collect::<Vec<_>>(), [(0.0, "Very Easy"), (0.25, "Easy"), (0.5, "Normal"), (0.75, "Hard"), (1.0, "Very Hard")]);
        let menu::Kind::List { choices } = &g.categories[1].rows[0].kind else { panic!() };
        // The rules the page goes by, for the launcher (the shared tables).
        let slowdown = &g.categories[1].rows[1];
        assert_eq!(slowdown.key, "vats-slowdown");
        assert_eq!(slowdown.needs, vec![menu::Rule { parent: "vats-style".into(), choices: vec!["Classic".into()], why: "Classic V.A.T.S. only.".into() }]);
        assert!(g.categories[1].rows[0].needs.is_empty() && g.categories[1].rows[0].shown_with.is_none());
        let upscaler = &m.tabs[1].categories[0].rows[0];
        assert_eq!(upscaler.key, "upscaler");
        assert_eq!(choices.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Classic", "Mark and Fire"]);
        let menu::Kind::List { choices } = &m.tabs[1].categories[0].rows[0].kind else { panic!() };
        assert_eq!(choices.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Off", "DLSS"], "an action is not a value the INI keeps");
        let volume = &m.tabs[2].categories[0].rows[0];
        assert_eq!(volume.kind, menu::Kind::Slider { min: 0.1, step: 0.05, steps: Vec::new() }, "a percentage: no words");
    }

    #[test]
    fn written_beside_the_ini_only_when_it_changed() {
        let dir = std::env::temp_dir().join(format!("nvf-settings-menu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = app();
        app.insert_resource(SettingsFile(Some(dir.join("NVForever.ini"))));
        let file = dir.join(menu::MENU_FILE);
        write_menu(app.world());
        let text = std::fs::read_to_string(&file).unwrap();
        assert_eq!(Menu::parse(&text).map(|m| m.rows().count()), Some(7));
        std::fs::write(&file, "old").unwrap();
        write_menu(app.world());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text, "an old file is replaced");
        app.insert_resource(SettingsFile(None));
        std::fs::remove_file(&file).unwrap();
        write_menu(app.world());
        assert!(!file.exists(), "no settings file: no menu");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
