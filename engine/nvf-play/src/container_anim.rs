//! Containers whose model opens and closes (a locker, a footlocker, a safe, a toolbox: a
//! CONT whose model has "Open" and "Close" sequences; playtest list 5 item 65). They are
//! drawn and timed as doors that open in place are (nvforever's `DoorRig`,
//! [`crate::doors::Doors`]), so their sequences' `Sound:` text keys play where they stand.
//!
//! - **Opening**: a request to open the container menu on such a container
//!   ([`crate::OpenContainer`], from activation, a script, a lock picked) first plays its
//!   "Open" sequence; the menu opens when the sequence ends (`end` text key)
//!   [observed: owner, playtest list 5 item 65 — in the game the lid or door opens before
//!   the menu appears; the sequences are 0.13–0.5 s long: `fnvtool nif decode` of
//!   `locker01.nif`, `footlocker01.nif`, `toolbox01.nif`, `genericsafe01.nif`].
//! - **Closing**: when the container menu on it closes, its "Close" sequence plays
//!   [hypothesis: as the open; the owner reports the close sound in the game].
//! - **Sounds**: these CONT records carry no open or close sound (SNAM/QNAM) of their own
//!   [observed: `fnvtool esm decode` of Locker01School, Footlocker01FactoryNV,
//!   VLootSafeTier1WastelandLock, VLootToolsToolboxAbandonedLow]: their sounds are the
//!   sequences' keys (`DRSLocker01Open`, `DRSSafeOpen`, …), heard through the door
//!   sounds. A container that is not drawn (or has no sequences) opens at once, as before.
//!
//! Log: `container 0x… plays Open before its menu` and `container 0x… closes (menu
//! closed)`.

use crate::doors::Doors;
use crate::menu::model::Mode;
use crate::menu::{ActiveMenu, MenuSystems};
use crate::{OpenContainer, PlaySet};
use bevy::prelude::*;
use fnv_sim::FormId;

/// The longest the menu waits for an "Open" sequence (seconds) [design: the longest
/// container sequence seen is 0.5 s].
const MAX_WAIT: f32 = 2.0;
/// How long a menu request sent after the animation may take to open the menu before
/// the container closes again (seconds) [design].
const MENU_GRACE: f32 = 0.5;

/// The animated container being opened, and the one whose menu is open.
#[derive(Resource, Default, Debug)]
pub struct ContainerOpening {
    /// Playing "Open"; the menu follows. (container, seconds waited)
    pending: Option<(u32, f32)>,
    /// Open for its menu. (container, menu seen open, seconds since the request)
    shown: Option<(u32, bool, f32)>,
}

impl ContainerOpening {
    /// Called with a request to open the container menu on `container`: when it is an
    /// animated container not yet fully open, its "Open" sequence starts (or goes on) and
    /// the request is held (true); the menu is asked for again when the sequence ends.
    pub fn hold(&mut self, doors: &mut Doors, container: u32) -> bool {
        if doors.is_open(container) != Some(false) {
            return false;
        }
        doors.set_open(container, true);
        if self.pending.is_none_or(|(c, _)| c != container) {
            info!("container 0x{container:08X} plays Open before its menu");
            self.pending = Some((container, 0.0));
        }
        true
    }
}

pub struct ContainerAnimPlugin;

impl Plugin for ContainerAnimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ContainerOpening>().add_systems(Update, follow.in_set(PlaySet::Ui).after(MenuSystems));
    }
}

/// Open the menu once the "Open" sequence has ended; play "Close" when the menu closes.
fn follow(time: Res<Time>, mut op: ResMut<ContainerOpening>, doors: Option<ResMut<Doors>>, active: Res<ActiveMenu>, mut out: MessageWriter<OpenContainer>) {
    let Some(mut doors) = doors else { return };
    let dt = time.delta_secs();
    if let Some((c, waited)) = op.pending.as_mut() {
        *waited += dt;
        let c = *c;
        if doors.is_open(c) != Some(false) || *waited > MAX_WAIT {
            out.write(OpenContainer { container: c });
            op.pending = None;
            op.shown = Some((c, false, 0.0));
        }
    }
    if let Some((c, seen, since)) = op.shown.as_mut() {
        *since += dt;
        let showing = active.0.as_ref().is_some_and(|m| m.mode == Mode::Container(FormId(*c)));
        if showing {
            *seen = true;
        } else if *seen || *since > MENU_GRACE {
            let c = *c;
            if doors.set_open(c, false) {
                info!("container 0x{c:08X} closes (menu closed)");
            }
            op.shown = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doors::DoorTiming;

    #[test]
    fn an_animated_container_is_held_until_it_has_opened() {
        let mut doors = Doors::default();
        let keys = vec![(0.0, "start".to_string()), (0.00001, "Sound: DRSLocker01Open".to_string()), (0.3, "end".to_string())];
        doors.register(5, false, DoorTiming::new((0.0, 0.3, &keys), (0.0, 0.4, &keys)));
        let mut op = ContainerOpening::default();
        // Not drawn (unknown): opens at once.
        assert!(!op.hold(&mut doors, 6));
        assert!(op.hold(&mut doors, 5));
        assert!(op.hold(&mut doors, 5), "asked again while it opens: still held");
        assert_eq!(op.pending.map(|p| p.0), Some(5));
        doors.advance(0.35);
        assert_eq!(doors.is_open(5), Some(true));
        assert!(!op.hold(&mut doors, 5), "fully open: the menu opens");
    }
}
