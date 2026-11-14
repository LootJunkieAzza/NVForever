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
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub buttons: Vec<Placement>,
    pub stick: Placement,
    pub floating: bool,
    pub southpaw: bool,
    pub opacity: f32,
    pub hints: bool,
    pub aiming: super::aiming::Aiming,
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
        }
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
        let mut nearest = None;
        let mut distance = f32::MAX;
        for (i, b) in BUTTONS.iter().enumerate() {
            let t = Target::Button(i);
            if b.playing != playing || (!editing && !self.buttons[i].visible) {
                continue;
            }
            let d = point.distance(self.center(t, size));
            if d <= self.diameter(t, size) * 0.5 && d < distance {
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
        let place = |p: &Placement| json!({"x":p.center.x,"y":p.center.y,"scale":p.scale,"visible":p.visible});
        json!({"version":1,"buttons":BUTTONS.iter().zip(&self.buttons).map(|(b,p)| (b.text.to_string(),place(p))).collect::<serde_json::Map<_,_>>(),"stick":place(&self.stick),"floating":self.floating,"southpaw":self.southpaw,"opacity":self.opacity,"hints":self.hints,"aiming":self.aiming.encode()})
    }
    pub fn decode(v: &Value) -> Result<Self, String> {
        if v["version"].as_u64() != Some(1) {
            return Err("Unknown touch-layout version".into());
        }
        let mut c = Self::default();
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
        let bytes = serde_json::to_vec_pretty(&self.encode()).map_err(|e| e.to_string())?;
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
    drag: Option<(u64, Vec2)>,
}
#[derive(Resource, Default)]
pub struct Layout {
    pub config: Config,
    pub editor: Option<Editor>,
    pub message: String,
}
pub const EDIT_ACTIONS: [&str; 12] = [
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
];
pub fn editor_rect(index: usize, size: Vec2) -> Rect {
    let width = (size.x * 0.4).max(288.0).min(size.x * 0.75);
    let h = 52.0_f32.min(size.y * 0.15);
    let x = (size.x - width) * 0.5 + (index % 3) as f32 * width / 3.0;
    let y = (size.y - h * 4.0) * 0.5 + (index / 3) as f32 * h;
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
                if let Some(action) = (0..EDIT_ACTIONS.len()).find(|&i| {
                    let rect = if e.aiming {
                        super::aiming::editor_rect(i, size)
                    } else {
                        editor_rect(i, size)
                    };
                    rect.contains(t.position)
                }) {
                    // A toolbar tap ends the previous finger's drag before changing its target/layout.
                    e.drag = None;
                    if e.aiming && action < 9 {
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
                                self.config = Config::preset(self.config.southpaw);
                                self.config.aiming = aiming;
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
                            _ => {
                                e.aiming = !e.aiming;
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
                    .filter(|_| !e.aiming)
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
