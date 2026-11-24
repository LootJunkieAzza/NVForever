//! The live settings window (F11): every knob the network takes, applied without a restart,
//! as the unofficial DLSS 5 builds (RenoDX, Neural Forge) expose them. The values persist in
//! `nr-settings.ini` in the engine home; the environment variables are the start-up defaults.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};

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
            .add_systems(Update, (toggle_key, persist).chain())
            // The window draws inside the egui pass: adding a system to this schedule is also
            // what creates it (the context loop runs it per frame).
            .add_systems(bevy_egui::EguiPrimaryContextPass, draw);
    }

    // EguiPlugin needs the asset resources DefaultPlugins builds; this plugin is added before
    // them (with NrInitPlugin), so the egui plugin joins at finish, after every build ran.
    fn finish(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default());
    }
}

fn toggle_key(keys: Res<ButtonInput<KeyCode>>, mut s: ResMut<NrSettings>) {
    if keys.just_pressed(KeyCode::F11) {
        s.show = !s.show;
    }
}

fn draw(mut contexts: EguiContexts, mut s: ResMut<NrSettings>) {
    if !s.show {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
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
