//! Marks drawn on the HUD over points of the world (the fork, the owner's ask of
//! 2026-11-06): the other players' **name tags** (a setting) and **pings**. Built as JAM's
//! world markers (Just Visual Objectives) look: its arrow pointing down at the point, the
//! distance over it and the name over that, in the HUD's colour and JVO's font
//! (`glow_monofonto_medium`, its `_JVOFont` 3), at full strength near the crosshair and
//! fainter away from it [observed: JAM's `menus\prefabs\JVO\JVO.xml`, `_JVOAlphaMult`
//! 0.6 outside `_JVORadiusMax` 0.06; the arrow is its `textures\interface\JVO\JVOClassic.dds`
//! turned half round]. Without JAM the arrow is a small diamond of our own [design]. JAM's
//! own quest markers are its scripts' HUD tiles, not these.
//!
//! - **Name tags** ([`NameTags`], setting `name-tags`, on at first): each other player in
//!   this player's cell or worldspace, its name just over its head ([`tag_point`]: over
//!   the top of its drawn head, at most [`TAG_GAP_MAX`] of the view above it); seen through walls, as JVO's markers are; not drawn while it is off screen.
//!   None for a player in another interior or worldspace (its `here`), so none through a
//!   door. In full up to the setting `name-tag-near` ([`NEAR_DEFAULT`]), then smaller and
//!   fainter, gone at `name-tag-far` ([`FAR_DEFAULT`]) [design: owner 2026-11-09, queue
//!   item 136, "fade away, get smaller and then disappear"; [`tag_fade`]].
//! - **Pings** (control Ping, C or the mouse's back button [design]): the point under the
//!   crosshair, out to [`PING_REACH`], is marked for every player for [`PING_SECONDS`],
//!   the pinger's name over it, with the thing pinged when it has a name ("Player 2:
//!   Sunny Smiles"); a ping on an actor follows the actor. A player's new ping takes the
//!   place of its last; pinging again within [`PING_UNDO_SECONDS`] near the last takes it
//!   away. A ping off screen stays at the screen's nearest edge (JVO's off-screen
//!   handling 1). In co-op the pings ride the table actions ([`crate::coop_tables::game::PING`],
//!   table [`EVERYONE`]: no protocol change); alone they are this game's only.
//!
//! Log lines: `world marks: ping at (x, y, z)[ on <name>]` and `world marks: <player>
//! pinged …`; `world marks: name tags on/off`.

use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use nvf_assets::GameData;

use crate::bindings::{Control, ControlInput};
use crate::coop_social::SocialMates;
use crate::coop_tables::{game, Tables, R, W};
use crate::interface::{amber, at, face, plain_font, set_line, u, Face, FONT_FILES};
use crate::settings::{add_setting, Setting};
use crate::sim::SimState;
use crate::{space, Location, PlayState, PlaySet, PlayerCamera};

/// The top of a standing player's head over its feet (game units), for a player whose
/// puppet is not drawn yet [observed: the 2026-11-20 split relay traces, head bone 130
/// over the feet, the head's top 14 over that].
pub const HEAD_HEIGHT: f32 = 144.0;
/// The top of a drawn head over its head bone (game units) [observed: the 2026-11-20 split
/// relay frames and traces, 13.6 and 14.5 units at 250 to 300 units away].
pub const HEAD_TOP: f32 = 14.0;
/// A tag's gap over the top of the head, as far as it is drawn in the world (game units):
/// the look of queue item 137 (24 over the head bone) at a normal distance [design].
pub const OVER_HEAD: f32 = 10.0;
/// The most that gap takes on the screen, as a part of the view's height: close by, or in
/// a split-screen half (its view is half the height, the world drawn as large as on the
/// whole screen), 10 units grew to 115 pixels of a 720-pixel half and the tags stood
/// over the house behind the players [observed: the 2026-11-20 relay frame,
/// online-relay-c-240s.png, players about 150 units away]; 25 pixels of a 720-pixel view
/// [design].
pub const TAG_GAP_MAX: f32 = 0.035;
/// A name tag is in full up to the near distance, then fades and shrinks, gone by the far
/// distance (game units); the settings offer these [design: queue item 136, the parity
/// session's suggestion of about 1500 and 4000].
pub const NEAR_DEFAULT: f32 = 1500.0;
pub const FAR_DEFAULT: f32 = 4000.0;
pub const NEAR_CHOICES: [f32; 7] = [500.0, 1000.0, 1500.0, 2000.0, 3000.0, 4000.0, 6000.0];
pub const FAR_CHOICES: [f32; 7] = [2000.0, 3000.0, 4000.0, 6000.0, 8000.0, 12000.0, 16000.0];
/// How small and faint a tag gets just before it is gone (parts of its size and
/// strength) [design: kept readable to the cutoff; at 0.1 strength a 640x360 laptop run
/// showed nothing at 3500 of 4000 units, queue item 136].
pub const FAR_SCALE: f32 = 0.5;
pub const FAR_ALPHA: f32 = 0.35;
/// Fade steps: a tag is drawn again only when its fade moves a step [design].
const FADE_STEPS: f32 = 20.0;

/// How a name tag `distance` away is drawn: its strength and its size (parts of full),
/// or `None` when it is gone. Eased (smoothstep) between `near` and `far` down to
/// [`FAR_ALPHA`] and [`FAR_SCALE`], then gone; a `far` not past `near` counts as 500 past
/// it [design].
pub fn tag_fade(distance: f32, near: f32, far: f32) -> Option<(f32, f32)> {
    let far = far.max(near + 500.0);
    if distance >= far {
        return None;
    }
    let t = ((distance - near) / (far - near)).clamp(0.0, 1.0);
    let t = t * t * (3.0 - 2.0 * t);
    let step = |v: f32| (v * FADE_STEPS).round() / FADE_STEPS;
    Some((step(1.0 - (1.0 - FAR_ALPHA) * t), step(1.0 - (1.0 - FAR_SCALE) * t)))
}
/// How far the crosshair's ray reaches for a ping (game units) [design: the loaded cells].
pub const PING_REACH: f32 = 40_000.0;
/// How long a ping stays (real seconds) [design].
pub const PING_SECONDS: f32 = 12.0;
/// Pinging again this soon (real seconds) within [`PING_UNDO_RANGE`] of the last ping
/// takes it away [design].
pub const PING_UNDO_SECONDS: f32 = 1.5;
pub const PING_UNDO_RANGE: f32 = 64.0;
/// A ping on an actor marks this far over its feet (game units) [design].
pub const PING_ACTOR_HEIGHT: f32 = 100.0;
/// The table of a table action for every player.
pub const EVERYONE: u32 = u32::MAX;
/// The near-crosshair radius and the strength away from it [observed: JVO.xml
/// `_JVORadiusMax` 0.06 squared against the screen-height-relative offset,
/// `_JVOAlphaMult` 0.6].
const FOCUS_RADIUS: f32 = 0.06;
const AWAY_ALPHA: f32 = 0.6;
/// JVO's arrow and its size in interface units [observed: JVO.xml `_JVOWidthBase` 24,
/// `_JVOHeightBase` 36].
const ARROW: &str = "textures\\interface\\jvo\\jvoclassic.dds";
const ARROW_SIZE: Vec2 = Vec2::new(24.0, 36.0);
/// The off-screen margin, as parts of the screen [observed: JVO.xml `_JVOOffsetWidth`
/// 0.02, `_JVOOffsetHeight` 0.01].
const EDGE: Vec2 = Vec2::new(0.02, 0.01);
/// A distance in feet [observed: JVO's script divides game units by 21.333 for "ft."].
const UNITS_PER_FOOT: f32 = 21.333;

/// The name-tag setting.
#[derive(Resource, Debug, Clone, Copy)]
pub struct NameTags {
    pub on: bool,
    /// The fade's near and far distances (game units, [`tag_fade`]).
    pub near: f32,
    pub far: f32,
}

/// The other players whose name tag is not drawn (their references): a body a camera shot
/// leaves out is not marked there either (the arm-wrestling end view, `match_end`).
#[derive(Resource, Debug, Clone, Default)]
pub struct HiddenTags(pub Vec<u32>);

impl Default for NameTags {
    fn default() -> Self {
        NameTags { on: true, near: NEAR_DEFAULT, far: FAR_DEFAULT }
    }
}

fn choice_of(choices: &[f32], v: f32) -> f32 {
    let i = choices.iter().position(|&c| c >= v).unwrap_or(choices.len() - 1);
    crate::settings::choice_part(i, choices.len())
}

fn value_of(choices: &[f32], part: f32) -> f32 {
    choices[crate::settings::choice_index(part, choices.len()).min(choices.len() - 1)]
}

fn units(v: f32) -> String {
    format!("{v:.0} units")
}

/// What a mark is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    NameTag,
    Ping,
}

/// A ping as it goes between the games: the place (worldspace, interior 0 for none), the
/// point (game space) and the reference pinged (0 for none).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PingMsg {
    pub world: u32,
    pub interior: u32,
    pub pos: [f32; 3],
    pub target: u32,
    /// Take the sender's ping away instead.
    pub clear: bool,
    /// `target` is an actor: the ping follows it.
    pub actor: bool,
}

impl PingMsg {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.u8(1).u8(u8::from(self.clear) | u8::from(self.actor) << 1).u32(self.world).u32(self.interior).f32(self.pos[0]).f32(self.pos[1]).f32(self.pos[2]).u32(self.target);
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<PingMsg> {
        let mut r = R(b);
        if r.u8()? != 1 {
            return None;
        }
        let flags = r.u8()?;
        Some(PingMsg { clear: flags & 1 != 0, actor: flags & 2 != 0, world: r.u32()?, interior: r.u32()?, pos: [r.f32()?, r.f32()?, r.f32()?], target: r.u32()? })
    }

    fn here(&self, loc: &Location) -> bool {
        match loc.interior {
            Some(c) => self.interior == c,
            None => self.interior == 0 && self.world == loc.world,
        }
    }
}

/// A ping shown: whose (a player id, 255 for this game's own when alone), its words, and
/// until when (real seconds).
#[derive(Debug, Clone, PartialEq)]
pub struct Ping {
    pub from: u8,
    pub msg: PingMsg,
    pub label: String,
    pub at: f32,
    pub until: f32,
    /// Its sound was played ([`ping_sound`]).
    pub sounded: bool,
}

/// The pings shown now.
#[derive(Resource, Debug, Default)]
pub struct Pings(pub Vec<Ping>);

impl Pings {
    /// Take `from`'s ping, or away its last (`msg.clear`).
    pub fn take(&mut self, from: u8, msg: PingMsg, label: String, now: f32) {
        self.0.retain(|p| p.from != from);
        if !msg.clear {
            self.0.push(Ping { from, msg, label, at: now, until: now + PING_SECONDS, sounded: false });
        }
    }

    /// Whether a new ping at `pos` by `from` takes away its last instead
    /// ([`PING_UNDO_SECONDS`]).
    pub fn undoes(&self, from: u8, pos: [f32; 3], now: f32) -> bool {
        self.0.iter().any(|p| {
            p.from == from && now - p.at <= PING_UNDO_SECONDS && (0..3).map(|k| (p.msg.pos[k] - pos[k]).powi(2)).sum::<f32>() <= PING_UNDO_RANGE * PING_UNDO_RANGE
        })
    }
}

/// One mark as drawn this frame.
#[derive(Debug, Clone, PartialEq)]
struct Shown {
    kind: MarkKind,
    label: String,
    distance: Option<String>,
    focus: bool,
    /// A name tag's distance fade: its strength and size (1 for pings, [`tag_fade`]).
    fade: (f32, f32),
}

/// A mark's root node on the screen.
#[derive(Component)]
struct MarkRoot;

struct Kit {
    face: Face,
    arrow: Option<Handle<Image>>,
}

pub struct WorldMarksPlugin;

impl Plugin for WorldMarksPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NameTags>().init_resource::<HiddenTags>().init_resource::<Pings>().add_systems(Update, ping_input.in_set(PlaySet::Input).after(crate::control::read_controls)).add_systems(
            Update,
            (take_pings, ping_sound, draw, trace_tags).chain().in_set(PlaySet::Ui),
        );
        add_setting(
            app,
            Setting {
                key: "name-tags",
                group: "Co-op",
                label: "Player name tags",
                note: "On: the other players' names are shown over their heads, through walls. Off: not shown.",
                switches: "",
                get: |w| w.resource::<NameTags>().on,
                set: |w, on| {
                    info!("world marks: name tags {}", if on { "on" } else { "off" });
                    w.resource_mut::<NameTags>().on = on;
                },
                slider: None,
            },
        );
        add_setting(
            app,
            Setting::slider(
                "name-tag-near",
                "Co-op",
                "Name tags fade from",
                "A player's name tag is in full up to this distance, then fades and shrinks.",
                crate::settings::Slider::choice(
                    NEAR_CHOICES.len(),
                    |w| choice_of(&NEAR_CHOICES, w.resource::<NameTags>().near),
                    |w, v| {
                        let near = value_of(&NEAR_CHOICES, v);
                        info!("world marks: name tags fade from {near:.0} units");
                        w.resource_mut::<NameTags>().near = near;
                    },
                    |v| units(value_of(&NEAR_CHOICES, v)),
                ),
            ),
        );
        add_setting(
            app,
            Setting::slider(
                "name-tag-far",
                "Co-op",
                "Name tags gone at",
                "A player's name tag is gone at this distance (at least 500 units past where it starts to fade).",
                crate::settings::Slider::choice(
                    FAR_CHOICES.len(),
                    |w| choice_of(&FAR_CHOICES, w.resource::<NameTags>().far),
                    |w, v| {
                        let far = value_of(&FAR_CHOICES, v);
                        info!("world marks: name tags gone at {far:.0} units");
                        w.resource_mut::<NameTags>().far = far;
                    },
                    |v| units(value_of(&FAR_CHOICES, v)),
                ),
            ),
        );
    }
}

/// This game's player pings what is under the crosshair.
#[allow(clippy::too_many_arguments)]
fn ping_input(
    input: ControlInput,
    state: Res<State<PlayState>>,
    real: Res<Time<Real>>,
    player: Option<Res<crate::player::Player>>,
    control: Option<Res<crate::control::PlayerControl>>,
    phys: Option<ResMut<crate::phys::PhysState>>,
    loc: Option<Res<Location>>,
    sim: Option<Res<SimState>>,
    script: Option<Res<crate::script::ScriptState>>,
    (mut pings, mut tables): (ResMut<Pings>, ResMut<Tables>),
) {
    if *state.get() != PlayState::Playing || !input.just_pressed(Control::Ping) {
        return;
    }
    let (Some(player), Some(mut phys), Some(loc)) = (player, phys, loc) else { return };
    let crosshair = control.as_deref().and_then(|c| c.crosshair);
    let (eye, dir) = crate::control::Crosshair::ray(crosshair.as_ref(), player.eye(), player.look_dir());
    let mut filter = fnv_phys::Filter::default();
    filter.exclude_layers.insert(fnv_phys::layers::TRIGGER);
    let Some(hit) = phys.ray(eye, dir, PING_REACH, filter) else {
        info!("world marks: ping: nothing under the crosshair within {PING_REACH} units");
        return;
    };
    let (target, actor) = match hit.owner {
        fnv_phys::Owner::Ref { form_id, .. } => (form_id, false),
        fnv_phys::Owner::Actor { form_id, .. } => (form_id, true),
        _ => (0, false),
    };
    let now = real.elapsed_secs();
    let me = tables.me.filter(|_| tables.on).unwrap_or(255);
    let mut msg = PingMsg { world: loc.world, interior: loc.interior.unwrap_or(0), pos: hit.point, target, clear: false, actor };
    msg.clear = pings.undoes(me, hit.point, now);
    let game = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game);
    let thing = sim.as_deref().filter(|_| target != 0).and_then(|s| s.ref_name(game, target)).map(str::to_string);
    if msg.clear {
        info!("world marks: ping taken away");
    } else {
        let [x, y, z] = hit.point;
        info!("world marks: ping at ({x:.0}, {y:.0}, {z:.0}){}", thing.as_deref().map(|t| format!(" on {t}")).unwrap_or_default());
    }
    let name = "You";
    pings.take(me, msg, label(name, thing.as_deref()), now);
    tables.send(EVERYONE, game::PING, msg.encode());
}

/// Each new ping in this player's place is heard from its direction: the Pip-Boy radio's
/// "signal found" (`UIRadioSignalFound`) [design: the owner's ask of 2026-11-06; the base
/// game has no sound for a custom map marker, observed: no such SOUN in FalloutNV.esm].
/// An interface sound plays flat; this one is panned toward the ping
/// (`crate::sfx::Sfx::placed_2d`), at its record's level at any distance. Behind and in
/// front sound alike (the engine's panning is left and right).
fn ping_sound(
    mut pings: ResMut<Pings>,
    loc: Option<Res<Location>>,
    sfx: Option<ResMut<crate::sfx::Sfx>>,
    player: Option<Res<crate::player::Player>>,
    mut out: MessageWriter<crate::sfx::PlaySound>,
    mut sound: Local<Option<Option<fnv_sim::FormId>>>,
) {
    let (Some(loc), Some(mut sfx)) = (loc, sfx) else { return };
    let sound = *sound.get_or_insert_with(|| {
        let id = sfx.forms.sounds.iter().find(|(_, s)| s.edid.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(PING_SOUND))).map(|(id, _)| *id);
        match id {
            Some(id) => {
                sfx.placed_2d.insert(id);
            }
            None => info!("world marks: no {PING_SOUND} in the load order: pings are silent"),
        }
        id
    });
    for p in pings.0.iter_mut().filter(|p| !p.sounded) {
        p.sounded = true;
        let Some(s) = sound.filter(|_| p.msg.here(&loc)) else { continue };
        let [l, r] = player.as_deref().map_or([1.0, 1.0], |pl| crate::sfx::pan_gains(pl.eye(), pl.heading, p.msg.pos));
        info!("world marks: ping heard from ({:.0}, {:.0}, {:.0}), left {l:.2} right {r:.2}", p.msg.pos[0], p.msg.pos[1], p.msg.pos[2]);
        out.write(crate::sfx::PlaySound { sound: s, at: Some(p.msg.pos) });
    }
}

/// The ping's sound's SOUN ([`ping_sound`]).
pub const PING_SOUND: &str = "UIRadioSignalFound";

fn label(who: &str, thing: Option<&str>) -> String {
    match thing.filter(|t| !t.is_empty()) {
        Some(t) => format!("{who}: {t}"),
        None => who.to_string(),
    }
}

/// The other players' pings, and the end of old ones.
fn take_pings(real: Res<Time<Real>>, mut pings: ResMut<Pings>, mut tables: ResMut<Tables>, sim: Option<Res<SimState>>, script: Option<Res<crate::script::ScriptState>>) {
    let now = real.elapsed_secs();
    let actions = tables.take_actions(game::PING);
    let game = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game);
    for (from, table, blob) in actions {
        let Some(msg) = PingMsg::decode(&blob).filter(|_| table == EVERYONE) else { continue };
        let thing = sim.as_deref().filter(|_| msg.target != 0).and_then(|s| s.ref_name(game, msg.target)).map(str::to_string);
        let name = tables.name(from);
        if msg.clear {
            info!("world marks: {name} took its ping away");
        } else {
            let [x, y, z] = msg.pos;
            info!("world marks: {name} pinged ({x:.0}, {y:.0}, {z:.0}){}", thing.as_deref().map(|t| format!(" on {t}")).unwrap_or_default());
        }
        pings.take(from, msg, label(&name, thing.as_deref()), now);
    }
    if pings.0.iter().any(|p| p.until <= now) {
        pings.0.retain(|p| p.until > now);
    }
}

/// Where a point of the world is on a `size` screen, as parts of it from the top left, and
/// whether it is in front of the camera.
fn on_screen(cam: &Camera, t: &GlobalTransform, size: Vec2, p: [f32; 3]) -> Option<(Vec2, bool)> {
    let world = space::to_bevy(p);
    let ndc = cam.world_to_ndc(t, world).into_iter().next()?;
    let in_front = ndc.z > 0.0 && ndc.z < 1.0;
    let mut s = Vec2::new((ndc.x + 1.0) / 2.0, (1.0 - ndc.y) / 2.0);
    if !in_front {
        // Behind the camera: mirrored, so it lands on the side it is on (as WorldToScreen,
        // `fnv_game::nvse_jam`), pushed past the nearest edge.
        let d = Vec2::splat(0.5) - s;
        let m = d.x.abs().max(d.y.abs()).max(1e-6);
        s = Vec2::splat(0.5) + d * if m < 0.5 { 0.505 / m } else { 1.0 };
    }
    let _ = size;
    Some((s, in_front))
}

/// The play state and the Pip-Boy (full and glance), which hide the marks.
type PlayAndPipBoy<'w> = (Res<'w, State<PlayState>>, Option<Res<'w, crate::pipboy::PipBoy>>, Option<Res<'w, crate::pipboy::glance::Glance>>);

/// The name tags' switches: on, the hidden ones, and the drinking contest's screen and
/// win shot, which hide them.
type TagParams<'w> = (Res<'w, NameTags>, Res<'w, HiddenTags>, Option<Res<'w, crate::drink_contest::DrinkScreen>>, Option<Res<'w, crate::drink_world::DrinkWorld>>);

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    (tags, hidden, drink, stage): TagParams,
    pings: Res<Pings>,
    mates: Res<SocialMates>,
    heads: Res<crate::emotes::MateHeads>,
    loc: Option<Res<Location>>,
    sim: Option<Res<SimState>>,
    (state, pipboy, glance): PlayAndPipBoy,
    cameras: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    data: Option<Res<GameData>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut images: ResMut<Assets<Image>>,
    mut zone: ResMut<crate::safe_zone::SafeZone>,
    mut kit: Local<Option<Kit>>,
    mut last: Local<Vec<(Shown, Entity)>>,
) {
    // Not over a menu: the paused ones leave `Playing`; the Pip-Boy up over a running game
    // (co-op's live Pip-Boy, the glance) hides them too, as JVO hides its own on
    // `MenuMode` [design: owner's playtest 2026-11-06, marks drew over the Pip-Boy].
    let playing = matches!(*state.get(), PlayState::Playing) && !crate::reticle::pipboy_hides_crosshair(pipboy.as_deref(), glance.as_deref());
    let (Some(loc), Ok((cam, ct)), Ok(window)) = (loc.as_deref(), cameras.single(), windows.single()) else {
        for (_, e) in last.drain(..) {
            commands.entity(e).despawn();
        }
        return;
    };
    let size = window.size();
    let eye = ct.translation();
    let mut want: Vec<(Shown, Vec2)> = Vec::new();
    if playing {
        let focus = |s: Vec2| {
            let d = Vec2::new((s.x - 0.5) * size.x / size.y.max(1.0), s.y - 0.5);
            d.length_squared() <= FOCUS_RADIUS * FOCUS_RADIUS
        };
        // Not over the drinking contest's screen (its roster names the drinkers; a tag lay
        // over its result banner, parity 2026-11-12), nor under its win shot, which outlasts
        // the loser's screen by a moment.
        let contest = drink.as_deref().is_some_and(|d| d.0.is_some()) || stage.as_deref().is_some_and(|w| w.stage.is_some());
        if tags.on && !contest {
            for m in mates.mates.iter().filter(|m| m.here && !m.lost && !hidden.0.contains(&m.reference)) {
                // Over its drawn head (queue item 137; seated on furniture too); its feet
                // until its puppet is drawn.
                let top = head_top(heads.0.get(&m.reference), m.feet);
                let Some(fade) = tag_fade(space::to_bevy(top).distance(eye), tags.near, tags.far) else { continue };
                let Some((s, true)) = tag_point(cam, ct, size, top) else { continue };
                if !(0.0..=1.0).contains(&s.x) || !(0.0..=1.0).contains(&s.y) {
                    continue;
                }
                want.push((Shown { kind: MarkKind::NameTag, label: m.name.clone(), distance: None, focus: focus(s), fade }, s));
            }
        }
        for p in pings.0.iter().filter(|p| p.msg.here(loc)) {
            let followed = sim.as_deref().filter(|_| p.msg.target != 0).filter(|_| p.msg.actor).and_then(|s| s.world.peek(fnv_sim::FormId(p.msg.target))).map(|r| [r.pos[0], r.pos[1], r.pos[2] + PING_ACTOR_HEIGHT]);
            let at = followed.unwrap_or(p.msg.pos);
            let Some((s, _)) = on_screen(cam, ct, size, at) else { continue };
            let s = Vec2::new(s.x.clamp(EDGE.x, 1.0 - EDGE.x), s.y.clamp(EDGE.y, 1.0 - EDGE.y));
            let feet = (space::to_bevy(at).distance(eye) / UNITS_PER_FOOT).round() as i64;
            want.push((Shown { kind: MarkKind::Ping, label: p.label.clone(), distance: Some(format!("{feet} ft")), focus: focus(s), fade: (1.0, 1.0) }, s));
        }
    }
    let bc = crate::interface::bc_supported(formats.as_deref());
    if !want.is_empty() && kit.is_none() {
        let face = face(FONT_FILES[2], plain_font(20.0, 10.0), data.as_deref(), &mut images, bc);
        let arrow = data.as_deref().and_then(|d| crate::interface::texture(ARROW, d, &mut images, bc)).map(|(h, _)| h);
        info!("world marks: {}", if arrow.is_some() { "JAM's JVO arrow found: pings use it" } else { "no JVO arrow in the load order: pings use a diamond" });
        *kit = Some(Kit { face, arrow });
    }
    // Off the HUD's pieces (`crate::safe_zone`): each mark's box over its point.
    if !want.is_empty() {
        zone.want();
    }
    if let Some(kit) = kit.as_ref() {
        for (shown, s) in want.iter_mut() {
            let area = mark_box(kit, shown);
            let p = *s * size;
            let d = zone.clear(Rect { min: p + area.min, max: p + area.max });
            *s = (p + d) / size;
        }
    }
    let same = want.len() == last.len() && want.iter().zip(last.iter()).all(|(a, b)| a.0 == b.0);
    if same {
        // Only moved: place the roots again.
        for ((_, s), (_, e)) in want.iter().zip(last.iter()) {
            commands.entity(*e).insert(root_node(*s, size));
        }
        return;
    }
    for (_, e) in last.drain(..) {
        commands.entity(e).despawn();
    }
    if want.is_empty() {
        return;
    }
    let Some(kit) = kit.as_ref() else { return };
    for (shown, s) in &want {
        let alpha = if shown.focus { 1.0 } else { AWAY_ALPHA } * shown.fade.0;
        let color = amber(255.0).with_alpha(alpha);
        let line_h = kit.face.font.size.max(16.0);
        let base = crate::interface::line_base(&kit.face);
        let root = commands.spawn((MarkRoot, crate::safe_zone::SafeZoneIgnore, root_node(*s, size), UiTransform::from_scale(Vec2::splat(shown.fade.1)), GlobalZIndex(3))).id();
        last.push((shown.clone(), root));
        commands.entity(root).with_children(|p| {
            let mut top = 0.0;
            if shown.kind == MarkKind::Ping {
                top -= ARROW_SIZE.y;
                match &kit.arrow {
                    Some(h) => {
                        p.spawn((
                            at(-ARROW_SIZE.x / 2.0, top, ARROW_SIZE.x, ARROW_SIZE.y),
                            ImageNode { color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(h.clone()) },
                            UiTransform::from_rotation(Rot2::radians(std::f32::consts::PI)),
                        ));
                    }
                    None => {
                        let side = 14.0;
                        p.spawn((
                            at(-side / 2.0, -side * 1.3, side, side),
                            BackgroundColor(color),
                            UiTransform::from_rotation(Rot2::radians(std::f32::consts::FRAC_PI_4)),
                        ));
                        top = -side * 2.2;
                    }
                }
            }
            for text in shown.distance.iter().chain(std::iter::once(&shown.label)) {
                let Some(line) = kit.face.font.lines(text.trim(), None).into_iter().next() else { continue };
                top -= line_h;
                set_line(p, &kit.face, &line, -line.width / 2.0, top + base, color);
            }
        });
    }
}

/// A mark's box about its point (logical pixels): the arrow (pings) with its tip on the
/// point, then each line over it, as [`draw`] lays them out.
fn mark_box(kit: &Kit, shown: &Shown) -> Rect {
    let line_h = kit.face.font.size.max(16.0);
    let (mut top, mut half) = (0.0f32, 0.0f32);
    if shown.kind == MarkKind::Ping {
        if kit.arrow.is_some() {
            top -= ARROW_SIZE.y;
            half = ARROW_SIZE.x / 2.0;
        } else {
            top = -14.0 * 2.2;
            half = 7.0 * std::f32::consts::SQRT_2;
        }
    }
    for text in shown.distance.iter().chain(std::iter::once(&shown.label)) {
        let Some(line) = kit.face.font.lines(text.trim(), None).into_iter().next() else { continue };
        top -= line_h;
        half = half.max(line.width / 2.0);
    }
    let k = shown.fade.1;
    Rect { min: Vec2::new(-half, top) * k, max: Vec2::new(half, 0.0) * k }
}

/// Test aid `NVF_PLAY_TEST_MARKS_LOG` (any value): every [`TRACE_SECONDS`], each other
/// player's tag point (its head bone, or its feet when no head is known), its feet, this
/// camera's eye and vertical field of view and where the point falls on the window, to
/// check a tag against the body in a frame saved at the same time.
fn trace_tags(
    mates: Res<SocialMates>,
    heads: Res<crate::emotes::MateHeads>,
    cameras: Query<(&Camera, &GlobalTransform, &Projection), With<PlayerCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut next: Local<Option<std::time::Instant>>,
) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ON.get_or_init(|| std::env::var_os("NVF_PLAY_TEST_MARKS_LOG").is_some()) {
        return;
    }
    let now = std::time::Instant::now();
    if next.is_some_and(|n| now < n) {
        return;
    }
    *next = Some(now + std::time::Duration::from_secs_f32(TRACE_SECONDS));
    let (Ok((cam, ct, proj)), Ok(window)) = (cameras.single(), windows.single()) else { return };
    let size = window.size();
    // The setting's (the projection as this frame's systems left it) and the one the
    // camera's matrix was made with (split screen's half, `nvforever` split/view.rs).
    let fov = match proj {
        Projection::Perspective(p) => p.fov.to_degrees(),
        _ => 0.0,
    };
    let drawn = 2.0 * (1.0 / cam.clip_from_view().y_axis.y.max(1e-6)).atan().to_degrees();
    let eye = space::to_game(ct.translation());
    for m in mates.mates.iter().filter(|m| m.here && !m.lost) {
        let head = heads.0.get(&m.reference);
        let p = head_top(head, m.feet);
        let at = tag_point(cam, ct, size, p).map(|(s, front)| format!("({:.0}, {:.0}){}", s.x * size.x, s.y * size.y, if front { "" } else { " behind" })).unwrap_or_else(|| "-".into());
        info!(
            "world marks trace: {} {:08X} {} ({:.0}, {:.0}, {:.0}) feet ({:.0}, {:.0}, {:.0}) eye ({:.0}, {:.0}, {:.0}) fov {fov:.1} drawn {drawn:.1} window {:.0}x{:.0}: at {at}",
            m.name,
            m.reference,
            if head.is_some() { "head" } else { "feet+" },
            p[0],
            p[1],
            p[2],
            m.feet[0],
            m.feet[1],
            m.feet[2],
            eye[0],
            eye[1],
            eye[2],
            size.x,
            size.y
        );
    }
}

/// The top of a player's drawn head (game space): over its head bone, or over its feet
/// until its puppet is drawn.
fn head_top(head: Option<&[f32; 3]>, feet: [f32; 3]) -> [f32; 3] {
    head.map_or([feet[0], feet[1], feet[2] + HEAD_HEIGHT], |h| [h[0], h[1], h[2] + HEAD_TOP])
}

/// Where a name tag's foot stands on the screen (parts of it) for a head whose top is at
/// `top`: [`OVER_HEAD`] over it as the camera draws it, but never more than
/// [`TAG_GAP_MAX`] of the view's height ([`tag_gap`]).
fn tag_point(cam: &Camera, t: &GlobalTransform, size: Vec2, top: [f32; 3]) -> Option<(Vec2, bool)> {
    let (s, front) = on_screen(cam, t, size, top)?;
    if !front {
        return Some((s, front));
    }
    let over = on_screen(cam, t, size, [top[0], top[1], top[2] + OVER_HEAD]).filter(|(_, f)| *f).map_or(0.0, |(o, _)| s.y - o.y);
    Some((Vec2::new(s.x, s.y - tag_gap(over)), front))
}

/// The gap between a head's top and its tag (parts of the view's height): `drawn`, the
/// world gap as the camera draws it, within 0 and [`TAG_GAP_MAX`].
fn tag_gap(drawn: f32) -> f32 {
    drawn.clamp(0.0, TAG_GAP_MAX)
}

/// How often [`trace_tags`] logs (seconds).
const TRACE_SECONDS: f32 = 5.0;

/// A mark's root: a zero-size node at its point on the screen.
fn root_node(s: Vec2, size: Vec2) -> Node {
    Node { position_type: PositionType::Absolute, left: Val::Px(s.x * size.x), top: Val::Px(s.y * size.y), width: u(0.0), height: u(0.0), ..default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tag's gap over the head: as drawn at a normal distance, capped close by or in a
    /// split-screen half (the 2026-11-20 relay frame: 115 of 720 pixels).
    #[test]
    fn a_name_tag_keeps_close_over_the_head() {
        assert_eq!(tag_gap(0.02), 0.02, "a normal distance in a full view: as drawn");
        assert_eq!(tag_gap(115.0 / 720.0), TAG_GAP_MAX, "close by in a half: capped");
        assert_eq!(tag_gap(-0.01), 0.0, "never under the head");
        assert!(TAG_GAP_MAX * 720.0 <= 26.0);
        let top = head_top(Some(&[10.0, 20.0, 130.0]), [10.0, 20.0, 0.0]);
        assert_eq!(top, [10.0, 20.0, 144.0]);
        assert_eq!(head_top(None, [10.0, 20.0, 0.0]), top, "a puppet not drawn yet: about the same");
    }

    #[test]
    fn a_name_tag_is_full_near_then_smaller_and_fainter_then_gone() {
        assert_eq!(tag_fade(0.0, 1500.0, 4000.0), Some((1.0, 1.0)));
        assert_eq!(tag_fade(1500.0, 1500.0, 4000.0), Some((1.0, 1.0)));
        let (a, k) = tag_fade(2750.0, 1500.0, 4000.0).unwrap();
        assert!((0.6..=0.7).contains(&a) && (k - 0.75).abs() < 1e-6, "halfway: {a} {k}");
        let (a2, k2) = tag_fade(3500.0, 1500.0, 4000.0).unwrap();
        assert!(a2 < a && k2 < k && k2 >= FAR_SCALE && a2 >= FAR_ALPHA, "3500: {a2} {k2}");
        // Still readable just before the cutoff, then gone.
        let (a3, k3) = tag_fade(3990.0, 1500.0, 4000.0).unwrap();
        assert!((a3 - FAR_ALPHA).abs() < 0.05 && (k3 - FAR_SCALE).abs() < 0.05);
        assert_eq!(tag_fade(4000.0, 1500.0, 4000.0), None);
        // A far distance not past the near one counts as 500 past it.
        assert!(tag_fade(2400.0, 2000.0, 1000.0).is_some());
        assert_eq!(tag_fade(2500.0, 2000.0, 1000.0), None);
    }

    #[test]
    fn the_fade_settings_go_to_their_choices_and_back() {
        for (choices, v) in [(&NEAR_CHOICES, NEAR_DEFAULT), (&FAR_CHOICES, FAR_DEFAULT)] {
            assert_eq!(value_of(choices, choice_of(choices, v)), v);
        }
    }

    #[test]
    fn a_ping_goes_over_the_wire_and_back() {
        let m = PingMsg { world: 0xDA726, interior: 0, pos: [-68375.5, 4800.0, 8600.25], target: 0x0010_4E84, clear: false, actor: true };
        assert_eq!(PingMsg::decode(&m.encode()), Some(m));
        let c = PingMsg { clear: true, ..m };
        assert_eq!(PingMsg::decode(&c.encode()), Some(c));
        assert_eq!(PingMsg::decode(&[2, 0]), None);
        assert_eq!(PingMsg::decode(&m.encode()[..10]), None);
    }

    #[test]
    fn a_players_new_ping_takes_the_place_of_its_last_and_a_quick_second_takes_it_away() {
        let mut p = Pings::default();
        let m = |x: f32| PingMsg { world: 1, interior: 0, pos: [x, 0.0, 0.0], target: 0, clear: false, actor: false };
        p.take(1, m(0.0), "a".into(), 0.0);
        p.take(2, m(500.0), "b".into(), 0.0);
        p.take(1, m(1000.0), "a2".into(), 5.0);
        assert_eq!(p.0.iter().map(|x| x.label.as_str()).collect::<Vec<_>>(), ["b", "a2"]);
        assert!(p.undoes(1, [1010.0, 0.0, 0.0], 6.0));
        assert!(!p.undoes(1, [1010.0, 0.0, 0.0], 7.0), "too late");
        assert!(!p.undoes(1, [2000.0, 0.0, 0.0], 6.0), "too far");
        p.take(1, PingMsg { clear: true, ..m(1000.0) }, String::new(), 6.0);
        assert_eq!(p.0.len(), 1);
    }

    #[test]
    fn a_ping_is_shown_only_in_its_place() {
        let m = PingMsg { world: 7, interior: 0, pos: [0.0; 3], target: 0, clear: false, actor: false };
        assert!(m.here(&Location { world: 7, interior: None }));
        assert!(!m.here(&Location { world: 8, interior: None }));
        assert!(!m.here(&Location { world: 7, interior: Some(9) }));
        let inside = PingMsg { interior: 9, ..m };
        assert!(inside.here(&Location { world: 3, interior: Some(9) }));
    }
}
