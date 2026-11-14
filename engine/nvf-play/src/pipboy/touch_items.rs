//! Optional mobile pointer/assignment bridge. No resource on the desktop path.
use crate::hotkeys::{AMMO_SWAP, Hotkeys, SLOTS, Slot};
use crate::menu::{
    ActiveMenu,
    model::{Mode, RowGuard, RowKey, Side},
};
use bevy::asset::uuid::Uuid;
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use std::collections::VecDeque;

/// Rechecked after device switching, at the actual picking boundary.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Admission<'w> {
    bindings: Option<Res<'w, crate::bindings::Bindings>>,
    platform: Option<Res<'w, crate::pad::PlatformInputGate>>,
    switching: Option<Res<'w, crate::pad::TouchDeviceSwitch>>,
    capture: Option<Res<'w, crate::bindings::KeyboardCapture>>,
    camera: Option<Res<'w, crate::free_cam::FreeCam>>,
}
impl Admission<'_> {
    pub fn allows(&self) -> bool {
        self.bindings
            .as_ref()
            .is_none_or(|b| b.device != crate::bindings::InputDevice::Controller)
            && self.platform.as_ref().is_none_or(|p| p.allows_actions())
            && self.switching.as_ref().is_none_or(|s| s.allows_actions())
            && self.capture.as_ref().is_none_or(|c| !c.0)
            && self.camera.as_ref().is_none_or(|c| !c.active)
    }
}

pub const POINTER: bevy::picking::pointer::PointerId =
    bevy::picking::pointer::PointerId::Custom(Uuid::from_u128(0x6e76665f746f7563685f706970726f77));
#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub phase: TouchPhase,
    pub position: Vec2,
}
#[derive(Clone, Copy, Debug)]
struct Contact {
    id: u64,
    origin: Vec2,
    at: Vec2,
    started: u64,
    ended: bool,
    moved: bool,
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
    pub message: String,
}
impl TouchItems {
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
            });
            self.row = None;
            self.drop_slot = None;
            self.suppress = false;
            self.points.clear();
        }
        let Some(c) = self.contact.as_mut().filter(|c| c.id == event.id) else {
            return false;
        };
        if event.phase == TouchPhase::Canceled || !event.position.is_finite() {
            self.cancel();
            return true;
        }
        c.at = event.position;
        c.moved |= c.at.distance(c.origin) > 12.;
        c.ended |= event.phase == TouchPhase::Ended;
        self.suppress |= c.moved || now.saturating_sub(c.started) >= 450;
        let point = Point {
            phase: event.phase,
            position: event.position,
        };
        // Keep press/release, coalesce motion, and project one point per frame so a
        // fast release cannot relocate the pointer before its press has hit a row.
        if event.phase == TouchPhase::Moved
            && self
                .points
                .back()
                .is_some_and(|p| p.phase == TouchPhase::Moved)
        {
            self.points.pop_back();
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
        }
    }
    pub fn project(&mut self, mut map: impl FnMut(Vec2) -> Option<Vec2>) {
        if self.projected.is_none() {
            if let Some(p) = self.points.pop_front() {
                self.projected = Some((p, map(p.position)));
            }
        }
    }
    pub fn take_projected(&mut self) -> Option<(Point, Option<Vec2>)> {
        self.projected.take()
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
        self.drop_slot = slot.filter(|&n| n < SLOTS);
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
