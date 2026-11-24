//! Steam's relays: a guest anywhere reaches a host behind any router (a campus network, a
//! carrier's NAT) with no port to open and no VPN.
//!
//! Both games talk to Steam as Spacewar ([`APP`], the test app every Steamworks developer
//! may use), so Steam shows the players as playing Spacewar while it is on. Packets go
//! through Steam Networking Messages: unreliable, connectionless, from one Steam account
//! to another, by Valve's relays when a direct route cannot be made. The link's packets
//! are the same as over UDP; a Steam peer gets a stand-in address ([`addr`]) so that
//! `net` keeps one kind of address.
//!
//! Steam is optional: [`steam`] is None when Steam is not running (or `NVF_COOP_STEAM=0`,
//! or `steam_api64.dll` is not beside the game), and the link is plain UDP. The game loads
//! the DLL only when co-op first asks for Steam (it is delay-loaded: nvforever/build.rs),
//! so this checks for it first: a call into a missing delay-loaded DLL would crash.

use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6};
use std::sync::OnceLock;

use steamworks::networking_types::{NetworkingIdentity, SendFlags};
use steamworks::{Client, SteamId};

/// Spacewar.
pub const APP: u32 = 480;
/// Turns Steam off (`0`): the link is plain UDP.
pub const ENV: &str = "NVF_COOP_STEAM";
/// The first 64 bits of a Steam peer's stand-in address (`fd53:7465:616d::/64`, private
/// IPv6, "Steam" in ASCII); its Steam id is the rest.
const PREFIX: u64 = 0xFD53_7465_616D_0000;
/// The Steam id of an individual account in the public universe is this plus its account
/// number.
const INDIVIDUAL: u64 = 0x0110_0001_0000_0000;
/// Messages read in one go.
const BATCH: usize = 64;

/// This game's Steam, once asked for ([`steam`]).
pub struct Steam {
    client: Client,
    me: u64,
}

static STEAM: OnceLock<Option<Steam>> = OnceLock::new();

/// Steam, started on first use (as Spacewar); None when it cannot be (not running, or
/// turned off with [`ENV`]). Every session request is accepted: a Hello for another
/// lobby is turned away by `net`, as over UDP.
pub fn steam() -> Option<&'static Steam> {
    STEAM
        .get_or_init(|| {
            if std::env::var(ENV).is_ok_and(|v| v.trim() == "0") {
                return None;
            }
            if !dll_present() {
                log::info!("co-op: no steam_api64.dll beside the game; the link is plain UDP");
                return None;
            }
            let client = match Client::init_app(APP) {
                Ok(c) => c,
                Err(e) => {
                    log::info!("co-op: no Steam ({e}); the link is plain UDP");
                    return None;
                }
            };
            let me = client.user().steam_id().raw();
            let messages = client.networking_messages();
            messages.session_request_callback(|request| {
                request.accept();
            });
            messages.session_failed_callback(|info| {
                log::warn!("co-op: a Steam session failed: {:?}", info.end_reason());
            });
            // The relays take a few seconds to measure: start now, not at the first packet.
            client.networking_utils().init_relay_network_access();
            log::info!("co-op: Steam is on (as Spacewar, app {APP}), account {}", account_of(me));
            Some(Steam { client, me })
        })
        .as_ref()
}

impl Steam {
    /// This player's Steam account number (what a code carries).
    pub fn account(&self) -> u32 {
        account_of(self.me)
    }

    /// Whether Valve's relays can be used yet, and Steam's words on it.
    pub fn relays(&self) -> String {
        format!("{:?}", self.client.networking_utils().relay_network_status())
    }

    pub(crate) fn send(&self, to: u64, bytes: &[u8]) {
        let to = NetworkingIdentity::new_steam_id(SteamId::from_raw(to));
        let flags = SendFlags::UNRELIABLE_NO_NAGLE | SendFlags::AUTO_RESTART_BROKEN_SESSION;
        let _ = self.client.networking_messages().send_message_to_user(to, flags, bytes, 0);
    }

    /// Run Steam's callbacks (session requests) and take the packets waiting, each with its
    /// sender's stand-in address.
    pub(crate) fn receive(&self, out: &mut Vec<(Vec<u8>, SocketAddr)>) {
        self.client.run_callbacks();
        loop {
            let got = self.client.networking_messages().receive_messages_on_channel(0, BATCH);
            let n = got.len();
            for m in got {
                if let Some(from) = m.identity_peer().steam_id() {
                    out.push((m.data().to_vec(), addr(from.raw())));
                }
            }
            if n < BATCH {
                break;
            }
        }
    }
}

/// Whether Steam's DLL can be loaded (loading it keeps it: the delay-loaded calls use it).
#[cfg(windows)]
fn dll_present() -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut std::ffi::c_void;
    }
    let name: Vec<u16> = "steam_api64.dll\0".encode_utf16().collect();
    // SAFETY: a NUL-terminated wide string; the module is never freed.
    !unsafe { LoadLibraryW(name.as_ptr()) }.is_null()
}

#[cfg(not(windows))]
fn dll_present() -> bool {
    true
}

/// The Steam id of account number `account` (an individual's, public universe).
pub fn steam_id(account: u32) -> u64 {
    INDIVIDUAL | account as u64
}

fn account_of(id: u64) -> u32 {
    id as u32
}

/// A Steam peer's stand-in address.
pub fn addr(steam_id: u64) -> SocketAddr {
    let bits = (PREFIX as u128) << 64 | steam_id as u128;
    SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::from(bits), 0, 0, 0))
}

/// The Steam id a stand-in address stands for; None for a real address.
pub fn peer(addr: SocketAddr) -> Option<u64> {
    let SocketAddr::V6(a) = addr else { return None };
    let bits = u128::from(*a.ip());
    (bits >> 64 == PREFIX as u128 && a.port() == 0).then_some(bits as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stand_in_addresses_round_trip() {
        let id = steam_id(123_456_789);
        assert_eq!(id, 76_561_198_083_722_517);
        assert_eq!(peer(addr(id)), Some(id));
        assert_eq!(account_of(id), 123_456_789);
        assert_eq!(peer("127.0.0.1:47960".parse().unwrap()), None);
        assert_eq!(peer("[::1]:47960".parse().unwrap()), None);
    }
}
