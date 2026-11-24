//! The settings page's Co-op rows (docs/design/coop-networking.md §10): how this game
//! connects to other players. They set `nvf_coop::prefs`, which the network code reads when
//! a lobby opens or a join starts, so a change applies to the next lobby or join.

use bevy::prelude::*;
use nvf_coop::prefs::{self, Connection};
use nvf_play::settings::{add_choice_list, add_setting, ChoiceList, Setting};

pub const CONNECTION_KEY: &str = "coop-connection";
pub const PORT_KEY: &str = "coop-router-port";
pub const RELAY_KEY: &str = "coop-relay";
pub const ASK_KEY: &str = "coop-ask";

fn connection_pick(_: &mut World, name: &str) -> bool {
    match Connection::by_name(name) {
        Some(c) => {
            prefs::set_connection(c);
            true
        }
        None => false,
    }
}

/// The rows' states from the INI, now: a `--host` or `--join` given on the command line
/// starts while the plugin is built, before the settings page applies the INI
/// (`nvf_play::settings::apply_stored`, PostStartup) [observed 2026-11-24: a LAN only guest
/// joined by online code, its INI read 3 s after the join began]. The page reads them
/// again later, to the same states.
fn load_early() {
    let Some(text) = crate::settings::settings_text_at_start() else { return };
    let get = |key: &str| fnv_install::ini_get(&text, nvf_play::settings::SECTION, key);
    let on = |key: &str| get(key).map(|v| !matches!(v.trim(), "0" | "false" | "off" | "Off"));
    if let Some(c) = get(CONNECTION_KEY).and_then(|v| Connection::by_name(&v)) {
        prefs::set_connection(c);
    }
    if let Some(v) = on(PORT_KEY) {
        prefs::set_port_mapping(v);
    }
    if let Some(v) = on(RELAY_KEY) {
        prefs::set_relay_for_others(v);
    }
    if let Some(v) = on(ASK_KEY) {
        prefs::set_ask_before_join(v);
    }    // Self-hosting (docs/coop-self-hosting.md): `[Coop] SignalUrl=` a matchmaking server of
    // the player's own (`nvf-signal-server`; NVF_COOP_SIGNAL, when set, wins), `[Coop]
    // Relay=host:port` a relay (`nvf-relay`) offered as the last-resort path.
    let coop = |key: &str| fnv_install::ini_get(&text, COOP_SECTION, key).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    if let Some(url) = coop("SignalUrl") {
        if std::env::var_os(nvf_coop::signal_io::SERVICE_ENV).is_none() {
            info!("co-op: matchmaking server {url} ([Coop] SignalUrl)");
            std::env::set_var(nvf_coop::signal_io::SERVICE_ENV, &url);
        }
    }
    if let Some(relay) = coop("Relay") {
        use std::net::ToSocketAddrs;
        match relay.to_socket_addrs().ok().and_then(|mut a| a.find(|a| a.is_ipv4())) {
            Some(at) => {
                info!("co-op: hosted relay {relay} ({at}) ([Coop] Relay)");
                prefs::set_hosted_relay(Some(at));
            }
            None => warn!("co-op: [Coop] Relay={relay}: not a host:port this PC can resolve"),
        }
    }
}

/// The INI section of the self-hosting keys.
const COOP_SECTION: &str = "Coop";

pub fn register(app: &mut App) {
    load_early();
    let group = nvf_play::pause::GROUP_MENUS;
    add_choice_list(
        app,
        Setting {
            key: CONNECTION_KEY,
            group,
            label: "Co-op connection",
            note: "Automatic: online codes, and through another player when needed. Direct only: never through another player. LAN only: players on this network only.",
            switches: "",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        ChoiceList {
            names: |_| Connection::ALL.iter().map(|c| c.name().to_string()).collect(),
            chosen: |_| prefs::connection().name().to_string(),
            pick: connection_pick,
            save: |_| prefs::connection().name().to_string(),
            load: connection_pick,
            ask_keep: false,
        },
    );
    add_setting(
        app,
        Setting {
            key: PORT_KEY,
            group,
            label: "Open router port automatically",
            note: "On: your router is asked to let co-op players in (UPnP, NAT-PMP or PCP), and the port closes when you leave. Applies to the next lobby or join.",
            switches: "",
            get: |_| prefs::port_mapping(),
            set: |_, on| prefs::set_port_mapping(on),
            slider: None,
        },
    );
    add_setting(
        app,
        Setting {
            key: RELAY_KEY,
            group,
            label: "Relay for other players",
            note: "On: a player in your lobby who cannot reach the host may play through your game (at most 2, about 15 KB/s each). Their traffic stays encrypted.",
            switches: "",
            get: |_| prefs::relay_for_others(),
            set: |_, on| prefs::set_relay_for_others(on),
            slider: None,
        },
    );
    add_setting(
        app,
        Setting {
            key: ASK_KEY,
            group,
            label: "Ask before players join",
            note: "On: when you host, a player joining by online code waits until you let them in from the Co-op Lobby. Players on your network join at once.",
            switches: "",
            get: |_| prefs::ask_before_join(),
            set: |_, on| prefs::set_ask_before_join(on),
            slider: None,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_coop_rows_register_with_short_notes() {
        let mut app = App::new();
        register(&mut app);
        let settings = app.world().resource::<nvf_play::settings::Settings>();
        for key in [CONNECTION_KEY, PORT_KEY, RELAY_KEY, ASK_KEY] {
            let i = settings.index(key).unwrap_or_else(|| panic!("{key} registered"));
            assert!(nvf_play::settings::note_over_budget(settings.list()[i].note).is_none(), "{key}: note within budget");
        }
        let i = settings.index(CONNECTION_KEY).unwrap();
        let list = settings.choices(i).unwrap();
        assert_eq!((list.names)(app.world()), ["Automatic", "Direct only", "LAN only"]);
    }
}
