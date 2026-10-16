//! The Pip-Boy's own sounds (docs/spec-notes/pipboy.md §6.1; the owner's playtest list 5,
//! items 14, 54, 55 and 56), played at the listener by their SOUN records [observed:
//! FalloutNV.esm, the records' files as the owner names them]:
//!
//! - another tab of a page: `UIPipBoyTab` (`fx\ui\pipboy\ui_pipboy_tab.wav`);
//! - another page (the three buttons on the device, F1–F3): `UIPipBoyMode`
//!   (`ui_pipboy_mode.wav`);
//! - the pointer coming onto a tab of the tab line: `UIMenuFocus`
//!   (`fx\ui\menu\ui_menu_focus.wav`); the ITEMS list's rows already sound as the inventory
//!   menu's (`crate::ui_sounds`, `UIPipBoyScroll`, the same file);
//! - while the Pip-Boy is up, now and then one of the static crackles: `UIPipBoyStatic`
//!   (the folder `fx\ui\pipboy\static\`, a file picked at random each time by
//!   `crate::sfx`), first [`STATIC_FIRST`] after it is up and then every
//!   [`STATIC_EVERY`] [hypothesis: the owner asked for "a random sound from static" while
//!   it is up; how often the original plays one is not captured];
//! - the Pip-Boy light (`crate::flashlight`) switched: `UIPipBoyLightOn` / `UIPipBoyLightOff`.
//!
//! Each is logged ("pipboy sound: …").

use super::{Page, Phase, PipBoy};
use crate::flashlight::Flashlight;
use crate::sfx::{PlaySound, Sfx};
use bevy::prelude::*;
use fnv_sim::FormId;
use std::collections::HashMap;

/// Seconds from the Pip-Boy being up to the first static, and between statics after
/// (each a random time in the range) [hypothesis].
pub const STATIC_FIRST: (f32, f32) = (0.5, 2.0);
pub const STATIC_EVERY: (f32, f32) = (4.0, 10.0);

/// The pointer came onto a tab of the tab line (sent by the tab's node).
#[derive(Message, Clone, Copy, Debug)]
pub struct TabHovered(pub usize);

/// What the sounds watch.
#[derive(Default)]
pub struct Watch {
    shown: Option<(Page, usize)>,
    up: bool,
    next_static: Option<f32>,
    light: Option<u32>,
    rng: u64,
    sounds: Option<HashMap<String, FormId>>,
}

impl Watch {
    /// A number in `[lo, hi)`.
    fn between(&mut self, (lo, hi): (f32, f32)) -> f32 {
        self.rng = self.rng.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        lo + (hi - lo) * ((self.rng >> 40) as f32 / (1u64 << 24) as f32)
    }
}

/// What changed from one frame to the next: the sounds to play, by Editor ID.
pub fn changes(before: Option<(Page, usize)>, now: Option<(Page, usize)>) -> Option<&'static str> {
    match (before, now) {
        (Some((pa, _)), Some((pb, _))) if pa != pb => Some("UIPipBoyMode"),
        (Some((_, ta)), Some((_, tb))) if ta != tb => Some("UIPipBoyTab"),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn play(
    pip: Res<PipBoy>,
    light: Option<Res<Flashlight>>,
    time: Res<Time<Real>>,
    sfx: Option<Res<Sfx>>,
    mut hovered: MessageReader<TabHovered>,
    mut out: MessageWriter<PlaySound>,
    mut w: Local<Watch>,
) {
    let mut want: Vec<&'static str> = Vec::new();
    let now = pip.open().then(|| (pip.page, pip.tab()));
    want.extend(changes(w.shown, now));
    w.shown = now;
    for h in hovered.read() {
        if pip.open() && h.0 != pip.tab() {
            want.push("UIMenuFocus");
        }
    }
    // Static while up.
    let t = time.elapsed_secs();
    let up = pip.phase == Phase::Up;
    if up && !w.up {
        if w.rng == 0 {
            w.rng = time.elapsed().as_nanos() as u64 | 1;
        }
        let d = w.between(STATIC_FIRST);
        w.next_static = Some(t + d);
    }
    w.up = up;
    if !up {
        w.next_static = None;
    } else if w.next_static.is_some_and(|at| t >= at) {
        want.push("UIPipBoyStatic");
        let d = w.between(STATIC_EVERY);
        w.next_static = Some(t + d);
    }
    // The light.
    if let Some(l) = light {
        if w.light.is_some_and(|n| n != l.toggles) {
            want.push(if l.on { "UIPipBoyLightOn" } else { "UIPipBoyLightOff" });
        }
        w.light = Some(l.toggles);
    }
    if want.is_empty() {
        return;
    }
    let Some(sfx) = sfx else { return };
    let sounds = w.sounds.get_or_insert_with(|| sfx.forms.sounds.iter().filter_map(|(&id, s)| s.edid.clone().map(|e| (e, id))).collect());
    for name in want {
        match sounds.get(name) {
            Some(&sound) => {
                if name == "UIMenuFocus" {
                    debug!("pipboy sound: {name}");
                } else {
                    info!("pipboy sound: {name}");
                }
                out.write(PlaySound { sound, at: None });
            }
            None => warn!("pipboy sound: no SOUN {name}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_sound_the_mode_and_tabs_the_tab() {
        let s = |p, t| Some((p, t));
        assert_eq!(changes(s(Page::Items, 0), s(Page::Stats, 0)), Some("UIPipBoyMode"));
        assert_eq!(changes(s(Page::Items, 0), s(Page::Stats, 3)), Some("UIPipBoyMode"));
        assert_eq!(changes(s(Page::Items, 0), s(Page::Items, 1)), Some("UIPipBoyTab"));
        assert_eq!(changes(s(Page::Items, 1), s(Page::Items, 1)), None);
        // Coming up and going down are the access sounds' (crate::ui_sounds).
        assert_eq!(changes(None, s(Page::Items, 1)), None);
        assert_eq!(changes(s(Page::Items, 1), None), None);
    }

    #[test]
    fn statics_come_within_their_range() {
        let mut w = Watch { rng: 12345, ..Default::default() };
        for _ in 0..100 {
            let d = w.between(STATIC_EVERY);
            assert!((STATIC_EVERY.0..STATIC_EVERY.1).contains(&d));
        }
    }
}
