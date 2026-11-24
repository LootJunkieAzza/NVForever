//! The live settings window (F11): every knob the network takes, applied without a restart,
//! as the unofficial DLSS 5 builds (RenoDX, Neural Forge) expose them. The values persist in
//! `nr-settings.ini` in the engine home; the environment variables are the start-up defaults.

use bevy::camera::{Camera, RenderTarget};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::input::{EguiContextImeState, EguiContextPointerPosition, EguiContextPointerTouchId};
use bevy_egui::{
    egui, EguiContext, EguiContextSettings, EguiContexts, EguiFullOutput, EguiGlobalSettings, EguiInput, EguiOutput, EguiPlugin, EguiRenderOutput, EguiSchedule,
    EguiZoomFactor, PrimaryEguiContext,
};

/// The network's settings, main world; extracted to the render world every frame.
#[derive(Resource, Clone, Debug)]
pub struct NrSettings {
    pub enabled: bool,
    pub show: bool,
    /// 0 Reference, 1 Natural, 2 Cinematic (conditioning + the composite's grade).
    pub style: f32,
    pub intensity: f32,
    pub tone: f32,
    pub structure: f32,
    pub skin: f32,
    pub automask: bool,
    pub white: f32,
    pub network: String,
}

impl Default for NrSettings {
    fn default() -> Self {
        let mut s = Self {
            enabled: *super::ALLOWED,
            show: false,
            style: *super::STYLE,
            intensity: *super::INTENSITY,
            tone: *super::TONE,
            structure: *super::STRUCTURE,
            skin: *super::SKIN,
            automask: *super::AUTOMASK,
            white: *super::WHITE,
            network: super::NETWORK.clone(),
        };
        if let Some(home) = fnv_install::EngineHome::user() {
            if let Ok(text) = std::fs::read_to_string(home.root.join("nr-settings.ini")) {
                for line in text.lines() {
                    let Some((k, v)) = line.split_once('=') else { continue };
                    let v = v.trim();
                    match k.trim() {
                        "enabled" => s.enabled = v == "1",
                        "style" => s.style = v.parse().unwrap_or(s.style),
                        "intensity" => s.intensity = v.parse().unwrap_or(s.intensity),
                        "tone" => s.tone = v.parse().unwrap_or(s.tone),
                        "structure" => s.structure = v.parse().unwrap_or(s.structure),
                        "skin" => s.skin = v.parse().unwrap_or(s.skin),
                        "automask" => s.automask = v == "1",
                        "white" => s.white = v.parse().unwrap_or(s.white),
                        "network" => s.network = v.to_string(),
                        _ => {}
                    }
                }
            }
        }
        s
    }
}

impl NrSettings {
    fn save(&self) {
        let Some(home) = fnv_install::EngineHome::user() else { return };
        let text = format!(
            "enabled={}\nstyle={}\nintensity={}\ntone={}\nstructure={}\nskin={}\nautomask={}\nwhite={}\nnetwork={}\n",
            u8::from(self.enabled), self.style, self.intensity, self.tone, self.structure, self.skin, u8::from(self.automask), self.white, self.network
        );
        let _ = std::fs::write(home.root.join("nr-settings.ini"), text);
    }
}

/// F11 opens and closes the window; the settings save themselves (at most once a second).
pub struct NrOverlayPlugin;

impl Plugin for NrOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NrSettings>()
            .add_plugins(EguiPlugin::default())
            // bevy_egui's auto-attach puts the context on the FIRST camera ever spawned; a
            // handoff run (save load / back to menu) spawns its loading cover first, and the
            // context would stay on that inactive camera forever. Place it ourselves instead.
            .insert_resource(EguiGlobalSettings { auto_create_primary_context: false, ..default() })
            .add_systems(
                PreUpdate,
                input_gate
                    .after(bevy::input::InputSystems)
                    .after(bevy_egui::EguiPreUpdateSet::ProcessInput),
            )
            .add_systems(Update, (place_primary_context, persist).chain())
            .add_systems(Last, cursor_while_open)
            // The window draws inside the egui pass: adding a system to this schedule is also
            // what creates it (the context loop runs it per frame).
            .add_systems(bevy_egui::EguiPrimaryContextPass, draw);
    }
}

/// F11 opens and closes the window. While it is open the game's own input is swallowed (keys,
/// mouse buttons, look motion, scroll) after egui has read this frame's input, so the sliders
/// can be dragged without the camera or the weapon reacting. Runs in PreUpdate, before the
/// game's Update systems read the input.
fn input_gate(
    mut s: ResMut<NrSettings>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut buttons: ResMut<ButtonInput<MouseButton>>,
    mut motion: ResMut<bevy::input::mouse::AccumulatedMouseMotion>,
    mut scroll: ResMut<bevy::input::mouse::AccumulatedMouseScroll>,
) {
    if keys.just_pressed(KeyCode::F11) {
        s.show = !s.show;
        info!("NR overlay: F11 pressed, show={}", s.show);
    }
    if s.show {
        keys.clear();
        buttons.clear();
        motion.delta = Vec2::ZERO;
        scroll.delta = Vec2::ZERO;
    }
}

/// The pointer is free and visible while the window is open; closed, the engine's own cursor
/// state is left as it was (the main menu needs its pointer too).
fn cursor_while_open(s: Res<NrSettings>, mut cursors: Query<&mut bevy::window::CursorOptions, With<PrimaryWindow>>) {
    if !s.show {
        return;
    }
    for mut cursor in &mut cursors {
        cursor.visible = true;
        cursor.grab_mode = bevy::window::CursorGrabMode::None;
    }
}

/// Keeps the primary egui context on the active camera that draws the primary window, moving it
/// when the engine switches cameras (loading cover -> game, game -> menu).
fn place_primary_context(
    mut commands: Commands,
    windows: Query<Entity, With<PrimaryWindow>>,
    cameras: Query<(Entity, &Camera, &RenderTarget)>,
    existing: Query<Entity, With<EguiContext>>,
) {
    let Ok(win) = windows.single() else { return };
    let on_window = |cam: &Camera, rt: &RenderTarget| {
        cam.is_active
            && match rt {
                RenderTarget::Window(bevy::window::WindowRef::Primary) => true,
                RenderTarget::Window(bevy::window::WindowRef::Entity(w)) => *w == win,
                _ => false,
            }
    };
    if let Some(old) = existing.iter().next() {
        if cameras.get(old).is_ok_and(|(_, cam, rt)| on_window(cam, rt)) {
            return;
        }
        commands.entity(old).remove::<(
            EguiContext,
            PrimaryEguiContext,
            EguiSchedule,
            EguiContextSettings,
            EguiInput,
            EguiZoomFactor,
            EguiContextPointerPosition,
            EguiContextPointerTouchId,
            EguiContextImeState,
            EguiFullOutput,
            EguiRenderOutput,
            EguiOutput,
        )>();
    }
    if let Some(target) = cameras.iter().find(|(_, cam, rt)| on_window(cam, rt)).map(|(e, _, _)| e) {
        commands.entity(target).insert((EguiContext::default(), PrimaryEguiContext));
    }
}

fn draw(mut contexts: EguiContexts, mut s: ResMut<NrSettings>, mut warned: Local<bool>) {
    if !s.show {
        return;
    }
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(e) => {
            if !*warned {
                warn!("NR overlay: no egui context: {e:?}");
                *warned = true;
            }
            return;
        }
    };
    egui::Window::new("DLSS 5 — Neural Rendering")
        .resizable(true)
        .default_width(360.0)
        .show(ctx, |ui| {
            ui.checkbox(&mut s.enabled, "Enable neural rendering");
            ui.horizontal(|ui| {
                ui.label("Network:");
                egui::ComboBox::from_id_salt("nr_network")
                    .selected_text(if s.network == "full" { "full (71 blocks)" } else { "small (31 blocks)" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut s.network, "small".to_string(), "small (31 blocks, ~2x faster)");
                        ui.selectable_value(&mut s.network, "full".to_string(), "full (71 blocks, finest)");
                    });
            });
            ui.separator();
            ui.label("Style preset:");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut s.style, 0.0, "Reference");
                ui.selectable_value(&mut s.style, 1.0, "Natural");
                ui.selectable_value(&mut s.style, 2.0, "Cinematic");
            });
            ui.add(egui::Slider::new(&mut s.intensity, 0.0..=1.0).text("Intensity"));
            ui.add(egui::Slider::new(&mut s.tone, 0.0..=4.0).text("localTone"));
            ui.add(egui::Slider::new(&mut s.structure, 0.0..=4.0).text("localStructure"));
            ui.add(egui::Slider::new(&mut s.skin, -1.0..=4.0).text("skinStructure (-1 = follow)"));
            ui.checkbox(&mut s.automask, "autoMask (the network's own masking)");
            ui.add(egui::Slider::new(&mut s.white, 0.1..=8.0).text("Paper white").logarithmic(true));
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Reference preset").clicked() {
                    s.style = 0.0; s.intensity = 1.0; s.tone = 1.0; s.structure = 1.0; s.skin = -1.0; s.automask = true;
                }
                if ui.button("Natural preset").clicked() {
                    s.style = 1.0; s.intensity = 0.8; s.tone = 1.0; s.structure = 1.0; s.skin = -1.0; s.automask = true;
                }
                if ui.button("Cinematic preset").clicked() {
                    s.style = 2.0; s.intensity = 0.7; s.tone = 1.0; s.structure = 1.0; s.skin = -1.0; s.automask = true;
                }
            });
            ui.separator();
            let active = super::NR_ACTIVE.load(std::sync::atomic::Ordering::Relaxed);
            ui.label(if active { "Status: drawing" } else { "Status: idle (needs the DLSS upscaler on)" });
            ui.label("F11 — close. Changes apply live and save automatically.");
            ui.label("Boiling in dark areas is the single-frame mode; temporal history is the next step.");
        });
}

fn persist(s: Res<NrSettings>, mut last: Local<Option<std::time::Instant>>) {
    if !s.is_changed() {
        return;
    }
    let now = std::time::Instant::now();
    if last.is_some_and(|t| now.duration_since(t) < std::time::Duration::from_secs(1)) {
        return;
    }
    s.save();
    *last = Some(now);
}
