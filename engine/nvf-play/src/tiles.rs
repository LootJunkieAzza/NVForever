//! The tile tree beside the drawn menus [design: ui-tile-tree.md §6.2 step 1].

use crate::{script::ScriptState, PlaySet, PlayState};
use bevy::{ecs::system::SystemParam, prelude::*, window::PrimaryWindow};
use fnv_game::ui_loading::{self, UiFiles};
use fnv_ui::Value;
use nvf_assets::GameData;

#[derive(SystemParam)]
pub(crate) struct MenuFiles<'w, 's> {
    data: Option<Res<'w, GameData>>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    paused: Option<Res<'w, crate::pause::Paused>>,
    main_menu: Option<Res<'w, crate::main_menu::MainMenu>>,
}

impl MenuFiles<'_, '_> {
    pub(crate) fn load(&self, game: &mut fnv_game::Game<'_>, world: &fnv_sim::World<'_>) {
        let Some(data) = self.data.as_ref() else { return };
        let files = UiFiles::new(data.vfs.clone(), world);
        let paths = files.menu_paths();
        let report = ui_loading::load(game, Box::new(files), paths);
        self.write(game);
        game.ui_tree.update();
        for (file, messages) in &report.warnings {
            warn!("ui: {file}: {}", messages.join("; "));
        }
        info!("ui: {} menus loaded ({} tiles), {} warnings", report.menus, report.tiles, report.warning_count());
    }

    pub(crate) fn write(&self, game: &mut fnv_game::Game<'_>) {
        if game.ui_files.is_none() {
            return;
        }
        if let Ok(window) = self.windows.single() {
            screen(game, window);
        }
        let report = ui_loading::sync(game, self.paused.as_ref().is_some_and(|p| p.0), self.main_menu.as_ref().is_some_and(|m| m.open));
        for line in report.changes {
            info!("ui: {line}");
        }
        for line in report.warnings {
            warn!("ui: {line}");
        }
    }
}

fn screen(game: &mut fnv_game::Game<'_>, window: &Window) {
    // Window dimensions, as fnvtool's --screen [hypothesis: menu-xml.md §4.2, screen() row].
    // cropx, cropy and resolutionconverter stay unset until their units are known [design].
    let id = game.ui_tree.screen();
    game.ui_tree.set_trait(id, "width", Value::Num(f64::from(window.width())));
    game.ui_tree.set_trait(id, "height", Value::Num(f64::from(window.height())));
}

/// Read the same inputs as the current HUD, before scripts [design]. Timed pieces
/// use the current target/queue/meter state; their clocks remain with the drawing.
#[derive(SystemParam)]
struct HudTiles<'w> {
    state: Res<'w, State<PlayState>>,
    sim: Res<'w, crate::sim::SimState>,
    control: Option<Res<'w, crate::control::PlayerControl>>,
    locks: Option<Res<'w, crate::control::ControlLocks>>,
    driven: Option<Res<'w, crate::player::ViewDriven>>,
    wheel: Option<Res<'w, crate::weapon_wheel::WeaponWheel>>,
    creator: Option<Res<'w, crate::creator::CreatorMenu>>,
    sights: Option<Res<'w, crate::aim_zoom::TrueSights>>,
    scope: Option<Res<'w, crate::aim_zoom::Scope>>,
    combat: Option<Res<'w, crate::combat::Combat>>,
    target: Option<Res<'w, crate::activate::Target>>,
    enemy: Option<Res<'w, crate::hud_target::HudTarget>>,
    limbs: Option<Res<'w, crate::hud_limbs::Indicator>>,
    player_limbs: Option<Res<'w, crate::combat::PlayerLimbs>>,
    breath: Option<Res<'w, crate::drowning::PlayerBreath>>,
    xp: Option<Res<'w, crate::xp_meter::Meter>>,
    corner: Option<Res<'w, crate::corner_messages::CornerMessages>>,
    quests: Option<Res<'w, crate::quest_messages::QuestMessages>>,
    hud: Option<Res<'w, crate::script_hud::ScriptHud>>,
    player: Option<Res<'w, crate::player::Player>>,
    panel: Option<Res<'w, crate::message_menu::MessagePanel>>,
    loading: Option<Res<'w, crate::loading::Loading>>,
}

impl HudTiles<'_> {
    fn shown(&self, game: &fnv_game::Game<'_>, files: &MenuFiles<'_, '_>) -> fnv_game::ui_hud::Shown {
        let playing = *self.state.get() == PlayState::Playing;
        let root = playing
            && !game.in_menu()
            && !files.paused.as_ref().is_some_and(|p| p.0)
            && !files.main_menu.as_ref().is_some_and(|m| m.open)
            && !self.loading.as_ref().is_some_and(|l| l.active());
        let driven = self.driven.as_ref().is_some_and(|d| d.0);
        let wheel = self.wheel.as_ref().is_some_and(|w| w.is_open());
        let creating = self.creator.as_ref().is_some_and(|c| c.is_open());
        let locks = self.locks.as_deref().copied().unwrap_or_default();
        // hud::gather and hud::{draw,texts}: the opening's movement lock, portrait,
        // held camera and weapon wheel hide the corner HUD and its meters [design].
        let corners = playing && !locks.movement && !self.control.as_ref().is_some_and(|c| c.portrait.is_some()) && !driven && !wheel;
        let weapon = self.combat.as_ref().map(|c| c.hud_weapon(&self.sim)).unwrap_or_default();
        let enemy = self.enemy.as_ref().and_then(|e| e.0.as_ref());
        // reticle::draw: the prompt also goes under true sights/the scope, and when
        // the enemy bar names this target; only the crosshair obeys movement [design].
        let reticle = playing && !creating && !driven && !wheel && !self.sights.as_ref().is_some_and(|s| s.0) && !self.scope.as_ref().is_some_and(|s| s.on);
        let info = reticle && !locks.rollover && self.target.as_ref().and_then(|t| t.0.as_ref()).is_some_and(|t| !enemy.is_some_and(|e| crate::reticle::under_bar(t, e.form_id)));
        // corner_messages::draw and quest_messages::draw [design].
        let notices = !driven && *self.state.get() != PlayState::Dialogue && !self.panel.as_ref().is_some_and(|p| p.view.is_some());
        let quest = self.quests.as_ref().and_then(|q| q.showing());
        // Map discovery to Region_Location [hypothesis: hud.md §14; one queue here].
        let location = matches!(quest, Some(crate::script_events::QuestNotice::Discovered(_)));
        // hud_needs::watch/draw: H2O/FOD/SLP in Hardcore, RAD and LMB in every mode.
        // The rect shows if any word has a look other than Hidden [design].
        use fnv_game::aid::{DEHYDRATION, HUNGER, RAD_LEVEL, SLEEP_DEPRIVATION};
        let need = |code| game.player.need_stages.get(&code).is_some_and(|held| game.data.need_stages.get(&code).is_some_and(|stages| stages.iter().any(|s| s.stage == *held)));
        // needs::hardcore with the live world's config, as hud_needs::watch [design].
        let hardcore = game.player.hardcore.unwrap_or(self.sim.world.config().hardcore);
        // Only the six limbs count, as hud_needs::watch (the brain is excluded) [design].
        let needs = need(RAD_LEVEL)
            || (hardcore && [DEHYDRATION, HUNGER, SLEEP_DEPRIVATION].into_iter().any(need))
            || self.player_limbs.as_ref().is_some_and(|l| (25..=30).any(|code| l.crippled(code)));
        fnv_game::ui_hud::Shown {
            root,
            corners,
            sneak: corners && self.control.as_ref().is_some_and(|c| c.sneak),
            reticle: reticle && !locks.movement,
            enemy: corners && enemy.is_some(),
            quests: notices && quest.is_some() && !location,
            messages: notices && self.corner.as_ref().is_some_and(|c| c.showing().is_some()),
            subtitles: !creating && self.hud.as_ref().is_some_and(|h| crate::script_hud::hud_subtitles_shown(h, game, &self.sim, self.player.as_deref())),
            info,
            location: notices && location,
            xp: corners && self.xp.as_ref().is_some_and(|m| m.show.is_some()), // xp_meter::draw [design].
            radiation: corners && game.radiation().rate > 0.0,                 // radiation::draw [design].
            breath: corners && self.breath.as_ref().is_some_and(|b| crate::hud_breath::shown(b)),
            limbs: corners && self.limbs.as_ref().is_some_and(|l| l.shown()), // hud_limbs::draw [design].
            needs: corners && needs,
            ammo_label: corners && weapon.ammo.is_some() && weapon.abbrev.as_ref().is_some_and(|s| !s.is_empty()),
            condition: corners && weapon.condition.is_some(),
        }
    }
}

fn write(mut script: ResMut<ScriptState>, files: MenuFiles, hud: HudTiles) {
    let Some(rt) = script.rt.as_mut() else { return };
    files.write(&mut rt.game);
    let shown = hud.shown(&rt.game, &files);
    fnv_game::ui_hud::write(&mut rt.game.ui_tree, shown);
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
        app.add_systems(Update, write.after(PlaySet::Physics).before(PlaySet::Activation)).add_systems(Update, update.after(PlaySet::Scripts).before(PlaySet::Ui));
    }
}
