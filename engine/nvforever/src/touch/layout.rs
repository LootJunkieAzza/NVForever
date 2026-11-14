//! Owned, versioned touch layouts. Coordinates are fractions of the current window.
use super::BUTTONS;
use bevy::prelude::*;
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub center: Vec2,
    pub scale: f32,
    pub visible: bool,
}
#[derive(Clone, Debug)]
pub struct Config {
    pub buttons: Vec<Placement>,
    pub stick: Placement,
    pub floating: bool,
    pub southpaw: bool,
    pub opacity: f32,
    pub hints: bool,
    pub aiming: super::aiming::Aiming,
    pub appearance: super::profiles::Appearance,
    pub preset_name: String,
    // Preserve newer action IDs, fields and saved slots when an older editor writes.
    pub(super) extra: Value,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            buttons: BUTTONS
                .iter()
                .map(|b| Placement {
                    center: Vec2::new(b.x + 0.055, b.y + 0.06),
                    scale: 1.0,
                    visible: true,
                })
                .collect(),
            stick: Placement {
                center: Vec2::new(0.18, 0.76),
                scale: 1.0,
                visible: true,
            },
            floating: true,
            southpaw: false,
            opacity: 0.7,
            hints: true,
            aiming: super::aiming::Aiming::default(),
            appearance: super::profiles::Appearance::default(),
            preset_name: "classic".into(),
            extra: json!({}),
        }
    }
}
impl PartialEq for Config {
    fn eq(&self, other: &Self) -> bool {
        self.encode() == other.encode()
    }
}
fn overlay(mut previous: Value, known: Value) -> Value {
    if !previous.is_object() {
        previous = json!({});
    }
    if let Some(fields) = known.as_object() {
        for (key, value) in fields {
            previous[key] = value.clone();
        }
    }
    previous
}
fn merge_profile(previous: &mut Value, saved: &Value) {
    if let (Some(a), Some(b)) = (previous.as_object_mut(), saved.as_object()) {
        for (key, value) in b {
            merge_profile(a.entry(key.clone()).or_insert(Value::Null), value);
        }
    } else {
        *previous = saved.clone();
    }
}
impl Config {
    pub fn preset(southpaw: bool) -> Self {
        let mut c = Self {
            southpaw,
            ..Self::default()
        };
        if southpaw {
            for b in &mut c.buttons {
                b.center.x = 1.0 - b.center.x;
            }
            c.stick.center.x = 1.0 - c.stick.center.x;
        }
        c
    }
    pub fn diameter(&self, target: Target, size: Vec2) -> f32 {
        let base = if target == Target::Stick {
            (size.min_element() * 0.28).max(104.0)
        } else {
            (size.min_element() * 0.12).max(48.0)
        };
        (base * self.placement(target).scale)
            .max(48.0)
            .min(size.min_element() * 0.48)
    }
    pub fn placement(&self, target: Target) -> &Placement {
        match target {
            Target::Stick => &self.stick,
            Target::Button(i) => &self.buttons[i],
        }
    }
    pub fn placement_mut(&mut self, target: Target) -> &mut Placement {
        match target {
            Target::Stick => &mut self.stick,
            Target::Button(i) => &mut self.buttons[i],
        }
    }
    pub fn center(&self, target: Target, size: Vec2) -> Vec2 {
        let r = self.diameter(target, size) * 0.5 + 4.0;
        (self.placement(target).center * size).clamp(Vec2::splat(r), size - Vec2::splat(r))
    }
    pub fn hit(&self, point: Vec2, size: Vec2, playing: bool, editing: bool) -> Option<Target> {
        self.hit_filtered(point, size, playing, editing, |_| true)
    }
    /// Shared active-control predicate for ordinary routing and owned overlay admission.
    pub fn hit_filtered(
        &self,
        point: Vec2,
        size: Vec2,
        playing: bool,
        editing: bool,
        enabled: impl Fn(usize) -> bool,
    ) -> Option<Target> {
        let mut nearest = None;
        let mut distance = f32::MAX;
        for (i, b) in BUTTONS.iter().enumerate() {
            let t = Target::Button(i);
            if b.playing != playing || (!editing && !self.buttons[i].visible) || !enabled(i) {
                continue;
            }
            let d = point.distance(self.center(t, size));
            let inside = self
                .appearance
                .geometry(self.diameter(t, size))
                .contains((point - self.center(t, size)).to_array());
            if inside && d < distance {
                nearest = Some(t);
                distance = d;
            }
        }
        if playing {
            let d = point.distance(self.center(Target::Stick, size));
            if d <= self.diameter(Target::Stick, size) * 0.65 && d < distance {
                nearest = Some(Target::Stick);
            }
        }
        nearest
    }
    pub fn movement_area(&self, p: Vec2) -> bool {
        p.y > 0.5
            && if self.stick.center.x > 0.5 {
                p.x > 0.55
            } else {
                p.x < 0.45
            }
    }
    pub fn look_area(&self, p: Vec2) -> bool {
        if self.stick.center.x > 0.5 {
            p.x < 0.5
        } else {
            p.x > 0.5
        }
    }
    pub fn encode(&self) -> Value {
        let place = |old: Value, p: &Placement| {
            overlay(
                old,
                json!({"x":p.center.x,"y":p.center.y,"scale":p.scale,"visible":p.visible}),
            )
        };
        let mut value = self.extra.clone();
        if !value.is_object() {
            value = json!({});
        }
        if !value["buttons"].is_object() {
            value["buttons"] = json!({});
        }
        for (b, p) in BUTTONS.iter().zip(&self.buttons) {
            value["buttons"][b.text] = place(value["buttons"][b.text].clone(), p);
        }
        value["stick"] = place(value["stick"].clone(), &self.stick);
        value["aiming"] = overlay(value["aiming"].clone(), self.aiming.encode());
        value["appearance"] = overlay(
            value["appearance"].clone(),
            json!({
            "theme":self.appearance.theme,"shape":self.appearance.shape,"family":self.appearance.family}),
        );
        overlay(
            value,
            json!({"version":1,"floating":self.floating,"southpaw":self.southpaw,
            "opacity":self.opacity,"hints":self.hints,"preset":self.preset_name}),
        )
    }
    pub fn apply_preset(&mut self, name: &str) {
        let mut fresh = Self::default();
        let positions = if name == "left" { "explorer" } else { name };
        for (i, p) in self.buttons.iter_mut().enumerate().take(9) {
            let mut next = fresh.buttons[i].clone();
            next.center = Vec2::from_array(super::profiles::position(
                positions,
                i,
                [next.center.x, next.center.y],
            ));
            next.scale = super::profiles::scale(positions, i);
            if name == "left" {
                next.center.x = 1.0 - next.center.x;
            }
            *p = next;
        }
        fresh.stick.center = Vec2::from_array(super::profiles::stick_position(
            name,
            fresh.stick.center.to_array(),
        ));
        fresh.stick.scale = super::profiles::stick_scale(name);
        self.stick = fresh.stick;
        self.southpaw = name == "left";
        self.preset_name = name.into();
        // A preset is placement only: aiming, appearance, unknown actions and slots survive.
    }
    pub fn slot_name(&self, slot: usize) -> Option<&str> {
        self.extra.get("profiles")?.get(slot)?.get("name")?.as_str()
    }
    pub fn store_slot(&mut self, slot: usize) -> Result<(), String> {
        if slot >= super::profiles::MAX_SLOTS {
            return Err("Invalid profile slot".into());
        }
        let mut snapshot = self.encode();
        snapshot.as_object_mut().unwrap().remove("profiles");
        let mut slots = self.extra["profiles"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        while slots.len() <= slot {
            slots.push(Value::Null);
        }
        if slots[slot]
            .get("layout")
            .is_some_and(|v| v["version"].as_u64() != Some(1))
        {
            return Err("This slot uses a newer layout version; kept unchanged".into());
        }
        let name = slots[slot]["name"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("My layout {}", slot + 1));
        slots[slot] = overlay(slots[slot].clone(), json!({"name":name,"layout":snapshot}));
        self.extra["profiles"] = Value::Array(slots);
        Ok(())
    }
    pub fn load_slot(&mut self, slot: usize) -> Result<(), String> {
        let value = &self.extra["profiles"][slot]["layout"];
        if value.get("profiles").is_some() {
            return Err("Nested profile libraries are not supported".into());
        }
        let mut merged = self.encode();
        merge_profile(&mut merged, value);
        *self = Self::decode(&merged)?;
        Ok(())
    }
    pub fn rename_slot(&mut self, slot: usize) -> Result<(), String> {
        let name = self
            .slot_name(slot)
            .ok_or("Store a layout in this slot first")?;
        let next = super::profiles::cycle(name, super::profiles::SLOT_NAMES.iter().copied());
        self.extra["profiles"][slot]["name"] = json!(next);
        Ok(())
    }
    pub fn decode(v: &Value) -> Result<Self, String> {
        if v["version"].as_u64() != Some(1) {
            return Err("Unknown touch-layout version".into());
        }
        if v.get("profiles").is_some_and(|p| {
            !p.is_array() || p.as_array().unwrap().len() > super::profiles::MAX_SLOTS
        }) {
            return Err("Invalid saved-profile library".into());
        }
        if v["profiles"].as_array().is_some_and(|slots| {
            slots.iter().any(|slot| {
                !slot.is_null()
                    && (!slot["layout"].is_object()
                        || !slot["name"].as_str().is_some_and(|name| {
                            !name.is_empty()
                                && name.chars().count() <= 32
                                && !name.chars().any(char::is_control)
                        }))
            })
        }) {
            return Err("Invalid saved-profile entry".into());
        }
        let mut c = Self { extra: v.clone(), ..Self::default() };
        let identity = |key: &str| {
            v["appearance"][key]
                .as_str()
                .filter(|s| s.len() <= 64 && !s.chars().any(char::is_control))
                .map(str::to_owned)
        };
        if let Some(t) = identity("theme") {
            c.appearance.theme = t;
        }
        if let Some(t) = identity("shape") {
            c.appearance.shape = t;
        }
        c.appearance.family = identity("family");
        if let Some(name) = v["preset"].as_str().filter(|n| n.len() <= 64) {
            c.preset_name = name.into();
        }
        let read = |v: &Value, p: &mut Placement| {
            let number = |key: &str, lo: f32, hi: f32| {
                v[key]
                    .as_f64()
                    .filter(|n| n.is_finite())
                    .map(|n| (n as f32).clamp(lo, hi))
            };
            if let Some(x) = number("x", 0.0, 1.0) {
                p.center.x = x;
            }
            if let Some(y) = number("y", 0.0, 1.0) {
                p.center.y = y;
            }
            if let Some(s) = number("scale", 0.8, 1.8) {
                p.scale = s;
            }
            if let Some(b) = v["visible"].as_bool() {
                p.visible = b;
            }
        };
        for (i, b) in BUTTONS.iter().enumerate() {
            read(&v["buttons"][b.text], &mut c.buttons[i]);
        }
        read(&v["stick"], &mut c.stick);
        c.aiming = super::aiming::Aiming::decode(&v["aiming"]);
        if let Some(b) = v["floating"].as_bool() {
            c.floating = b;
        }
        if let Some(b) = v["southpaw"].as_bool() {
            c.southpaw = b;
        }
        if let Some(b) = v["hints"].as_bool() {
            c.hints = b;
        }
        if let Some(n) = v["opacity"].as_f64().filter(|n| n.is_finite()) {
            c.opacity = (n as f32).clamp(0.3, 0.9);
        }
        Ok(c)
    }
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        use std::io::Write;
        fnv_install::guard_write(path, None).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        fnv_install::guard_write(&tmp, None).map_err(|e| e.to_string())?;
        // Never silently downgrade or replace an unknown newer on-disk schema.
        if path.exists() {
            if std::fs::metadata(path).map_err(|e| e.to_string())?.len()
                > super::profiles::MAX_FILE_BYTES
            {
                return Err("Existing touch settings exceed the size limit; kept unchanged".into());
            }
            let old: Value =
                serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            if old["version"].as_u64() != Some(1) {
                return Err("Newer touch settings kept unchanged".into());
            }
            Self::decode(&old)?;
        }
        let bytes = serde_json::to_vec_pretty(&self.encode()).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > super::profiles::MAX_FILE_BYTES {
            return Err("Touch settings exceed the size limit".into());
        }
        let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Stick,
    Button(usize),
}
impl Target {
    pub fn name(self) -> &'static str {
        match self {
            Self::Stick => "Movement stick",
            Self::Button(i) => BUTTONS[i].text,
        }
    }
}
#[derive(Clone)]
pub struct Editor {
    original: Config,
    pub playing: bool,
    pub selected: Target,
    pub aiming: bool,
    pub styles: bool,
    pub slot: usize,
    drag: Option<(u64, Vec2)>,
}
#[derive(Resource, Default)]
pub struct Layout {
    pub config: Config,
    pub editor: Option<Editor>,
    pub message: String,
}
pub const EDIT_ACTIONS: [&str; 13] = [
    "Smaller",
    "Larger",
    "Opacity",
    "Mirror",
    "Stick mode",
    "Game / menu",
    "Show / hide",
    "Hints",
    "Reset",
    "Save",
    "Cancel",
    "Aiming",
    "Style / profiles",
];
pub fn editor_rect(index: usize, size: Vec2) -> Rect {
    let width = (size.x * 0.4).max(288.0).min(size.x * 0.75);
    let h = 52.0_f32.min(size.y * 0.15);
    let x = (size.x - width) * 0.5 + (index % 3) as f32 * width / 3.0;
    let y = (size.y - h * 5.0) * 0.5 + (index / 3) as f32 * h;
    Rect::from_corners(
        Vec2::new(x + 2.0, y + 2.0),
        Vec2::new(x + width / 3.0 - 2.0, y + h - 2.0),
    )
}
pub fn utility_rect(help: bool, size: Vec2) -> Rect {
    let center = Vec2::new(if help { 0.40 } else { 0.50 }, 0.075) * size;
    Rect::from_center_size(Vec2::new(center.x, center.y.max(28.0)), Vec2::splat(48.0))
}
impl Layout {
    pub fn open(&mut self, playing: bool) {
        self.editor = Some(Editor {
            original: self.config.clone(),
            playing,
            selected: Target::Stick,
            aiming: false,
            styles: false,
            slot: 0,
            drag: None,
        });
        self.message = "Drag a control to move it. Select it, then change its size.".into();
    }
    pub fn event(&mut self, t: &bevy::input::touch::TouchInput, size: Vec2) {
        use bevy::input::touch::TouchPhase;
        let Some(mut e) = self.editor.take() else {
            return;
        };
        let mut done = false;
        match t.phase {
            TouchPhase::Started => {
                let actions = if e.aiming || e.styles {
                    12
                } else {
                    EDIT_ACTIONS.len()
                };
                if let Some(action) = (0..actions).find(|&i| {
                    let rect = if e.aiming {
                        super::aiming::editor_rect(i, size)
                    } else {
                        editor_rect(i, size)
                    };
                    rect.contains(t.position)
                }) {
                    // A toolbar tap ends the previous finger's drag before changing its target/layout.
                    e.drag = None;
                    if e.styles && action < 9 {
                        let result = match action {
                            0 => {
                                self.config.appearance.next_theme();
                                Ok(())
                            }
                            1 => {
                                self.config.appearance.next_shape();
                                Ok(())
                            }
                            2 => {
                                self.config.appearance.family = super::icons::next_family(
                                    self.config.appearance.family.as_deref(),
                                )
                                .map(str::to_owned);
                                Ok(())
                            }
                            3 => {
                                let name = super::profiles::cycle(
                                    &self.config.preset_name,
                                    super::profiles::PRESETS.iter().map(|p| p.0),
                                );
                                self.config.apply_preset(name);
                                Ok(())
                            }
                            4 => {
                                e.slot = (e.slot + 1) % super::profiles::MAX_SLOTS;
                                Ok(())
                            }
                            5 => self.config.load_slot(e.slot),
                            6 => self.config.store_slot(e.slot),
                            7 => self.config.rename_slot(e.slot),
                            _ => {
                                let name = self.config.preset_name.clone();
                                self.config.apply_preset(&name);
                                Ok(())
                            }
                        };
                        self.message=match result{Ok(())=>"Preview changes here. Save keeps them; Cancel restores your layout and slots.".into(),Err(err)=>err};
                    } else if e.aiming && action < 9 {
                        self.config.aiming.adjust(action);
                        self.message = if action == 5 {
                            "Look scales touch swipes. ADS uses actual sight state; dead zone is a share of stick radius.".into()
                        } else {
                            format!(
                                "Look {:.0}% · ADS {:.0}% · dead zone {:.0}%",
                                self.config.aiming.look * 100.0,
                                self.config.aiming.ads * 100.0,
                                self.config.aiming.dead_zone * 100.0
                            )
                        };
                    } else {
                        match action {
                            0 | 1 => {
                                let p = self.config.placement_mut(e.selected);
                                p.scale = (p.scale + if action == 0 { -0.1 } else { 0.1 })
                                    .clamp(0.8, 1.8);
                            }
                            2 => {
                                self.config.opacity = if self.config.opacity >= 0.85 {
                                    0.35
                                } else {
                                    (self.config.opacity + 0.15).min(0.9)
                                };
                            }
                            3 => {
                                self.config.southpaw = !self.config.southpaw;
                                for p in &mut self.config.buttons {
                                    p.center.x = 1.0 - p.center.x;
                                }
                                self.config.stick.center.x = 1.0 - self.config.stick.center.x;
                            }
                            4 => self.config.floating = !self.config.floating,
                            5 => {
                                e.playing = !e.playing;
                                e.selected = Target::Stick;
                            }
                            6 => {
                                if e.selected != Target::Stick {
                                    let p = self.config.placement_mut(e.selected);
                                    p.visible = !p.visible;
                                }
                            }
                            7 => self.config.hints = !self.config.hints,
                            8 => {
                                let aiming = self.config.aiming;
                                let appearance = self.config.appearance.clone();
                                let extra = self.config.extra.clone();
                                self.config = Config::preset(self.config.southpaw);
                                self.config.aiming = aiming;
                                self.config.appearance = appearance;
                                self.config.extra = extra;
                            }
                            9 => {
                                // Tests use Layout without a home and never write player settings.
                                let result = std::env::var_os(fnv_install::HOME_ENV)
                                    .ok_or_else(|| "Settings directory is unavailable".to_string())
                                    .and_then(|p| {
                                        self.config.save_to(
                                            &std::path::PathBuf::from(p)
                                                .join("touch-controls.json"),
                                        )
                                    });
                                match result {
                                    Ok(()) => {
                                        self.message =
                                            "Touch layout saved. Resume from the pause menu."
                                                .into();
                                        done = true;
                                    }
                                    Err(err) => self.message = format!("Could not save: {err}"),
                                }
                            }
                            10 => {
                                self.config = e.original.clone();
                                self.message = "Changes cancelled.".into();
                                done = true;
                            }
                            12 => {
                                e.styles = true;
                                e.aiming = false;
                                self.message="Choose a theme, shape or layout. Store captures a slot; Load previews it.".into();
                            }
                            _ => {
                                if e.styles {
                                    e.styles = false;
                                } else {
                                    e.aiming = !e.aiming;
                                }
                                self.message = if e.aiming {
                                    "Adjust touch aiming, then Save or Cancel."
                                } else {
                                    "Drag a control to move it. Select it, then change its size."
                                }
                                .into();
                            }
                        }
                    }
                } else if let Some(target) = self
                    .config
                    .hit(t.position, size, e.playing, true)
                    .filter(|_| !e.aiming && !e.styles)
                {
                    e.selected = target;
                    e.drag = Some((t.id, self.config.center(target, size) - t.position));
                    self.message = format!(
                        "{} · drag to move · {:.0}%",
                        target.name(),
                        self.config.placement(target).scale * 100.0
                    );
                }
            }
            TouchPhase::Moved => {
                if let Some((id, offset)) = e.drag.filter(|(id, _)| *id == t.id) {
                    let r = self.config.diameter(e.selected, size) * 0.5 + 4.0;
                    self.config.placement_mut(e.selected).center =
                        ((t.position + offset).clamp(Vec2::splat(r), size - Vec2::splat(r))) / size;
                    e.drag = Some((id, offset));
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                if e.drag.is_some_and(|(id, _)| id == t.id) {
                    e.drag = None;
                }
            }
        }
        if !done {
            self.editor = Some(e);
        }
    }
}
pub fn load(mut layout: ResMut<Layout>) {
    if !cfg!(target_os = "android") {
        return;
    }
    let Some(home) = std::env::var_os(fnv_install::HOME_ENV) else {
        return;
    };
    let path = std::path::PathBuf::from(home).join("touch-controls.json");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > super::profiles::MAX_FILE_BYTES) {
        warn!("touch layout exceeds size limit; kept unchanged");
        return;
    }
    if let Ok(text) = std::fs::read_to_string(path) {
        match serde_json::from_str(&text)
            .map_err(|e| e.to_string())
            .and_then(|v| Config::decode(&v))
        {
            Ok(c) => layout.config = c,
            Err(e) => warn!("touch layout: {e}; using defaults"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tap_editor(layout: &mut Layout, action: usize) {
        use bevy::input::touch::{TouchInput, TouchPhase};
        let size = Vec2::new(792.0, 363.0);
        let position = editor_rect(action, size).center();
        for phase in [TouchPhase::Started, TouchPhase::Ended] {
            layout.event(
                &TouchInput {
                    phase,
                    position,
                    window: Entity::PLACEHOLDER,
                    id: 77,
                    force: None,
                },
                size,
            );
        }
    }
    #[test]
    fn editor_cancel_restores_profiles_appearance_and_original_settings() {
        let mut layout = Layout::default();
        layout.config.extra["foreign"] = json!({"keep":true});
        layout.config.store_slot(0).unwrap();
        let original = layout.config.clone();
        layout.open(true);
        for action in [12, 0, 1, 2, 3, 4, 6, 7] {
            tap_editor(&mut layout, action);
        }
        assert!(layout.editor.as_ref().unwrap().styles);
        assert_ne!(layout.config, original);
        assert!(layout.config.slot_name(1).is_some());
        tap_editor(&mut layout, 10);
        assert!(layout.editor.is_none());
        assert_eq!(layout.config, original);
    }
    #[test]
    fn layout_reset_keeps_profile_library_and_does_not_change_aiming() {
        let mut layout = Layout::default();
        layout.config.appearance.next_theme();
        layout.config.aiming.look = 1.9;
        layout.config.store_slot(0).unwrap();
        let slots = layout.config.extra["profiles"].clone();
        let appearance = layout.config.appearance.clone();
        layout.open(true);
        tap_editor(&mut layout, 8);
        assert_eq!(layout.config.extra["profiles"], slots);
        assert_eq!(layout.config.appearance, appearance);
        assert_eq!(layout.config.aiming.look, 1.9);
    }
    #[test]
    fn square_controls_admit_their_visible_corners() {
        let size = Vec2::new(792.0, 363.0);
        let mut c = Config::default();
        let target = Target::Button(7);
        let corner = c.center(target, size) + Vec2::splat(c.diameter(target, size) * 0.46);
        assert_eq!(c.hit(corner, size, true, false), None);
        c.appearance.shape = "square".into();
        assert_eq!(c.hit(corner, size, true, false), Some(target));
    }
    #[test]
    fn button_hits_follow_visible_edges_and_cut_corners_for_every_shape() {
        let size = Vec2::new(1600.0, 900.0);
        let target = Target::Button(7);
        for (shape, _) in super::super::profiles::SHAPES {
            let mut c = Config::default();
            c.appearance.shape = (*shape).into();
            let center = c.center(target, size);
            let d = c.diameter(target, size);
            let hit = |x, y| c.hit(center + Vec2::new(x * d, y * d), size, true, false);
            assert_eq!(hit(0.0, 0.0), Some(target), "{shape}: centre");
            assert_eq!(hit(0.5, 0.0), Some(target), "{shape}: visible right edge");
            assert_eq!(hit(0.51, 0.0), None, "{shape}: beyond right edge");
            if *shape == "square" {
                assert_eq!(hit(0.48, 0.48), Some(target));
            } else {
                assert_eq!(hit(0.48, 0.48), None, "{shape}: invisible corner");
            }
            if *shape == "capsule" {
                assert_eq!(hit(0.0, 0.35), Some(target));
                assert_eq!(hit(0.0, 0.49), None, "capsule: invisible circle above top");
                assert_eq!(hit(0.48, 0.30), None, "capsule: clipped corner");
            } else {
                assert_eq!(hit(0.0, 0.49), Some(target));
            }
        }
    }
    #[test]
    fn capsule_hit_ends_at_the_visible_top_instead_of_the_old_circle() {
        let size = Vec2::new(1600.0, 900.0);
        let target = Target::Button(7);
        let mut c = Config::default();
        c.appearance.shape = "capsule".into();
        let centre = c.center(target, size);
        let d = c.diameter(target, size);
        assert_eq!(
            c.hit(centre + Vec2::new(0.0, d * 0.35), size, true, false),
            Some(target)
        );
        assert_eq!(
            c.hit(centre + Vec2::new(0.0, d * 0.49), size, true, false),
            None
        );
    }
    #[test]
    fn button_shape_never_changes_the_joysticks_circular_grab_region() {
        let size = Vec2::new(1600.0, 900.0);
        for (shape, _) in super::super::profiles::SHAPES {
            let mut c = Config::default();
            c.appearance.shape = (*shape).into();
            let center = c.center(Target::Stick, size);
            let d = c.diameter(Target::Stick, size);
            assert_eq!(
                c.hit(center, size, true, false),
                Some(Target::Stick),
                "{shape}"
            );
            // Preserve the existing circular 0.65D grab padding, not a square corner.
            assert_eq!(
                c.hit(center + Vec2::new(d * 0.60, 0.0), size, true, false),
                Some(Target::Stick),
                "{shape}"
            );
            assert_eq!(
                c.hit(center + Vec2::splat(d * 0.48), size, true, false),
                None,
                "{shape}"
            );
        }
    }
    #[test]
    fn all_registered_auxiliary_actions_keep_custom_positions_and_visibility() {
        for (preset, _) in super::super::profiles::PRESETS {
            let mut c = Config::default();
            for p in &mut c.buttons[9..] {
                p.center = Vec2::new(0.42, 0.31);
                p.scale = 1.3;
                p.visible = false;
            }
            let before = c.buttons[9..].to_vec();
            c.apply_preset(preset);
            assert_eq!(
                c.buttons[9..],
                before,
                "{preset}: menus and any registered Gyro ID18"
            );
        }
    }
    #[test]
    fn thumb_and_index_finger_templates_have_distinct_deliberate_zones() {
        let mut thumbs = Config::default();
        thumbs.apply_preset("explorer");
        let mut combat = Config::default();
        combat.apply_preset("combat");
        let mut claw = Config::default();
        claw.apply_preset("claw");
        let mut left = Config::default();
        left.apply_preset("left");
        assert!(thumbs.buttons[7].center.x > 0.9 && thumbs.buttons[7].center.y > 0.8);
        assert!(combat.buttons[7].scale > combat.buttons[4].scale);
        assert!(claw.buttons[7].center.x < 0.2 && claw.buttons[7].center.y < 0.3);
        assert!(claw.buttons[6].center.x > 0.8 && claw.buttons[6].center.y < 0.3);
        for i in 0..9 {
            assert_eq!(
                left.buttons[i].center,
                Vec2::new(1.0 - thumbs.buttons[i].center.x, thumbs.buttons[i].center.y)
            );
        }
        for c in [&thumbs, &combat, &claw] {
            let size = Vec2::new(640.0, 360.0);
            let look = Vec2::new(0.58, 0.42);
            assert!(c.look_area(look));
            assert_eq!(c.hit(look * size, size, true, false), None);
            assert!(c.buttons[..9].iter().all(|p| p.visible));
        }
    }
    #[test]
    fn explicit_templates_avoid_each_other_utilities_and_a_24_pixel_bottom_inset() {
        let overlaps = |a: Rect, b: Rect| {
            a.min.x < b.max.x && a.max.x > b.min.x && a.min.y < b.max.y && a.max.y > b.min.y
        };
        for size in [
            Vec2::new(640.0, 360.0),
            Vec2::new(792.0, 363.0),
            Vec2::new(1280.0, 720.0),
        ] {
            for (preset, _) in super::super::profiles::PRESETS {
                if *preset == "classic" {
                    continue;
                } // Existing default is preserved, not silently repositioned.
                let mut c = Config::default();
                c.apply_preset(preset);
                let mut controls = Vec::new();
                for i in 0..9 {
                    let target = Target::Button(i);
                    let d = c.diameter(target, size);
                    let rect = Rect::from_center_size(c.center(target, size), Vec2::splat(d));
                    assert!(rect.max.y <= size.y - 24.0, "{preset}/{i} {size:?}");
                    for help in [false, true] {
                        assert!(
                            !overlaps(rect, utility_rect(help, size)),
                            "utility {preset}/{i} {size:?}"
                        );
                    }
                    for (j, other) in &controls {
                        assert!(!overlaps(rect, *other), "buttons {preset}/{i}/{j} {size:?}");
                    }
                    controls.push((i, rect));
                }
                let grab = c.diameter(Target::Stick, size) * 0.65;
                let stick =
                    Rect::from_center_size(c.center(Target::Stick, size), Vec2::splat(grab * 2.0));
                assert!(stick.max.y <= size.y - 24.0, "stick {preset} {size:?}");
                for (i, button) in controls {
                    assert!(!overlaps(stick, button), "stick/{i} {preset} {size:?}");
                }
            }
        }
    }
    #[test]
    fn new_templates_never_reapply_themselves_when_loading_saved_coordinates() {
        let mut c = Config {
            preset_name: "claw".into(),
            ..Config::default()
        };
        c.buttons[7].center = Vec2::new(0.63, 0.58);
        c.stick.center = Vec2::new(0.22, 0.71);
        let saved = c.encode();
        let loaded = Config::decode(&saved).unwrap();
        assert_eq!(loaded, c);
        let classic = Config::default();
        let mut selected = classic.clone();
        selected.apply_preset("classic");
        assert_eq!(classic, selected);
    }
    #[test]
    fn older_slots_keep_actions_and_settings_added_after_the_slot_was_saved() {
        let mut c = Config::default();
        c.store_slot(0).unwrap();
        c.extra["buttons"]["NewAction"] = json!({"x":0.4,"y":0.5,"keep":true});
        c.extra["later_feature"] = json!({"enabled":true});
        let future = c.extra["buttons"]["NewAction"].clone();
        c.load_slot(0).unwrap();
        assert_eq!(c.encode()["buttons"]["NewAction"], future);
        assert_eq!(c.encode()["later_feature"]["enabled"], true);
    }
    #[test]
    fn future_actions_and_fields_survive_presets_and_roundtrip() {
        let value = json!({"version":1,"future":{"keep":true},"buttons":{"FutureGyro":{"x":0.4,"future":9},"Fire":{"future":7}},"aiming":{"future_gyro_metadata":{"keep":true}},"appearance":{"theme":"unknown-theme","shape":"unknown-shape","future":3}});
        let mut config = Config::decode(&value).unwrap();
        config.apply_preset("left");
        let next = config.encode();
        assert_eq!(
            next["buttons"]["FutureGyro"],
            value["buttons"]["FutureGyro"]
        );
        assert_eq!(next["buttons"]["Fire"]["future"], 7);
        assert_eq!(
            next["aiming"]["future_gyro_metadata"],
            value["aiming"]["future_gyro_metadata"]
        );
        assert_eq!(next["future"], value["future"]);
        for key in ["theme", "shape", "future"] {
            assert_eq!(next["appearance"][key], value["appearance"][key]);
        }
    }
    #[test]
    fn saved_slots_capture_appearance_without_recursive_libraries() {
        let mut c = Config::default();
        c.appearance.next_theme();
        c.appearance.next_shape();
        let saved = c.clone();
        c.store_slot(0).unwrap();
        c.apply_preset("claw");
        c.appearance.next_theme();
        c.store_slot(1).unwrap();
        c.load_slot(0).unwrap();
        assert_eq!(c.buttons, saved.buttons);
        assert_eq!(c.appearance, saved.appearance);
        assert!(c.encode()["profiles"][0]["layout"]
            .get("profiles")
            .is_none());
        assert_eq!(c.encode()["profiles"].as_array().unwrap().len(), 2);
        c.rename_slot(0).unwrap();
        assert_eq!(c.slot_name(0), Some("My layout"));
        assert!(c.store_slot(super::super::profiles::MAX_SLOTS).is_err());
        assert_eq!(Config::decode(&c.encode()).unwrap(), c);
    }
    #[test]
    fn unknown_slot_versions_and_unknown_disk_versions_are_kept() {
        let mut c=Config::decode(&json!({"version":1,"profiles":[{"name":"Future","layout":{"version":99,"keep":true}}]})).unwrap();
        let old = c.encode();
        assert!(c.load_slot(0).is_err());
        assert!(c.store_slot(0).is_err());
        assert_eq!(c.encode(), old);
        let root = std::env::temp_dir().join(format!("nvf-touch-future-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("touch-controls.json");
        let bytes = b"{\"version\":99,\"keep\":true}";
        std::fs::write(&path, bytes).unwrap();
        assert!(Config::default().save_to(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn preset_controls_keep_menu_customizations_and_do_not_overlap_on_phone() {
        let size = Vec2::new(792.0, 363.0);
        for (name, _) in super::super::profiles::PRESETS {
            let mut c = Config::default();
            c.buttons[9].center = Vec2::new(0.4, 0.4);
            c.aiming.look = 2.0;
            c.apply_preset(name);
            assert_eq!(c.buttons[9].center, Vec2::new(0.4, 0.4));
            assert_eq!(c.aiming.look, 2.0);
            for i in 0..9 {
                for j in i + 1..9 {
                    let a = c.center(Target::Button(i), size);
                    let b = c.center(Target::Button(j), size);
                    assert!(
                        a.distance(b) + 0.01
                            >= (c.diameter(Target::Button(i), size)
                                + c.diameter(Target::Button(j), size))
                                * 0.5,
                        "{name}: {i}/{j}"
                    );
                }
            }
        }
    }
    #[test]
    fn saved_layout_roundtrips_and_clamps_untrusted_values() {
        let mut c = Config::preset(true);
        c.buttons[3].center = Vec2::new(0.4, 0.3);
        c.buttons[4].visible = false;
        c.floating = false;
        assert_eq!(Config::decode(&c.encode()).unwrap(), c);
        let mut v = c.encode();
        v["buttons"]["Fire"]["scale"] = json!(999);
        v["opacity"] = json!(-5);
        v["stick"]["x"] = json!(-7);
        let c = Config::decode(&v).unwrap();
        assert_eq!(c.buttons[7].scale, 1.8);
        assert_eq!(c.opacity, 0.3);
        assert_eq!(c.stick.center.x, 0.0);
        assert!(Config::decode(&json!({"version":99})).is_err());
    }
    #[test]
    fn saving_twice_replaces_only_the_owned_settings_file() {
        let root = std::env::temp_dir().join(format!("nvf-touch-layout-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("touch-controls.json");
        let first = Config::default();
        first.save_to(&path).unwrap();
        let mut next = Config::preset(true);
        next.opacity = 0.4;
        next.save_to(&path).unwrap();
        let value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(Config::decode(&value).unwrap(), next);
        assert!(!path.with_extension("json.tmp").exists());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn visual_centers_and_input_targets_match_on_a_landscape_phone() {
        let size = Vec2::new(792.0, 363.0);
        for southpaw in [false, true] {
            let c = Config::preset(southpaw);
            for (i, b) in BUTTONS.iter().enumerate() {
                assert_eq!(
                    c.hit(c.center(Target::Button(i), size), size, b.playing, false),
                    Some(Target::Button(i))
                );
                assert!(c.diameter(Target::Button(i), size) >= 48.0);
            }
            assert!(c.movement_area(c.stick.center));
            assert!(!c.look_area(c.stick.center));
        }
    }
}
