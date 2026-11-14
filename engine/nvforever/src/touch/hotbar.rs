//! Original touch presentation of the existing gameplay hotkeys.
use bevy::{
    ecs::system::SystemParam,
    input::touch::{TouchInput, TouchPhase},
    prelude::*,
};
use nvf_play::{
    bindings::ControlInput,
    hotkeys::{AMMO_SWAP, Hotkeys, SLOTS},
    menu::{ActiveMenu, model::Mode},
    pipboy::{Phase, PipBoy, touch_items::TouchItems},
    settings::{self, ChoiceList, Setting, Slider},
};
use std::collections::HashSet;
#[path = "hotbar_gesture.rs"]
mod gesture;
use gesture::{Box2, Gesture, Outcome, Target};

#[derive(Resource, Clone, PartialEq)]
struct Preferences {
    enabled: bool,
    count: usize,
    scale: f32,
    tray: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            enabled: false,
            count: 4,
            scale: 1.,
            tray: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
struct Placement {
    center: Vec2,
    pins: [bool; SLOTS],
}
impl Default for Placement {
    fn default() -> Self {
        Self {
            center: Vec2::new(0.5, 0.82),
            pins: [false; SLOTS],
        }
    }
}
impl Placement {
    fn decode(value: &serde_json::Value) -> Option<Self> {
        if value.get("version")?.as_u64()? != 1 {
            return None;
        }
        let center = value.get("center")?.as_array()?;
        if center.len() != 2 {
            return None;
        }
        let x = center[0].as_f64()? as f32;
        let y = center[1].as_f64()? as f32;
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let mut out = Self {
            center: Vec2::new(x, y).clamp(Vec2::splat(0.05), Vec2::splat(0.95)),
            ..default()
        };
        let pins = value.get("pins")?.as_array()?;
        if pins.len() != SLOTS {
            return None;
        }
        for (target, value) in out.pins.iter_mut().zip(pins) {
            *target = value.as_bool()?;
        }
        out.pins[AMMO_SWAP] = false;
        Some(out)
    }
    fn encode(&self) -> serde_json::Value {
        serde_json::json!({"version":1,"center":[self.center.x,self.center.y],"pins":self.pins})
    }
    fn save(&self) {
        let Some(home) = std::env::var_os(fnv_install::HOME_ENV) else {
            return;
        };
        let path = std::path::PathBuf::from(home).join("touch-hotbar.json");
        let result = (|| -> Result<(), String> {
            use std::io::Write;
            fnv_install::guard_write(&path, None).map_err(|e| e.to_string())?;
            let tmp = path.with_extension("json.tmp");
            fnv_install::guard_write(&tmp, None).map_err(|e| e.to_string())?;
            let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
            file.write_all(&serde_json::to_vec_pretty(&self.encode()).map_err(|e| e.to_string())?)
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
            drop(file);
            std::fs::rename(tmp, path).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            warn!("touch hotbar placement could not be saved");
        }
    }
}
#[derive(Resource, Default)]
struct Bar {
    placement: Placement,
    gesture: Gesture,
    ignored: HashSet<u64>,
    context: Option<usize>,
    outcomes: Vec<Outcome>,
    tray_open: bool,
    moving: bool,
    move_id: Option<u64>,
    move_original: Option<Vec2>,
    safe_bottom: f32,
    size: Vec2,
    mode: u8,
    dimensions: Option<(usize, u32, bool)>,
    message: String,
}
impl Bar {
    fn cancel(&mut self) {
        self.mode = 0;
        self.gesture.cancel();
        self.ignored.clear();
        self.outcomes.clear();
        self.context = None;
        self.move_id = None;
        if let Some(original) = self.move_original.take() {
            self.placement.center = original;
        }
        self.moving = false;
    }
}

/// Parent inventory gestures can request the tray without taking the Pip-Boy swipe.
#[derive(Message, Clone, Copy)]
pub(crate) struct UtilityTrayRequest(pub bool);

#[derive(Clone, Debug, PartialEq)]
struct Geometry {
    slots: Vec<(usize, Rect)>,
    tray: Option<Rect>,
    context: Vec<Rect>,
    bounds: Rect,
}
fn geometry(size: Vec2, prefs: &Preferences, bar: &Bar) -> Option<Geometry> {
    if !size.is_finite() || !bar.safe_bottom.is_finite() {
        return None;
    }
    // Keep the parent's bottom-swipe handle and Android gesture inset unobstructed.
    let size = Vec2::new(size.x, size.y - bar.safe_bottom.max(0.) - 64.);
    if !size.is_finite() || size.min_element() < 96. || !prefs.scale.is_finite() {
        return None;
    }
    let side = (size.min_element() * 0.11 * prefs.scale.clamp(0.6, 1.8)).clamp(48., 96.);
    let gap = 6.;
    let count = prefs.count.min(SLOTS);
    let buttons = count + usize::from(prefs.tray);
    let columns = (((size.x - 16.) / (side + gap)).floor() as usize).clamp(1, buttons.max(1));
    let rows = buttons.div_ceil(columns);
    let width = columns as f32 * (side + gap) - gap;
    let height = rows as f32 * (side + gap) - gap;
    let half = Vec2::new(width, height) * 0.5;
    if half.x * 2. > size.x - 8. || half.y * 2. > size.y - 8. {
        return None;
    }
    let center =
        (bar.placement.center * size).clamp(half + Vec2::splat(4.), size - half - Vec2::splat(4.));
    let bounds = Rect::from_center_size(center, half * 2.);
    let rect = |i: usize| {
        Rect::from_corners(
            bounds.min
                + Vec2::new(
                    (i % columns) as f32 * (side + gap),
                    (i / columns) as f32 * (side + gap),
                ),
            bounds.min
                + Vec2::new(
                    (i % columns) as f32 * (side + gap) + side,
                    (i / columns) as f32 * (side + gap) + side,
                ),
        )
    };
    let mut slots = (0..count).map(|i| (i, rect(i))).collect::<Vec<_>>();
    if prefs.tray && bar.tray_open {
        let pinned = bar
            .placement
            .pins
            .iter()
            .enumerate()
            .filter(|(_, on)| **on)
            .map(|(i, _)| i);
        for (k, i) in pinned.enumerate() {
            let y = bounds.min.y - (side + gap) * (1 + k / columns) as f32;
            if y < 4. {
                break;
            }
            let x = (bounds.min.x + (k % columns) as f32 * (side + gap)).min(size.x - side - 4.);
            slots.push((
                i,
                Rect::from_corners(Vec2::new(x, y), Vec2::new(x + side, y + side)),
            ));
        }
    }
    let context = if bar.context.is_some() {
        let width = (size.x * 0.42).clamp(220., 360.).min(size.x - 16.);
        let height = 48.;
        let top = (bounds.min.y - height * 4. - 12.).clamp(4., (size.y - height * 4. - 4.).max(4.));
        (0..4)
            .map(|i| {
                Rect::from_corners(
                    Vec2::new((size.x - width) * 0.5, top + i as f32 * height),
                    Vec2::new((size.x + width) * 0.5, top + (i + 1) as f32 * height),
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    Some(Geometry {
        slots,
        tray: prefs.tray.then(|| rect(count)),
        context,
        bounds,
    })
}
fn box2(r: Rect) -> Box2 {
    Box2 {
        min: r.min.to_array(),
        max: r.max.to_array(),
    }
}

#[derive(SystemParam)]
pub(super) struct Bridge<'w> {
    bar: ResMut<'w, Bar>,
    prefs: Res<'w, Preferences>,
    items: ResMut<'w, TouchItems>,
    active: Option<Res<'w, ActiveMenu>>,
    pip: Option<Res<'w, PipBoy>>,
    fresh: Option<Res<'w, nvf_play::menu::FreshMenu>>,
    time: Res<'w, Time<Real>>,
    safe: Option<Res<'w, super::drawer::SafeBottom>>,
}
impl Bridge<'_> {
    pub fn cancel(&mut self) {
        self.bar.cancel();
        self.items.cancel();
    }
    pub fn prepare(&mut self, size: Vec2, allowed: bool, playing: bool) {
        let inventory = self
            .active
            .as_ref()
            .and_then(|a| a.0.as_ref())
            .is_some_and(|m| m.mode == Mode::Inventory)
            && self.pip.as_ref().is_some_and(|p| p.phase == Phase::Up);
        let mode = if !allowed || !self.prefs.enabled {
            0
        } else if playing {
            1
        } else if inventory {
            2
        } else {
            0
        };
        let safe = self.safe.as_ref().map_or(0., |s| s.0).max(0.);
        let dimensions = (
            self.prefs.count,
            self.prefs.scale.to_bits(),
            self.prefs.tray,
        );
        if mode != self.bar.mode
            || self.bar.size != size
            || self.bar.dimensions != Some(dimensions)
            || self.bar.safe_bottom != safe
            || self.fresh.as_ref().is_some_and(|f| f.0)
        {
            self.cancel();
        }
        self.bar.dimensions = Some(dimensions);
        self.bar.safe_bottom = safe;
        self.bar.mode = mode;
        self.bar.size = size;
    }
    pub fn event(&mut self, event: &TouchInput, unclaimed: bool) -> bool {
        if self.bar.mode == 0 {
            return false;
        }
        let now = self
            .time
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        if self.items.owns(event.id) {
            self.items.event(event, now);
            if event.phase == TouchPhase::Ended {
                let target = geometry(self.bar.size, &self.prefs, &self.bar).and_then(|g| {
                    g.slots
                        .into_iter()
                        .find(|(_, r)| r.contains(event.position))
                        .map(|(i, _)| i)
                });
                self.items.release_over(target);
            }
            return true;
        }
        if self.bar.ignored.contains(&event.id) {
            if matches!(event.phase, TouchPhase::Ended | TouchPhase::Canceled) {
                self.bar.ignored.remove(&event.id);
            }
            return true;
        }
        if self.bar.move_id == Some(event.id) {
            if event.phase == TouchPhase::Moved && event.position.is_finite() {
                let size = Vec2::new(
                    self.bar.size.x,
                    (self.bar.size.y - self.bar.safe_bottom - 64.).max(1.),
                );
                self.bar.placement.center =
                    (event.position / size).clamp(Vec2::splat(0.05), Vec2::splat(0.95));
            }
            if matches!(event.phase, TouchPhase::Ended | TouchPhase::Canceled) {
                self.bar.move_id = None;
                self.bar.moving = false;
                if event.phase == TouchPhase::Ended && event.position.is_finite() {
                    self.bar.move_original = None;
                    self.bar.placement.save();
                } else if let Some(original) = self.bar.move_original.take() {
                    self.bar.placement.center = original;
                }
            }
            return true;
        }
        let Some(g) = geometry(self.bar.size, &self.prefs, &self.bar) else {
            return false;
        };
        if self.bar.gesture.owns(event.id) {
            let size = self.bar.size.to_array();
            match event.phase {
                TouchPhase::Moved => {
                    self.bar
                        .gesture
                        .moved(event.id, event.position.to_array(), size)
                }
                TouchPhase::Ended | TouchPhase::Canceled => {
                    if let Some(o) = self.bar.gesture.end(
                        event.id,
                        event.position.to_array(),
                        size,
                        now,
                        event.phase == TouchPhase::Canceled,
                    ) {
                        if self.bar.outcomes.is_empty() {
                            self.bar.outcomes.push(o);
                        }
                    }
                }
                _ => {}
            }
            return true;
        }
        if event.phase != TouchPhase::Started {
            return false;
        }
        if self.bar.moving && g.bounds.contains(event.position) {
            self.bar.move_id = Some(event.id);
            self.bar.move_original = Some(self.bar.placement.center);
            return true;
        }
        let target = g
            .context
            .iter()
            .enumerate()
            .find(|(_, r)| r.contains(event.position))
            .map(|(i, r)| (Target::Context(i), *r))
            .or_else(|| {
                g.tray
                    .filter(|r| r.contains(event.position))
                    .map(|r| (Target::Tray, r))
            })
            .or_else(|| {
                g.slots
                    .iter()
                    .find(|(_, r)| r.contains(event.position))
                    .map(|(i, r)| (Target::Slot(*i), *r))
            });
        if let Some((target, rect)) = target {
            if self.bar.gesture.busy() || self.items.busy() {
                if self.bar.ignored.len() < 16 {
                    self.bar.ignored.insert(event.id);
                }
            } else {
                let size = self.bar.size.to_array();
                self.bar.gesture.start(
                    event.id,
                    target,
                    box2(rect),
                    event.position.to_array(),
                    size,
                    now,
                );
            }
            return true;
        }
        if self.bar.context.is_some() {
            self.bar.context = None;
            return true;
        }
        if self.bar.mode == 2 && unclaimed && !self.items.busy() {
            return self.items.event(event, now);
        }
        false
    }
}

pub(super) fn install(app: &mut App) {
    app.init_resource::<Preferences>()
        .init_resource::<Bar>()
        .init_resource::<TouchItems>()
        .add_message::<UtilityTrayRequest>()
        .add_message::<nvf_play::weapon_wheel::TouchHotkey>()
        .add_systems(Startup, load)
        .add_systems(
            Update,
            dispatch
                .before(nvf_play::PlaySet::Input)
                .run_if(resource_exists::<nvf_play::sim::SimState>)
                .run_if(resource_exists::<Hotkeys>)
                .run_if(resource_exists::<nvf_play::bindings::Bindings>),
        )
        .add_systems(
            Update,
            (assign, draw)
                .chain()
                .after(nvf_play::pipboy::PointerSet)
                .run_if(resource_exists::<nvf_play::sim::SimState>)
                .run_if(resource_exists::<Hotkeys>),
        );
    let row = |key, label, note, get, set| Setting {
        key,
        group: "Controls",
        label,
        note,
        get,
        set,
        slider: None,
    };
    settings::add_setting(
        app,
        row(
            "touch-hotbar",
            "Touch hotbar",
            "Show quick slots on screen. Tap to use; hold for safe slot options. Item assignments follow the game's saved hotkeys.",
            |w| w.resource::<Preferences>().enabled,
            |w, on| w.resource_mut::<Preferences>().enabled = on,
        ),
    );
    settings::add_setting(
        app,
        row(
            "touch-hotbar-tray",
            "Pinned consumable tray",
            "Show a tray button for pinned Aid slots. Holding a slot offers Pin; no item is consumed by a hold.",
            |w| w.resource::<Preferences>().tray,
            |w, on| w.resource_mut::<Preferences>().tray = on,
        ),
    );
    settings::add_choice_list(
        app,
        row(
            "touch-hotbar-slots",
            "Visible quick slots",
            "Choose four, six or eight. Slot 2 remains Ammo Swap. Hidden slots remain assigned.",
            |_| false,
            |_, _| {},
        ),
        ChoiceList {
            names: |_| vec!["4".into(), "6".into(), "8".into()],
            chosen: |w| w.resource::<Preferences>().count.to_string(),
            pick: |w, v| {
                let n = match v {
                    "4" => 4,
                    "6" => 6,
                    "8" => 8,
                    _ => return false,
                };
                w.resource_mut::<Preferences>().count = n;
                true
            },
            save: |w| w.resource::<Preferences>().count.to_string(),
            load: |w, v| {
                let n = match v {
                    "4" => 4,
                    "6" => 6,
                    "8" => 8,
                    _ => return false,
                };
                w.resource_mut::<Preferences>().count = n;
                true
            },
            ask_keep: false,
        },
    );
    settings::add_setting(
        app,
        Setting::slider(
            "touch-hotbar-size",
            "Controls",
            "Quick slot size",
            "Resize the visible controls and their touch areas together. Hold a slot and choose Move bar to reposition.",
            Slider {
                get: |w| (w.resource::<Preferences>().scale - 0.6) / 1.2,
                set: |w, v| w.resource_mut::<Preferences>().scale = 0.6 + v.clamp(0., 1.) * 1.2,
                min: 0.,
                step: 0.1,
                shown: Some(|v| format!("{:.0}%", (0.6 + v * 1.2) * 100.0)),
            },
        ),
    );
}
fn load(mut commands: Commands, mut bar: ResMut<Bar>) {
    commands.spawn(nvf_play::pipboy::touch_items::POINTER);
    if !cfg!(target_os = "android") {
        return;
    }
    let Some(home) = std::env::var_os(fnv_install::HOME_ENV) else {
        return;
    };
    let path = std::path::PathBuf::from(home).join("touch-hotbar.json");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() <= 8192) {
        if let Some(p) = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .and_then(|v| Placement::decode(&v))
        {
            bar.placement = p;
        }
    }
}
fn dispatch(
    input: ControlInput,
    (mut bar, mut items): (ResMut<Bar>, ResMut<TouchItems>),
    (mut hotkeys, sim): (ResMut<Hotkeys>, Res<nvf_play::sim::SimState>),
    (touch, time): (
        Option<Res<nvf_play::pad::TouchDeviceSwitch>>,
        Res<Time<Real>>,
    ),
    (mut out, mut tray): (
        MessageWriter<nvf_play::weapon_wheel::TouchHotkey>,
        MessageReader<UtilityTrayRequest>,
    ),
) {
    if input.controller()
        || input.suppressed()
        || touch.as_ref().is_some_and(|t| !t.allows_actions())
    {
        bar.cancel();
        items.cancel();
        tray.clear();
        return;
    }
    for request in tray.read() {
        bar.tray_open = request.0;
    }
    let now = time.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    items.update_hold(now);
    if let Some(o) = bar.gesture.tick(now) {
        if bar.outcomes.is_empty() {
            bar.outcomes.push(o);
        }
    }
    for outcome in std::mem::take(&mut bar.outcomes) {
        match outcome {
            Outcome::Context(slot) => {
                bar.context = Some(slot);
            }
            Outcome::Tap(Target::Slot(slot)) => {
                if bar.mode == 1 && slot < SLOTS {
                    out.write(nvf_play::weapon_wheel::TouchHotkey(slot));
                }
            }
            Outcome::Tap(Target::Tray) => bar.tray_open = !bar.tray_open,
            Outcome::Tap(Target::Context(action)) => {
                let Some(slot) = bar.context.take() else {
                    continue;
                };
                match action {
                    0 => {
                        let aid = hotkeys.0[slot]
                            .and_then(|s| sim.forms.item(fnv_sim::FormId(s.item)))
                            .is_some_and(|i| i.kind == fnv_sim::forms::ItemKind::Aid);
                        if slot != AMMO_SWAP && aid {
                            bar.placement.pins[slot] = !bar.placement.pins[slot];
                            bar.placement.save();
                        } else {
                            bar.message = "Only Aid slots can be pinned.".into();
                        }
                    }
                    1 => {
                        if slot != AMMO_SWAP {
                            hotkeys.0[slot] = None;
                            bar.placement.pins[slot] = false;
                            bar.placement.save();
                            bar.message = "Slot cleared.".into();
                        } else {
                            bar.message = "Slot 2 is reserved for Ammo Swap.".into();
                        }
                    }
                    2 => {
                        bar.moving = true;
                        bar.message = "Drag the bar to reposition it.".into();
                    }
                    _ => {}
                }
            }
        }
    }
}
fn assign(
    active: Option<Res<ActiveMenu>>,
    mut items: ResMut<TouchItems>,
    mut hotkeys: ResMut<Hotkeys>,
) {
    if let Some(active) = active {
        items.assign_pending(&active, &mut hotkeys);
    }
}

#[derive(Component)]
struct Visual;
#[derive(Clone, PartialEq)]
struct Picture {
    mode: u8,
    size: Vec2,
    geometry: Geometry,
    entries: Vec<(usize, String, i32, bool)>,
    context: Option<usize>,
    pinned: [bool; SLOTS],
    message: String,
}
fn draw(
    mut commands: Commands,
    (prefs, bar, items): (Res<Preferences>, Res<Bar>, Res<TouchItems>),
    hotkeys: Res<Hotkeys>,
    mut sim: ResMut<nvf_play::sim::SimState>,
    roots: Query<Entity, With<Visual>>,
    mut previous: Local<Option<Picture>>,
) {
    let g = geometry(bar.size, &prefs, &bar);
    if bar.mode == 0 || !prefs.enabled || g.is_none() {
        for e in &roots {
            commands.entity(e).despawn();
        }
        *previous = None;
        return;
    }
    let g = g.unwrap();
    let mut entries = Vec::new();
    for (slot, _) in &g.slots {
        let (name, count, aid) = if *slot == AMMO_SWAP {
            ("Ammo Swap".into(), 1, false)
        } else if let Some(s) = hotkeys.0[*slot] {
            let name = sim
                .name(fnv_sim::FormId(s.item))
                .unwrap_or("Unknown")
                .chars()
                .take(24)
                .collect();
            let aid = sim
                .forms
                .item(fnv_sim::FormId(s.item))
                .is_some_and(|i| i.kind == fnv_sim::forms::ItemKind::Aid);
            (name, nvf_play::hotkeys::carried(&mut sim.world, s).0, aid)
        } else {
            ("Empty".into(), 0, false)
        };
        entries.push((*slot, name, count, aid));
    }
    let picture = Picture {
        mode: bar.mode,
        size: bar.size,
        geometry: g.clone(),
        entries: entries.clone(),
        context: bar.context,
        pinned: bar.placement.pins,
        message: if items.message.is_empty() {
            bar.message.clone()
        } else {
            items.message.clone()
        },
    };
    if previous.as_ref() == Some(&picture) {
        return;
    }
    *previous = Some(picture.clone());
    for e in &roots {
        commands.entity(e).despawn();
    }
    for ((slot, r), (_, name, count, aid)) in g.slots.iter().zip(entries) {
        tile(
            &mut commands,
            *r,
            &format!(
                "{} {}{}",
                slot + 1,
                name,
                if count > 1 {
                    format!(" ({count})")
                } else {
                    String::new()
                }
            ),
            Some(if aid { "aid" } else { "quickwheel" }),
            count > 0,
        );
    }
    if let Some(r) = g.tray {
        tile(&mut commands, r, "Pins", Some("quickwheel"), true);
    }
    if !picture.message.is_empty() {
        let width = (bar.size.x * 0.6).clamp(160., 480.).min(bar.size.x - 8.);
        let top = (g.bounds.min.y - 38.).max(4.);
        tile(
            &mut commands,
            Rect::from_corners(
                Vec2::new((bar.size.x - width) * 0.5, top),
                Vec2::new((bar.size.x + width) * 0.5, top + 32.),
            ),
            &picture.message,
            None,
            true,
        );
    }
    if let Some(slot) = bar.context {
        let pin = if bar.placement.pins[slot] {
            "Unpin consumable"
        } else {
            "Pin consumable"
        };
        for (r, label) in g
            .context
            .into_iter()
            .zip([pin, "Clear slot", "Move bar", "Close"])
        {
            tile(&mut commands, r, label, None, true);
        }
    }
}
fn tile(commands: &mut Commands, r: Rect, label: &str, icon: Option<&str>, enabled: bool) {
    commands
        .spawn((
            Visual,
            Node {
                position_type: PositionType::Absolute,
                left: px(r.min.x),
                top: px(r.min.y),
                width: px(r.width()),
                height: px(r.height()),
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(px(8.)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.06, 0.07, 0.88)),
            BorderColor::all(Color::srgba(
                0.7,
                0.9,
                0.85,
                if enabled { 0.9 } else { 0.35 },
            )),
            GlobalZIndex(2200),
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            if let Some(index) = icon.and_then(super::icons::semantic_index) {
                p.spawn((
                    ImageNode::default(),
                    super::icons::TouchIcon(index),
                    Node {
                        width: px(r.height() * 0.48),
                        height: px(r.height() * 0.48),
                        ..default()
                    },
                    Pickable::IGNORE,
                ));
            }
            p.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(12.),
                    ..default()
                },
                TextColor(Color::WHITE),
                Pickable::IGNORE,
            ));
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placement_roundtrips_bounded_geometry_and_reserved_pin() {
        let mut p = Placement::default();
        p.center = Vec2::new(0.3, 0.7);
        p.pins[2] = true;
        assert_eq!(Placement::decode(&p.encode()), Some(p));
        let bad = serde_json::json!({"version":1,"center":[999,-50],"pins":[true,true,false,false,false,false,false,false]});
        let clamped = Placement::decode(&bad).unwrap();
        assert_eq!(clamped.center, Vec2::new(0.95, 0.05));
        assert!(!clamped.pins[AMMO_SWAP]);
        for value in [
            serde_json::json!({"version":8}),
            serde_json::json!({"version":1,"center":[0.5,0.8],"pins":[]}),
        ] {
            assert!(Placement::decode(&value).is_none());
        }
    }
    #[test]
    fn geometry_resizes_visible_and_hit_bounds_together_without_overlapping_tray() {
        let mut bar = Bar::default();
        bar.tray_open = true;
        bar.placement.pins = [true; SLOTS];
        for size in [Vec2::new(1280., 720.), Vec2::new(640., 360.)] {
            for count in [4, 6, 8] {
                let small = geometry(
                    size,
                    &Preferences {
                        count,
                        scale: 0.6,
                        ..default()
                    },
                    &bar,
                )
                .unwrap();
                let large = geometry(
                    size,
                    &Preferences {
                        count,
                        scale: 1.8,
                        ..default()
                    },
                    &bar,
                )
                .unwrap();
                assert!(large.slots[0].1.width() >= small.slots[0].1.width());
                for g in [&small, &large] {
                    for (_, r) in &g.slots {
                        assert!(r.width() >= 48. && r.height() >= 48.);
                        assert!(box2(*r).contains(r.center().to_array()));
                        assert!(
                            r.min.x >= 0.
                                && r.min.y >= 0.
                                && r.max.x <= size.x
                                && r.max.y <= size.y
                        );
                    }
                    for a in 0..g.slots.len() {
                        for b in a + 1..g.slots.len() {
                            let x = g.slots[a].1.intersect(g.slots[b].1);
                            assert!(x.width() <= 0. || x.height() <= 0.);
                        }
                    }
                }
            }
        }
        for bad in [Vec2::ZERO, Vec2::splat(f32::NAN), Vec2::new(10., 100.)] {
            assert!(geometry(bad, &Preferences::default(), &bar).is_none());
        }
    }
    #[test]
    fn real_ui_nodes_use_the_same_slot_rectangles_as_touch_admission() {
        use bevy::ecs::system::RunSystemOnce;
        let forms = Box::leak(Box::new(fnv_sim::FormStore::default()));
        let refs = Box::leak(Box::new(fnv_sim::RefTable::default()));
        let mut world = World::new();
        let prefs = Preferences {
            enabled: true,
            count: 6,
            ..default()
        };
        let bar = Bar {
            mode: 1,
            size: Vec2::new(1000., 600.),
            ..default()
        };
        let expected = geometry(bar.size, &prefs, &bar).unwrap();
        world.insert_resource(prefs);
        world.insert_resource(bar);
        world.init_resource::<TouchItems>();
        world.init_resource::<Hotkeys>();
        world.insert_resource(nvf_play::sim::SimState {
            forms,
            refs,
            world: fnv_sim::World::new(forms, refs, fnv_sim::SimConfig::default()),
        });
        world.run_system_once(draw).unwrap();
        let nodes = world
            .query_filtered::<&Node, With<Visual>>()
            .iter(&world)
            .collect::<Vec<_>>();
        assert_eq!(nodes.len(), 7, "six slots plus tray");
        for (_, r) in &expected.slots {
            assert!(nodes.iter().any(|n| n.left == px(r.min.x)
                && n.top == px(r.min.y)
                && n.width == px(r.width())
                && n.height == px(r.height())));
        }
    }
}
