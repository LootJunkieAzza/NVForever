//! Optional mobile pointer/assignment bridge. No resource on the desktop path.
use super::touch_scroll::{gesture::Intent, PointerFrame};
use crate::hotkeys::{Hotkeys, Slot, AMMO_SWAP, SLOTS};
use crate::menu::{
    model::{Mode, RowGuard, RowKey, Side},
    ActiveMenu,
};
use bevy::asset::uuid::Uuid;
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use std::collections::VecDeque;

/// Wrapper-owned touch UI capture; absent keeps desktop/headless behavior unchanged.
#[derive(Resource, Default)]

pub struct OverlayCapture(pub bool);

/// Rechecked after device switching, at the actual picking boundary.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Admission<'w> {
    bindings: Option<Res<'w, crate::bindings::Bindings>>,
    platform: Option<Res<'w, crate::pad::PlatformInputGate>>,
    switching: Option<Res<'w, crate::pad::TouchDeviceSwitch>>,
    capture: Option<Res<'w, crate::bindings::KeyboardCapture>>,
    camera: Option<Res<'w, crate::free_cam::FreeCam>>,
    overlay: Option<Res<'w, OverlayCapture>>,
    fresh: Option<Res<'w, crate::menu::FreshMenu>>,

}
impl Admission<'_> {
    pub fn allows_contact(&self, id: Option<u64>) -> bool {
        self.allows()
            && id.is_none_or(|id| self.platform.as_ref().is_none_or(|g| g.allows_touch(id)))
    }
    pub fn allows(&self) -> bool {
        self.bindings
            .as_ref()
            .is_none_or(|b| b.device != crate::bindings::InputDevice::Controller)
            && self.platform.as_ref().is_none_or(|p| p.allows_actions())
            && self.switching.as_ref().is_none_or(|s| s.allows_actions())
            && self.capture.as_ref().is_none_or(|c| !c.0)
            && self.camera.as_ref().is_none_or(|c| !c.active)
            && self.overlay.as_ref().is_none_or(|c| !c.0)
            && self.fresh.as_ref().is_none_or(|f| !f.0)
    }
}

pub const POINTER: bevy::picking::pointer::PointerId =
    bevy::picking::pointer::PointerId::Custom(Uuid::from_u128(0x6e76665f746f7563685f706970726f77));
#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub phase: TouchPhase,
    pub position: Vec2,
    previous: Option<Vec2>,
    scrolling: bool,
}
#[derive(Clone, Copy, Debug)]
struct Contact {
    id: u64,
    origin: Vec2,
    at: Vec2,
    started: u64,
    ended: bool,
    moved: bool,
    intent: Intent,
}

#[derive(Clone, Copy, PartialEq)]
struct Session {
    page: super::Page,
    tab: usize,
    frame: super::Frame,
    model: bool,
    image: bevy::asset::AssetId<Image>,
    scale: u32,
}
#[derive(Resource, Default)]
pub struct TouchItems {
    contact: Option<Contact>,
    points: VecDeque<Point>,
    projected: Option<(Point, Option<Vec2>)>,
    row: Option<RowGuard>,
    drop_slot: Option<usize>,
    suppress: bool,
    cancel_pointer: bool,
    projected_scroll: Option<(Vec2, f32)>,
    scroll_remainder: f32,
    frame: PointerFrame,
    native_owner: Option<bool>,
    native_target: Option<super::touch_scroll::NativeTarget>,
    session: Option<Session>,
    flat_geometry: Option<(Vec2, Vec2)>,
    pub message: String,
}
impl TouchItems {
    pub fn begin_frame(&mut self) {
        self.frame = PointerFrame::default();
    }
    pub fn pointer_frame(&self) -> PointerFrame {
        self.frame
    }
    pub fn route_native(&mut self, target: Option<super::touch_scroll::NativeTarget>) -> bool {
        if self.frame.started {
            self.native_owner = Some(target.is_some());
            self.native_target = target;
        }
        let owner = self.native_owner.unwrap_or(false);
        self.frame.native = owner && target.is_some();
        self.frame.initially_selected = self.native_target.and_then(|t| t.selected);
        self.frame.tap &= self.native_release_valid(target);
        owner
    }
    pub fn native_release_valid(&self, target: Option<super::touch_scroll::NativeTarget>) -> bool {
        self.native_target
            .zip(target)
            .is_some_and(|(old, now)| old.same_target(&now))
    }
    pub fn take_scroll(&mut self) -> Option<(Vec2, f32)> {
        self.projected_scroll.take()
    }
    pub fn session(&mut self, pip: &super::PipBoy, screen: &super::PipBoyScreen) {
        let next = Session {
            page: pip.page,
            tab: pip.tab(),
            frame: pip.frame,
            model: screen.on_model,
            image: screen.image.id(),
            scale: screen.scale.to_bits(),
        };
        if pip.phase != super::Phase::Up || self.session.is_some_and(|old| old != next) {
            self.cancel();
        }
        self.session = Some(next);
    }
    pub fn flat_geometry(&mut self, size: Vec2, center: Vec2) {
        let next = Some((size, center));
        if self.flat_geometry.is_some() && self.flat_geometry != next {
            self.cancel();
        }
        self.flat_geometry = next;
    }

    pub fn contact_id(&self) -> Option<u64> {
        self.contact.map(|c| c.id)
    }
    pub fn owns(&self, id: u64) -> bool {
        self.contact.is_some_and(|c| c.id == id)
    }
    pub fn busy(&self) -> bool {
        self.contact.is_some_and(|c| !c.ended)
    }
    pub fn position(&self) -> Option<Vec2> {
        self.contact.map(|c| c.at)
    }
    pub fn cancel(&mut self) {
        self.cancel_pointer |= self.contact.is_some();
        self.contact = None;
        self.points.clear();
        self.projected = None;
        self.projected_scroll = None;
        self.scroll_remainder = 0.0;
        self.frame = PointerFrame::default();
        self.native_owner = None;
        self.native_target = None;
        self.row = None;
        self.drop_slot = None;
        self.suppress = true;
    }
    pub fn event(&mut self, event: &TouchInput, now: u64) -> bool {
        if event.phase == TouchPhase::Started {
            if self.busy() || !event.position.is_finite() {
                return false;
            }
            self.cancel_pointer |= self.contact.is_some();
            self.contact = Some(Contact {
                id: event.id,
                origin: event.position,
                at: event.position,
                started: now,
                ended: false,
                moved: false,
                intent: Intent::default(),
            });
            self.row = None;
            self.drop_slot = None;
            self.suppress = false;
            self.points.clear();
            self.projected = None;
            self.projected_scroll = None;
            self.scroll_remainder = 0.0;
            self.frame = PointerFrame::default();
            self.native_owner = None;
            self.native_target = None;
        }
        let Some(c) = self.contact.as_mut().filter(|c| c.id == event.id) else {
            return false;
        };
        if event.phase == TouchPhase::Canceled || !event.position.is_finite() {
            self.cancel();
            return true;
        }
        let previous = c.at;
        c.at = event.position;
        c.moved |= c.at.distance(c.origin) > 12.;
        c.intent
            .observe((c.at - c.origin).to_array(), now.saturating_sub(c.started));
        c.ended |= event.phase == TouchPhase::Ended;
        self.suppress |= c.moved || now.saturating_sub(c.started) >= 450;
        let mut point = Point {
            phase: event.phase,
            position: event.position,
            previous: Some(previous),
            scrolling: c.intent == Intent::Scroll,
        };
        // Keep press/release, coalesce motion, and project one point per frame so a
        // fast release cannot relocate the pointer before its press has hit a row.
        if event.phase == TouchPhase::Moved
            && self
                .points
                .back()
                .is_some_and(|p| p.phase == TouchPhase::Moved)
        {
            if let Some(old) = self.points.pop_back() {
                point.previous = old.previous;
            }
        }
        if self.points.len() >= 4 {
            self.cancel();
            return true;
        }
        self.points.push_back(point);
        true
    }
    pub fn update_hold(&mut self, now: u64) {
        if self
            .contact
            .is_some_and(|c| now.saturating_sub(c.started) >= 450)
        {
            self.suppress = true;
            if let Some(c) = self.contact.as_mut() {
                c.intent
                    .observe((c.at - c.origin).to_array(), now.saturating_sub(c.started));
            }
        }
    }
    pub fn project(&mut self, mut map: impl FnMut(Vec2) -> Option<Vec2>) {
        if self.projected.is_none() {
            if let Some(p) = self.points.pop_front() {
                let at = map(p.position);
                self.projected_scroll = if p.scrolling {
                    at.zip(p.previous.and_then(&mut map))
                        .zip(self.contact.and_then(|c| map(c.origin)))
                        .map(|((now, before), anchor)| {
                            (anchor, (now.y - before.y).clamp(-256.0, 256.0))
                        })
                } else {
                    None
                };
                if let Some((_, pixels)) = self.projected_scroll.as_mut() {
                    let total = *pixels + self.scroll_remainder;
                    let whole = (total / 20.0).trunc().clamp(-12.0, 12.0) * 20.0;
                    self.scroll_remainder = (total - whole).clamp(-19.999, 19.999);
                    *pixels = whole;
                }
                self.projected = Some((p, at));
            }
        }
    }
    pub fn take_projected(&mut self) -> Option<(Point, Option<Vec2>)> {
        let projected = self.projected.take();
        if let Some((p, at)) = projected {
            self.frame = PointerFrame {
                at: self.projected_scroll.map(|(anchor, _)| anchor).or(at),
                started: p.phase == TouchPhase::Started && at.is_some(),
                tap: p.phase == TouchPhase::Ended && !self.suppress && at.is_some(),
                held: self.busy(),
                scrolling: p.scrolling,
                native: false,
                initially_selected: None,
            };
        }
        projected
    }
    pub fn take_cancel(&mut self) -> bool {
        std::mem::take(&mut self.cancel_pointer)
    }
    pub fn suppress_click(&self) -> bool {
        self.suppress
    }
    pub fn capture_row(&mut self, menu: &crate::menu::model::MenuModel, index: usize) {
        if self.contact.is_none() || menu.mode != Mode::Inventory || menu.quantity.is_some() {
            return;
        }
        if let Some(key) = RowKey::at(&menu.list(Side::Player).rows, index) {
            self.row = Some(RowGuard {
                mode: menu.mode,
                side: Side::Player,
                index,
                key,
                selected: false,
                then: 0,
            });
        }
    }
    pub fn release_over(&mut self, slot: Option<usize>) {
        self.drop_slot =
            slot.filter(|&n| n < SLOTS && self.contact.is_some_and(|c| c.intent == Intent::Assign));
        if self.drop_slot.is_some() {
            self.suppress = true;
        }
    }
    pub fn assign_pending(&mut self, active: &ActiveMenu, hotkeys: &mut Hotkeys) -> bool {
        if !self.contact.is_some_and(|c| c.ended) {
            return false;
        }
        // Wait for the actual row's PointerPress observer when press/end arrived
        // together. A later menu cannot reuse this pending operation: the wrapper
        // cancels it on its menu/session boundary.
        if self.row.is_none() && (!self.points.is_empty() || self.projected.is_some()) {
            return false;
        }
        let slot = self.drop_slot.take();
        let guard = self.row.take();
        let Some(slot) = slot else {
            return false;
        };
        let Some((menu, guard)) = active.0.as_ref().zip(guard).filter(|(m, g)| m.row_is(g)) else {
            self.message = "Inventory changed; drag again.".into();
            return false;
        };
        let row = &menu.list(Side::Player).rows[guard.index];
        if slot == AMMO_SWAP || row.count <= 0 || row.live || row.folder.is_some() {
            self.message = "This item cannot be assigned to that slot.".into();
            return false;
        }
        match hotkeys.assign(
            slot,
            Slot {
                item: row.item.0,
                mods: row.mods,
            },
            row.kind,
        ) {
            Ok(_) => {
                self.message = format!("Slot {}: {}", slot + 1, row.name);
                true
            }
            Err(_) => {
                self.message = "This item cannot be assigned to that slot.".into();
                false
            }
        }
    }
}

#[cfg(test)]
mod overlay_tests {
    use super::*;
    #[derive(Resource, Default)]
    struct Seen(bool);
    fn read(admission: Admission, mut seen: ResMut<Seen>) {
        seen.0 = admission.allows();
    }
    #[test]
    fn wrapper_touch_overlay_capture_is_optional_and_blocks_admission_without_hardware_mutation() {
        let mut app = App::new();
        app.init_resource::<Seen>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, read);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.update();
        assert!(app.world().resource::<Seen>().0);
        app.insert_resource(OverlayCapture(true));
        app.update();
        assert!(!app.world().resource::<Seen>().0);
        assert!(app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::KeyW));
        app.world_mut().resource_mut::<OverlayCapture>().0 = false;
        app.update();
        assert!(app.world().resource::<Seen>().0);
        app.world_mut().remove_resource::<OverlayCapture>();
        app.update();
        assert!(app.world().resource::<Seen>().0);
    }
}

#[cfg(test)]
mod scroll_projection_tests {
    use super::*;
    fn e(phase: TouchPhase, position: Vec2) -> TouchInput {
        TouchInput {
            phase,
            position,
            window: Entity::PLACEHOLDER,
            id: 3,
            force: None,
        }
    }
    #[test]
    fn queued_press_motion_release_keep_order_and_scroll_uses_the_actual_projection() {
        let mut touch = TouchItems::default();
        touch.event(&e(TouchPhase::Started, Vec2::new(20., 100.)), 0);
        touch.event(&e(TouchPhase::Moved, Vec2::new(20., 80.)), 20);
        touch.event(&e(TouchPhase::Moved, Vec2::new(20., 60.)), 40);
        touch.event(&e(TouchPhase::Ended, Vec2::new(20., 60.)), 60);
        for (phase, scroll) in [
            (TouchPhase::Started, None),
            (TouchPhase::Moved, Some(-80.)),
            (TouchPhase::Ended, Some(0.)),
        ] {
            touch.begin_frame();
            touch.project(|p| Some(p * 2.0));
            assert_eq!(touch.take_projected().unwrap().0.phase, phase);
            assert_eq!(touch.take_scroll().map(|(_, y)| y), scroll);
            assert!(!touch.pointer_frame().tap);
        }
        touch.project(Some);
        assert!(touch.take_projected().is_none());
        assert!(touch.take_scroll().is_none());
    }
    #[test]
    fn geometry_change_cancels_projected_press_and_partial_scroll_cannot_cross_contacts() {
        let mut t = TouchItems::default();
        t.flat_geometry(Vec2::splat(500.), Vec2::splat(250.));
        t.event(&e(TouchPhase::Started, Vec2::new(20., 100.)), 0);
        t.project(Some);
        assert!(t.take_projected().is_some());
        t.event(&e(TouchPhase::Moved, Vec2::new(20., 84.)), 20);
        t.project(Some);
        t.take_projected();
        assert_eq!(t.take_scroll().unwrap().1, 0.);
        t.flat_geometry(Vec2::splat(600.), Vec2::splat(300.));
        assert!(t.take_cancel());
        assert!(!t.busy());
        assert!(t.take_projected().is_none());
        t.event(&e(TouchPhase::Started, Vec2::new(20., 100.)), 100);
        t.project(Some);
        t.take_projected();
        t.event(&e(TouchPhase::Moved, Vec2::new(20., 84.)), 120);
        t.project(Some);
        t.take_projected();
        assert_eq!(
            t.take_scroll().unwrap().1,
            0.,
            "old16px remainder cannot make a new contact scroll"
        );

    }
}
