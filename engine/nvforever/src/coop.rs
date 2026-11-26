//! Co-op, the game side of the network core (`nvf_coop`): hosting and joining a lobby from
//! the Co-op screens (`nvf_play::lobby`), this player's state sent to the others, and the
//! host's checks of what guests send, by its own load order.
//!
//! What this part does not do yet: draw the other players or share anything of the world.
//! Their states arrive and are kept ([`Coop::states`]); drawing them is the next part.
//!
//! **Off by default.** Nothing here opens a socket until the Co-op switch is on and a
//! lobby is created or joined [design]; this part opens none (below).
//!
//! **No transport here.** This part has no sockets, discovery or lobby codes: hosting or
//! joining says so (the log, and the join screen) until the internet transport of #3910 goes
//! on top and plugs into `nvf_coop::seam` [design]. The host's checks of every guest message
//! ([`HostForms`], `nvf_coop::guard`) and this player's state sent are in place for it.
//!
//! Log: `co-op: no network transport in this build`, `co-op: welcomed as player <n>`, `co-op: <name> joined`, `co-op:
//! player <n> left`, `co-op: player <n> at <place> feet (x, y, z) yaw <deg>` (the first
//! state from each player, then one every [`STATE_LOG_EVERY`] seconds), `co-op: not
//! joined: <why>` (what the join screen says), `co-op: turned away <n> Hello(s) without
//! the lobby's secret` (the host, at most once a second).

use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_coop::guard::FormCheck;
use nvf_coop::net::{flag, Event, Link, Place, PlayerState};
use nvf_play::lobby::{CoopSwitch, Hosting, Joining, LobbyRequest, LobbyState};
use nvf_play::{Location, PlayState};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The port a lobby opens on, UDP [design].
const DEFAULT_PORT: u16 = 47_960;
/// What a game hosting or joining says while no transport is plugged in (module notes).
const NO_TRANSPORT: &str = "This build has no network transport yet: the internet transport goes on top of this part.";
/// A guest looking for its host gives up after this long without an answer (seconds).
const FIND_FOR: f32 = 15.0;
/// Another player's state is logged when first heard, then once this often (seconds).
const STATE_LOG_EVERY: f32 = 10.0;
/// The host looks at the turned-away Hellos this often (seconds).
const DROPS_EVERY: f32 = 1.0;

/// What a lobby is asked for at start (the internet transport on top adds the command-line flags).
#[derive(Clone)]
pub struct Config {
    /// Host on this UDP port.
    pub host: Option<u16>,
    /// Join the lobby this code names.
    pub join: Option<String>,
    /// The name the other players see.
    pub name: String,
}

pub struct CoopPlugin(pub Config);

/// A player in the lobby besides this one.
#[derive(Debug, Clone, Default)]
struct Peer {
    name: String,
    lost: bool,
}

#[derive(Resource)]
pub(crate) struct Coop {
    /// None while no lobby is open or joined.
    link: Option<Link>,
    name: String,
    peers: BTreeMap<u8, Peer>,
    /// The others' newest states (drawing them is the next part).
    states: BTreeMap<u8, PlayerState>,
    /// The host's code, once hosting.
    code: Option<String>,
    note: Option<String>,
    joining: Joining,
    finding_since: Option<f32>,
    /// The host's checks were given the load order.
    forms_set: bool,
    /// When each player's state was last logged.
    state_logged: BTreeMap<u8, f32>,
    /// The host's Hellos turned away for the secret, as of the last look, and when.
    hello_drops: (u64, f32),
}

impl Plugin for CoopPlugin {
    fn build(&self, app: &mut App) {
        let c = &self.0;
        let mut coop = Coop {
            link: None,
            name: c.name.clone(),
            peers: BTreeMap::new(),
            states: BTreeMap::new(),
            code: None,
            note: None,
            joining: Joining::Idle,
            finding_since: None,
            forms_set: false,
            state_logged: BTreeMap::new(),
            hello_drops: (0, 0.0),
        };
        if let Some(port) = c.host {
            coop.start_hosting(port);
        } else if let Some(join) = c.join.as_deref() {
            coop.start_joining(join);
        }
        // The command line's lobby turns the switch on for this run (not kept).
        let on = coop.link.is_some();
        app.insert_resource(coop)
            .add_systems(
                PostStartup,
                (move |mut s: ResMut<CoopSwitch>| {
                    if on && !s.on {
                        s.on = true;
                    }
                })
                .after(nvf_play::lobby::load_switch),
            )
            .add_systems(Update, (host_forms, lobby, send, take_events).chain().after(nvf_play::PlaySet::Physics));
    }
}

impl Coop {
    fn is_host(&self) -> bool {
        self.link.as_ref().is_some_and(Link::is_host)
    }

    fn is_guest(&self) -> bool {
        self.link.as_ref().is_some_and(|l| !l.is_host())
    }

    /// Open a lobby on `port`: no transport is plugged in here, so none opens.
    fn start_hosting(&mut self, port: u16) {
        warn!("co-op: no network transport in this build: cannot host on UDP port {port}");
        self.note = Some(NO_TRANSPORT.to_string());
    }

    fn stop_hosting(&mut self) {
        // Dropping the link says goodbye to the guests.
        self.link = None;
        self.hello_drops = (0, 0.0);
        self.state_logged.clear();
        self.code = None;
        self.note = None;
        self.peers.clear();
        self.states.clear();
    }

    /// Join the lobby `at` names: no transport is plugged in here, so none opens.
    fn start_joining(&mut self, _at: &str) {
        warn!("co-op: no network transport in this build: cannot join");
        self.fail(NO_TRANSPORT);
    }

    /// A join that did not happen: what the join screen says, logged too.
    fn fail(&mut self, why: &str) {
        info!("co-op: not joined: {why}");
        self.joining = Joining::Failed(why.to_string());
    }

    fn stop_joining(&mut self) {
        self.link = None;
        self.state_logged.clear();
        self.joining = Joining::Idle;
        self.peers.clear();
        self.states.clear();
    }
}

/// The host's own load order, for the checks of what guests send (`nvf_coop::guard`): a
/// weapon must be a weapon, a worn item armour, furniture a placed furniture reference,
/// and a place a cell or worldspace the host has.
struct HostForms {
    forms: &'static fnv_sim::forms::FormStore,
    refs: &'static fnv_sim::refs::RefTable,
}

impl HostForms {
    fn sig(&self, form: u32) -> Option<[u8; 4]> {
        self.forms.sig(FormId(form))
    }
}

impl FormCheck for HostForms {
    fn carriable(&self, form: u32) -> bool {
        self.sig(form) == Some(*b"WEAP")
    }
    fn wearable(&self, form: u32) -> bool {
        self.sig(form) == Some(*b"ARMO")
    }
    fn furniture(&self, reference: u32) -> bool {
        self.refs.get(FormId(reference)).is_some_and(|r| self.sig(r.base.0) == Some(*b"FURN"))
    }
    fn place(&self, place: Place) -> bool {
        match place {
            Place::Interior(cell) => self.sig(cell) == Some(*b"CELL"),
            Place::Exterior(world) => self.sig(world) == Some(*b"WRLD"),
        }
    }
}

/// Give the host's checks its load order once the world is loaded.
fn host_forms(mut coop: ResMut<Coop>, sim: Option<Res<nvf_play::sim::SimState>>) {
    if coop.forms_set || !coop.is_host() {
        return;
    }
    let Some(sim) = sim else { return };
    let check: Arc<dyn FormCheck> = Arc::new(HostForms { forms: sim.forms, refs: sim.refs });
    if let Some(link) = &coop.link {
        link.set_forms(Some(check));
        coop.forms_set = true;
        info!("co-op: the host checks guests' forms by its load order");
    }
}

/// The lobby screens' asks, and what they show.
fn lobby(time: Res<Time<Real>>, mut coop: ResMut<Coop>, mut asks: MessageReader<LobbyRequest>, mut state: ResMut<LobbyState>, switch: Res<CoopSwitch>) {
    let now = time.elapsed_secs();
    let coop = &mut *coop;
    for ask in asks.read() {
        match ask {
            LobbyRequest::Create if !switch.on => info!("co-op: the Co-op switch is off: no lobby"),
            LobbyRequest::Create if coop.is_guest() => info!("co-op: in another player's lobby: leave it to open one"),
            LobbyRequest::Create if coop.link.is_none() => coop.start_hosting(DEFAULT_PORT),
            LobbyRequest::Create => {}
            LobbyRequest::Close if coop.is_host() => {
                coop.stop_hosting();
                info!("co-op: lobby closed");
            }
            LobbyRequest::Close => {}
            LobbyRequest::Leave if coop.is_guest() => {
                coop.stop_joining();
                info!("co-op: left the lobby");
            }
            LobbyRequest::Leave => {}
            LobbyRequest::Find(_) if !switch.on => info!("co-op: the Co-op switch is off: not joining"),
            LobbyRequest::Find(_) if coop.is_host() => info!("co-op: close your own lobby to join another"),
            LobbyRequest::Find(code) => {
                coop.stop_joining();
                coop.start_joining(code);
            }
            LobbyRequest::Cancel => {
                if coop.joining == Joining::Finding {
                    coop.stop_joining();
                }
                if matches!(coop.joining, Joining::Failed(_)) {
                    coop.joining = Joining::Idle;
                }
            }
        }
    }
    // A guest's search: answered (`take_events`), or given up.
    if coop.joining == Joining::Finding {
        let since = *coop.finding_since.get_or_insert(now);
        if coop.link.as_ref().is_some_and(|l| l.id().is_some()) {
            coop.joining = Joining::Found;
        } else if now - since > FIND_FOR {
            coop.link = None;
            coop.fail("No answer from the host: check the code, and that their lobby is open.");
        }
    }
    let hosting = match (&coop.code, coop.is_host()) {
        (Some(code), true) => Hosting::Open { code: code.clone(), note: coop.note.clone() },
        _ => Hosting::Off,
    };
    let mut players = Vec::new();
    if coop.link.is_some() {
        players.push(if coop.is_host() { format!("{} (host, you)", coop.name) } else { format!("{} (you)", coop.name) });
        for (&id, p) in &coop.peers {
            let n = label(id, &p.name);
            // The host is player 0: marked on a guest's list.
            let n = if id == 0 && coop.is_guest() && n != "The host" { format!("{n} (host)") } else { n };
            players.push(if p.lost { format!("{n} (reconnecting)") } else { n });
        }
    }
    state.set_if_neq(LobbyState { hosting, players, joining: coop.joining.clone(), guest: coop.is_guest() });
}

fn label(id: u8, name: &str) -> String {
    match (id, name.is_empty()) {
        (0, true) => "The host".into(),
        (_, true) => format!("Player {}", id + 1),
        _ => name.to_string(),
    }
}

fn place_of(loc: &Location) -> Place {
    loc.interior.map_or(Place::Exterior(loc.world), Place::Interior)
}

/// This player's state, for the others.
fn send(
    coop: Res<Coop>,
    player: Option<Res<nvf_play::player::Player>>,
    loc: Option<Res<Location>>,
    control: Option<Res<nvf_play::control::PlayerControl>>,
    play: Option<Res<State<PlayState>>>,
    main_menu: Option<Res<nvf_play::main_menu::MainMenu>>,
) {
    let (Some(link), Some(player), Some(loc)) = (coop.link.as_ref(), player, loc) else { return };
    // At the main menu there is no player in the world yet.
    if main_menu.is_some_and(|m| m.open) {
        return;
    }
    // Behind a loading screen the place is the one being left: keep the last state sent.
    if play.is_some_and(|s| *s.get() == PlayState::Loading) {
        return;
    }
    let mut s = PlayerState::at(place_of(&loc), player.feet, player.heading);
    s.pitch = player.pitch;
    if let Some(c) = control.as_deref() {
        let mut flags = 0;
        if c.weapon_out {
            flags |= flag::DRAWN;
            if c.aiming {
                flags |= flag::AIMING;
            }
        }
        if c.sneak {
            flags |= flag::SNEAKING;
        }
        s.flags = flags;
    }
    link.set_state(s);
}

/// What the link heard: who joined, left, came back; the others' states.
fn take_events(time: Res<Time<Real>>, mut coop: ResMut<Coop>) {
    let now = time.elapsed_secs();
    let Some(events) = coop.link.as_ref().map(Link::events) else { return };
    let coop = &mut *coop;
    // The host: Hellos turned away without the lobby's secret since the last look.
    if coop.is_host() && now - coop.hello_drops.1 >= DROPS_EVERY {
        let total = coop.link.as_ref().map_or(0, |l| hello_secret_drops(&l.guard_stats()));
        if total > coop.hello_drops.0 {
            info!("co-op: turned away {} Hello(s) without the lobby's secret", total - coop.hello_drops.0);
        }
        coop.hello_drops = (total, now);
    }
    for e in events {
        match e {
            Event::Welcomed(id) => {
                info!("co-op: welcomed as player {}", id + 1);
                coop.joining = Joining::Found;
            }
            Event::Refused => {
                coop.link = None;
                coop.fail("The lobby is full.");
            }
            Event::Joined { id, name } => {
                info!("co-op: {} joined", label(id, &name));
                coop.peers.entry(id).or_default().name = name;
            }
            Event::Named { id, name } => coop.peers.entry(id).or_default().name = name,
            Event::State { id, state } => {
                coop.peers.entry(id).or_default();
                if coop.state_logged.get(&id).is_none_or(|&t| now - t >= STATE_LOG_EVERY) {
                    coop.state_logged.insert(id, now);
                    info!("co-op: {}", state_line(id, &state));
                }
                coop.states.insert(id, state);
            }
            Event::Lost(id) => {
                if let Some(p) = coop.peers.get_mut(&id) {
                    p.lost = true;
                }
            }
            Event::Back(id) => {
                if let Some(p) = coop.peers.get_mut(&id) {
                    p.lost = false;
                }
            }
            Event::Left(id) => {
                info!("co-op: player {} left", id + 1);
                coop.peers.remove(&id);
                coop.states.remove(&id);
                coop.state_logged.remove(&id);
            }
            Event::HostLost => {
                if let Some(p) = coop.peers.get_mut(&0) {
                    p.lost = true;
                }
            }
            Event::HostBack => {
                if let Some(p) = coop.peers.get_mut(&0) {
                    p.lost = false;
                }
            }
            Event::HostGone => {
                coop.stop_joining();
                coop.fail("The host closed the lobby.");
            }
            // Looks and the reliable stream: for the parts that draw players and share
            // the world.
            Event::Look { .. } | Event::Reliable { .. } | Event::RelReset { .. } => {}
        }
    }
}

/// The Hellos the host turned away for a wrong or missing secret.
fn hello_secret_drops(stats: &nvf_coop::guard::Stats) -> u64 {
    use nvf_coop::guard::{Kind, Reason};
    stats.dropped.iter().filter(|(k, r, _)| *k == Kind::Hello && *r == Reason::Secret).map(|(_, _, n)| n).sum()
}

/// A player's state as the log shows it: where, the feet rounded, the heading in degrees.
fn state_line(id: u8, s: &PlayerState) -> String {
    let place = match s.place {
        Place::Interior(c) => format!("interior {c:08X}"),
        Place::Exterior(w) => format!("worldspace {w:08X}"),
    };
    let [x, y, z] = s.feet.map(|v| v.round() as i64);
    format!("player {} at {place} feet ({x}, {y}, {z}) yaw {:.0}", id + 1, s.yaw.to_degrees())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no transport plugged in, hosting opens nothing and joining says why.
    #[test]
    fn hosting_and_joining_without_a_transport_say_so() {
        let mut app = App::new();
        app.add_plugins(CoopPlugin(Config { host: Some(DEFAULT_PORT), join: None, name: "Courier".into() }));
        let coop = app.world().resource::<Coop>();
        assert!(coop.link.is_none() && coop.code.is_none());
        assert_eq!(coop.note.as_deref(), Some(NO_TRANSPORT));
        let mut app = App::new();
        app.add_plugins(CoopPlugin(Config { host: None, join: Some("anything".into()), name: "Courier".into() }));
        let coop = app.world().resource::<Coop>();
        assert!(coop.link.is_none());
        assert_eq!(coop.joining, Joining::Failed(NO_TRANSPORT.to_string()));
    }

    #[test]
    fn a_state_is_logged_with_its_place_and_rounded_feet() {
        let s = PlayerState::at(Place::Exterior(0x1234), [-68375.4, 4800.6, 8700.0], std::f32::consts::FRAC_PI_2);
        assert_eq!(state_line(1, &s), "player 2 at worldspace 00001234 feet (-68375, 4801, 8700) yaw 90");
    }

    #[test]
    fn players_are_named_by_slot_until_their_names_arrive() {
        assert_eq!(label(0, ""), "The host");
        assert_eq!(label(2, ""), "Player 3");
        assert_eq!(label(2, "Boone"), "Boone");
    }

    /// Co-op starts nothing: no lobby, no socket, until asked.
    #[test]
    fn nothing_is_opened_by_default() {
        let mut app = App::new();
        app.add_plugins(CoopPlugin(Config { host: None, join: None, name: "Courier".into() }));
        let coop = app.world().resource::<Coop>();
        assert!(coop.link.is_none() && coop.code.is_none() && coop.joining == Joining::Idle);
    }
}
