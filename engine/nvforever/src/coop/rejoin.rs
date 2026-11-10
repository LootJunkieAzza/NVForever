//! Back beside the host after everyone was down (overnight item 152, 2026-11-10) [design].
//!
//! When every player is down, each guest loads its own last shared save
//! (`nvf_play::coop_progress`, [`nvf_play::coop_progress::REJOIN_ENV`] set) and its new run
//! joins the host's again. A guest's save is its own game where it left it (its Mojave
//! spot, `nvf_play::save::SoloBase`), and the replication build's free roam moves a guest
//! to the host only when the host's script moves the host (`super::leash`), so before
//! this the guest woke up at its own save's spot, far from the party (Goodsprings, Doc
//! Mitchell's, playtest 119). Now that run takes the guest beside the host once: as soon
//! as this game is kept as the guest's own, the host is heard, and the host has stood in
//! its place for [`HOST_SETTLED`] seconds (its own reload's loading screen is over).
//!
//! Log: `co-op: back after everyone was down: beside the host at …`.

use super::{at_main_menu, scene, Coop};
use bevy::prelude::*;
use nvf_coop::net::Place;
use nvf_play::loading::{Loading, TravelRequest};
use nvf_play::Travel;

/// The host must have stood in its place this long (seconds) before a guest is brought
/// to it: the host's own reload puts it there behind a loading screen.
const HOST_SETTLED: f32 = 2.0;

/// Whether the trip beside the host may go now (pure, for the tests).
fn ready(guest: bool, kept: bool, host_lost: bool, loading: bool, host_here_since: Option<f32>, now: f32) -> bool {
    guest && kept && !host_lost && !loading && host_here_since.is_some_and(|t| now - t >= HOST_SETTLED)
}

/// The guest's first run after an all-down reload: beside the host, once (module notes).
#[allow(clippy::too_many_arguments)]
pub(super) fn rejoin_host(
    time: Res<Time>,
    mut coop: ResMut<Coop>,
    loading: Option<Res<Loading>>,
    main_menu: Option<Res<nvf_play::main_menu::MainMenu>>,
    solo: Option<Res<nvf_play::save::SoloBase>>,
    mut travel: MessageWriter<TravelRequest>,
    mut due: Local<Option<bool>>,
) {
    // Read once, and taken out of the environment so a later load of this run does not
    // take the guest along again.
    let due = due.get_or_insert_with(|| {
        let on = std::env::var_os(nvf_play::coop_progress::REJOIN_ENV).is_some();
        std::env::remove_var(nvf_play::coop_progress::REJOIN_ENV);
        if on {
            info!("co-op: this run follows an all-down reload: the guest goes beside the host once it is heard");
        }
        on
    });
    if !*due || at_main_menu(main_menu.as_deref()) {
        return;
    }
    let now = time.elapsed_secs();
    let since = coop.host_place.map(|(_, t)| t);
    if !ready(coop.is_guest(), solo.is_some(), coop.host_lost, loading.is_some_and(|l| l.active()), since, now) {
        return;
    }
    let Some(host) = coop.host else { return };
    // Each guest its own place beside the host (`scene::beside_slot`, item 154).
    let feet = scene::beside_slot(host.feet, host.yaw, coop.link.as_ref().and_then(|l| l.id()).unwrap_or(1));
    let (world, interior) = match host.place {
        Place::Interior(c) => (None, Some(c)),
        Place::Exterior(w) => (Some(w), None),
    };
    info!("co-op: back after everyone was down: beside the host at {:?} {feet:?}", host.place);
    travel.write(TravelRequest(Travel { world, interior, feet, heading_deg: host.yaw.to_degrees() }));
    coop.last_leash = Some(now);
    *due = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goes_once_the_host_has_settled() {
        // Not before the guest's game is kept, the host is heard and has settled.
        assert!(!ready(true, false, false, false, Some(0.0), 10.0));
        assert!(!ready(true, true, false, false, None, 10.0));
        assert!(!ready(true, true, false, false, Some(9.0), 10.0));
        assert!(ready(true, true, false, false, Some(8.0), 10.0));
        // Not while this game loads, nor with a lost host, nor on the host.
        assert!(!ready(true, true, false, true, Some(0.0), 10.0));
        assert!(!ready(true, true, true, false, Some(0.0), 10.0));
        assert!(!ready(false, true, false, false, Some(0.0), 10.0));
    }
}
