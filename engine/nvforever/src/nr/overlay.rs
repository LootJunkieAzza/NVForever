//! The live settings window (F11): every knob the network takes, applied without a restart,
//! as the unofficial DLSS 5 builds (RenoDX, Neural Forge) expose them, and a Shadows tab for
//! the sun contact shadows (sun_contact_shadows.rs). The values persist in `nr-settings.ini`
//! and `sun-shadows.ini` in the engine home; the environment variables are the start-up
//! defaults.

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

fn draw(
    mut contexts: EguiContexts,
    mut s: ResMut<NrSettings>,
    mut graphics: ResMut<crate::graphics::Graphics>,
    mut tuning: ResMut<crate::sun_contact_shadows::SunContactTuning>,
    mut parallax: ResMut<crate::terrain::ParallaxTuning>,
    mut water: ResMut<crate::water::WaterTuning>,
    mut outdoor_lighting: ResMut<crate::sky::OutdoorLighting>,
    mut grass_density: ResMut<crate::grass::GrassDensity>,
    mut warned: Local<bool>,
    mut tab: Local<usize>,
) {
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
    // The switches are `Graphics` fields, and other systems watch that resource: read them
    // without touching the resource's change mark, write only when a box changed.
    let mut shadow_on = graphics.sun_contact_shadows;
    let mut parallax_on = graphics.terrain_parallax;
    egui::Window::new("NVForever — live settings")
        .resizable(true)
        .default_width(380.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut *tab, 0, "DLSS 5");
                ui.selectable_value(&mut *tab, 1, "Shadows");
                ui.selectable_value(&mut *tab, 2, "Parallax");
                ui.selectable_value(&mut *tab, 3, "Water");
                ui.selectable_value(&mut *tab, 4, "Lighting");
                ui.selectable_value(&mut *tab, 5, "Grass");
            });
            ui.separator();
            match *tab {
                0 => nr_tab(ui, &mut s),
                1 => shadows_tab(ui, &mut shadow_on, &mut tuning),
                2 => parallax_tab(ui, &mut parallax_on, &mut parallax),
                3 => water_tab(ui, &mut water),
                4 => lighting_tab(ui, &mut outdoor_lighting),
                _ => grass_tab(ui, &mut grass_density),
            }
        });
    if shadow_on != graphics.sun_contact_shadows {
        graphics.sun_contact_shadows = shadow_on;
    }
    if parallax_on != graphics.terrain_parallax {
        graphics.terrain_parallax = parallax_on;
    }
}

fn grass_tab(ui: &mut egui::Ui, density: &mut crate::grass::GrassDensity) {
    let slider = ui.add(egui::Slider::new(&mut density.multiplier, 0.5..=3.0).text("Grass density").suffix("×").fixed_decimals(2));
    let distance = ui.add(egui::Slider::new(&mut density.distance, 0.25..=4.0).text("Grass distance multiplier").suffix("x"));
    let lod = ui.checkbox(&mut density.lod, "Experimental LOD: textured crossed planes");
    let lod_start = ui.add(egui::Slider::new(&mut density.lod_start, 500.0..=20000.0).text("LOD start (game units)"));
    let reset = ui.button("Reset to default").clicked();
    if reset {
        density.multiplier = 1.0;
        density.distance = 1.0;
        density.lod = false;
        density.lod_start = 4000.0;
    }
    if reset || lod.changed() || [&slider, &distance, &lod_start].iter().any(|r| r.drag_stopped() || (r.changed() && !r.dragged())) {
        density.save();
    }
    ui.label("Density applies after restarting. Distance and LOD apply live. Distance is limited by loaded cells.");
    ui.label("LOD switches whole grass chunks with hysteresis; visible popping is expected in this prototype.");
}

/// The network's tab: the switches of `nr/mod.rs`, the presets and the status.
fn nr_tab(ui: &mut egui::Ui, s: &mut NrSettings) {
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
}

/// The sun contact shadows' tab (sun_contact_shadows.rs): the strength, the march and the
/// switch, live; `sun-shadows.ini` keeps them.
fn shadows_tab(ui: &mut egui::Ui, enabled: &mut bool, t: &mut crate::sun_contact_shadows::SunContactTuning) {
    ui.checkbox(enabled, "Enable sun contact shadows");
    ui.label("Short shadows of grass, kerbs and low objects the sun's shadow maps miss.");
    ui.add(egui::Slider::new(&mut t.strength, 0.0..=1.0).text("Strength"));
    ui.add(egui::Slider::new(&mut t.length, crate::sun_contact_shadows::LENGTH_RANGE[0]..=crate::sun_contact_shadows::LENGTH_RANGE[1]).text("Length (units)"));
    ui.add(egui::Slider::new(&mut t.thickness, crate::sun_contact_shadows::THICKNESS_RANGE[0]..=crate::sun_contact_shadows::THICKNESS_RANGE[1]).text("Thickness (units)"));
    ui.add(egui::Slider::new(&mut t.bias, crate::sun_contact_shadows::BIAS_RANGE[0]..=crate::sun_contact_shadows::BIAS_RANGE[1]).text("Bias (anti-acne)"));
    ui.checkbox(&mut t.full, "Full resolution (slower, sharper)");
    ui.add(egui::Slider::new(&mut t.softness, 0.0..=1.0).text("Softness (0 = crisp, 1 = wide)"));
    ui.add(egui::Slider::new(&mut t.history, 0.0..=0.95).text("History (0 = off, higher = steadier)"));
    ui.add(egui::Slider::new(&mut t.steps, 4..=32).text("March steps"));
    ui.add(egui::Slider::new(&mut t.distance, crate::sun_contact_shadows::DISTANCE_RANGE[0]..=crate::sun_contact_shadows::DISTANCE_RANGE[1]).text("Contact range (units)"));
    ui.add(egui::Slider::new(&mut t.distant_distance, 0.0..=crate::sun_contact_shadows::DISTANT_DISTANCE_MAX).text("Distant range (0 = off)").step_by(1000.0));
    ui.separator();
    if ui.button("Reset to defaults").clicked() {
        *t = crate::sun_contact_shadows::SunContactTuning::default();
    }
    ui.label("Outdoors only. Needs a single-sampled view: TAA, FSR or DLSS (not MSAA).");
    ui.label("Saved to sun-shadows.ini; changes apply live. The switch is the settings row too.");
}

/// The terrain parallax's tab (terrain.rs "Parallax", nif_parallax.rs): the switches and
/// the march's height, for the landscape and for NIF materials alike.
fn parallax_tab(ui: &mut egui::Ui, enabled: &mut bool, t: &mut crate::terrain::ParallaxTuning) {
    ui.checkbox(enabled, "Enable terrain parallax");
    ui.label("Needs a ground pack: textures\\pbr\\<the ground texture's path>_p.dds, and Authored materials on at the start.");
    ui.checkbox(&mut t.objects, "Enable object parallax (NIF height maps)");
    ui.label("Objects: the game's own height maps and the packs' _p on shapes (walls, rubble, crates).");
    ui.add(egui::Slider::new(&mut t.height, 0.0..=crate::terrain::PARALLAX_HEIGHT_MAX).text("Height x (terrain + objects)"));
    ui.add(egui::Slider::new(&mut t.ao, 0.0..=1.0).text("Self-shadow AO (terrain)"));
    ui.add(egui::Slider::new(&mut t.fade, 200.0..=4000.0).text("Distance (units, terrain parallax)"));
    ui.label("1 = the depth the pack's .toml asks (0.03 by default); 0 = flat.");
    ui.label("Terrain anti-tiling must be off: the march does not run with it.");
    ui.label("Saved to parallax.ini; changes apply live.");
}

/// The water's reflection factors (water.rs `WaterTuning`), ScreenSpaceShaders' water tab.
fn water_tab(ui: &mut egui::Ui, t: &mut crate::water::WaterTuning) {
    ui.label("Reflection factors (ScreenSpaceShaders' water):");
    ui.add(egui::Slider::new(&mut t.sky, 0.0..=2.0).text("Sky reflection"));
    ui.add(egui::Slider::new(&mut t.map, 0.0..=2.0).text("Scene reflection (SSR)"));
    ui.add(egui::Slider::new(&mut t.vibrance, 0.0..=1.0).text("Reflection vibrance"));
    ui.horizontal(|ui| {
        ui.label("SSR quality:");
        egui::ComboBox::from_id_salt("water_ssr_quality")
            .selected_text(crate::water::water_quality_name(t.quality))
            .show_ui(ui, |ui| {
                for q in 0..=4u32 {
                    ui.selectable_value(&mut t.quality, q, crate::water::water_quality_name(q));
                }
            });
    });
    ui.separator();
    ui.label("Surface (ScreenSpaceShaders' ssfx_water):");
    ui.add(egui::Slider::new(&mut t.blur, 0.0..=1.0).text("Reflection blur"));
    ui.add(egui::Slider::new(&mut t.pattern, 0.0..=1.0).text("Glassy patches"));
    ui.add(egui::Slider::new(&mut t.distortion, 0.0..=2.0).text("Wave distortion"));
    ui.add(egui::Slider::new(&mut t.parallax, 0.0..=2.0).text("Wave parallax"));
    ui.separator();
    ui.label("Screen-space reflection:");
    ui.add(egui::Slider::new(&mut t.intensity, 0.0..=2.0).text("SSR intensity"));
    ui.add(egui::Slider::new(&mut t.distance, 20.0..=500.0).suffix(" m").text("SSR distance"));
    ui.add(egui::Slider::new(&mut t.thickness, 0.25..=4.0).text("Depth tolerance"));
    ui.separator();
    if ui.button("Reset to defaults").clicked() {
        *t = crate::water::WaterTuning::default();
    }
    ui.label("1.0 = as authored; the scene's share also scales the screen-space reflections.");
    ui.label("Saved to water.ini; changes apply live.");
}

fn lighting_tab(ui: &mut egui::Ui, lighting: &mut crate::sky::OutdoorLighting) {
    use crate::sky::OutdoorLighting;

    let sun = ui.add(egui::Slider::new(&mut lighting.sun, OutdoorLighting::RANGE).text("Sunlight").suffix("×").fixed_decimals(2));
    let sky = ui.add(egui::Slider::new(&mut lighting.sky, OutdoorLighting::RANGE).text("Ambient sky light").suffix("×").fixed_decimals(2));
    let reset = ui.button("Reset to defaults").clicked();
    ui.label("Outdoors only. 1.00× uses the weather's original lighting.");
    if reset {
        *lighting = OutdoorLighting::default();
    }
    if reset || [sun, sky].iter().any(|slider| slider.drag_stopped() || (slider.changed() && !slider.dragged())) {
        lighting.save();
    }
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
