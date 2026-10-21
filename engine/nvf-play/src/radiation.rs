//! Environment radiation in play (docs/spec-notes/radiation.md): the water the player is
//! in told to the game, the Geiger counter's clicks, the HUD's radiation meter, and the
//! log. The rule (markers, water, Rad Resistance, the rads taken) is
//! `fnv_game::radiation`; the sickness stages are `fnv_game::needs`.
//!
//! - **Water** ([`tell_water`]): the player is in water when the character swims, or wades
//!   with the feet under the surface the physics knows at that point; the water type is
//!   the cell's (its XCWT, else the worldspace's) [hypothesis: placed water volumes are
//!   taken as the cell's type].
//! - **Geiger counter** ([`geiger`]): while the level is above 0, one of the counter's
//!   one-second bursts every second: `UIPipBoyRadiationA` below [`BAND_B`] rads a
//!   second, `…B` below [`BAND_C`], `…C` from there [observed: the three SOUN records
//!   name folders of three one-second takes each, `a`, `b`, `c`; hypothesis: that they
//!   are rising strengths, the bands and the rhythm].
//! - **Meter** ([`draw`]): `RadiationMeter` of `menus\main\hud_main_menu.xml` while rads
//!   are taken: the rads a second (`radiation_text_value`, font 8, right-justified at x
//!   237) and "RADS" (`radiation_text`, font 7, from x 245), in the HUD's alternative
//!   colour (`&hudalt;`) [documented: the menu file; hypothesis: the texts, the word, and
//!   the place: the file leaves it to the code, set here just above the health piece].
//! - **Explosion zones**: `crate::combat` leaves them when an explosion with radiation
//!   goes off (`Game::add_radiation_zone`); here they are logged as they fade away.
//! - Logged: `radiation: level <l> rads/s (<source>), taking <r>/s (Rad Resistance
//!   <rr> %), rads <n>` when the source or the rounded level changes,
//!   `radiation: the zone of <explosion> has faded`, the water's damage hits,
//!   `radiation: out of radiation, rads <n>`, `geiger: <sound> (level <l>)` when the
//!   band changes, and at exit `radiation summary: …`.

use crate::hud::HudView;
use crate::interface::{at, bc_supported, face, plain_font, set_text, texture, u, Face, Set};
use crate::phys::cells::Space;
use crate::phys::PhysState;
use crate::reticle::crime_colour;
use crate::script::{with_world, ScriptState};
use crate::sfx::{PlaySound, Sfx};
use crate::sim::SimState;
use crate::{PlaySet, PlayState, PlayWorld};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_game::radiation::{Exposure, WaterContact};
use fnv_sim::FormId;
use fnv_world::cell_of;
use nvf_assets::GameData;

/// The level (rads a second) from which the counter plays its `B` and `C` bursts.
pub const BAND_B: f64 = 5.0;
pub const BAND_C: f64 = 10.0;
/// Seconds between bursts (each take is about a second long [observed: 88,2xx bytes of
/// 16-bit 44.1 kHz mono]).
const BURST: f32 = 1.0;

const SOUNDS: [&str; 3] = ["UIPipBoyRadiationA", "UIPipBoyRadiationB", "UIPipBoyRadiationC"];

/// The counter's band at a level: 0 silent, else 1 to 3 (A to C).
pub fn band(level: f64) -> usize {
    if level <= 0.0 {
        0
    } else if level < BAND_B {
        1
    } else if level < BAND_C {
        2
    } else {
        3
    }
}

// ---- the menu file's numbers [documented: hud_main_menu.xml `RadiationMeter`] ----------

const PIECE: Vec2 = Vec2::new(367.0, 60.0);
const BRACKET_PATH: &str = "textures\\interface\\hud\\glow_messages_radiation_seperator_right.dds";
const BRACKET_AT: f32 = -135.0;
const BRACKET: Vec2 = Vec2::new(567.0, 120.0);
const VALUE_END: f32 = 237.0;
const WORD_X: f32 = 245.0;
const TEXT_TOP: f32 = 65.0;
const VALUE_FONT: &str = "textures\\fonts\\glow_futura_caps_large.fnt";
const WORD_FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";

// ---- set here [hypothesis] ---------------------------------------------------------------

/// The word [hypothesis: `sStatsRADAbbrev` is set by no master].
const WORD: &str = "RADS";
/// The meter's place: the health piece's left margin, its bottom just over the piece
/// (`hud.rs`: 30 from the bottom, 127 high).
const LEFT: f32 = 40.0;
const BOTTOM: f32 = 160.0;
/// The texts' line, from the piece's top (the file's y 65 is taken as the top of the
/// text, the font's base about 28 below it).
const BASE: f32 = TEXT_TOP - 10.0;

/// What the last level line named: the marker, the zone's explosion, the water, and
/// the level in tenths.
type Shown = (Option<FormId>, Option<FormId>, Option<FormId>, i64);

/// What the log and the exit summary keep.
#[derive(Resource, Default, Debug)]
pub struct RadLog {
    shown: Option<Shown>,
    /// The explosion zones seen: serial and explosion.
    zones: Vec<(u32, FormId)>,
    pub zones_left: u32,
    water_hits: u32,
    band: usize,
    pub max_level: f64,
    pub seconds: f64,
    pub rads_start: Option<f64>,
    pub rads: f64,
    pub bursts: u32,
}

pub struct RadiationPlugin;

impl Plugin for RadiationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RadLog>()
            .add_systems(Update, (tell_water, watch, geiger, draw).chain().in_set(PlaySet::Ui).after(crate::hud::HudSet))
            .add_systems(Last, summary);
    }
}

/// Tell the game what water the player is in.
fn tell_water(phys: Option<Res<PhysState>>, d: Option<Res<PlayWorld>>, mut script: Option<ResMut<ScriptState>>, mut last: Local<Option<WaterContact>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let (Some(phys), Some(d)) = (phys, d) else { return };
    let contact = phys.player.as_ref().and_then(|c| {
        let surface = phys.world.water_surface_at(c.feet[0], c.feet[1])?;
        if !c.swimming && c.feet[2] >= surface {
            return None;
        }
        let ws = d.index.worldspaces.get(&phys.worldspace);
        let water = match phys.space {
            Space::Interior(id) => d.index.cells.get(&id).and_then(|cell| cell.water_type),
            Space::Exterior => {
                let cell = ws.and_then(|w| w.grid.get(&cell_of(c.feet[0], c.feet[1])));
                cell.and_then(|id| d.index.cells.get(id)).and_then(|cell| cell.water_type)
            }
        }
        .or(ws.and_then(|w| w.water))?;
        Some(WaterContact { water: FormId(water), swimming: c.swimming })
    });
    if *last != contact {
        match contact {
            Some(w) => info!(
                "radiation: the player is {} in water {}{}",
                if w.swimming { "swimming" } else { "wading" },
                rt.game.data.name(w.water),
                rt.game.data.water_damage.get(&w.water).map(|d| format!(" (it causes {d:.0} damage a second)")).unwrap_or_default()
            ),
            None if last.is_some() => info!("radiation: the player is out of the water"),
            None => {}
        }
        *last = contact;
    }
    rt.game.set_player_water(contact);
}

/// The exposure and the player's rads after the frame's tick, logged on change.
fn watch(time: Res<Time>, state: Res<State<PlayState>>, mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>, mut log: ResMut<RadLog>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let e = rt.game.radiation();
    let rads = with_world(rt, &mut sim, |rt| fnv_game::functions::actor_value(&mut rt.game, FormId::PLAYER_REF, fnv_game::aid::RAD_LEVEL).unwrap_or(0.0));
    log.rads_start.get_or_insert(rads);
    log.rads = rads;
    if *state.get() != PlayState::Playing {
        return;
    }
    // Explosion zones that faded since the last frame.
    let now: Vec<(u32, FormId)> = rt.game.radiation_zones().iter().map(|z| (z.serial, z.explosion)).collect();
    for &(serial, explosion) in log.zones.iter().filter(|(s, _)| !now.iter().any(|(n, _)| n == s)) {
        info!("radiation: the zone of {} (#{serial}) has faded", rt.game.data.name(explosion));
    }
    log.zones_left += now.iter().filter(|(s, _)| !log.zones.iter().any(|(o, _)| o == s)).count() as u32;
    log.zones = now;
    if e.water_hits > log.water_hits {
        if let Some(w) = e.water {
            info!("radiation: water {} takes {:.0} health ({} hit(s) in all)", rt.game.data.name(w.water), e.water_damage, e.water_hits);
        }
        log.water_hits = e.water_hits;
    }
    // A fading zone is logged by whole rads (a tenth would be a line every few frames).
    let step = if e.zone.is_some() { (e.level.round() * 10.0) as i64 } else { (e.level * 10.0).round() as i64 };
    let key = (e.level > 0.0).then(|| (e.marker, e.zone, e.water.filter(|_| e.from_water > 0.0).map(|w| w.water), step));
    if key != log.shown {
        match key {
            Some(_) => info!("radiation: level {:.1} rads/s ({}), taking {:.2}/s (Rad Resistance {:.0} %), rads {rads:.1}", e.level, source(&e, rt.game.data), e.rate, e.resist),
            None => info!("radiation: out of radiation, rads {rads:.1}"),
        }
        log.shown = key;
    }
    if e.level > 0.0 {
        log.max_level = log.max_level.max(e.level);
        log.seconds += time.delta_secs_f64();
    }
}

fn source(e: &Exposure, data: &fnv_game::GameData) -> String {
    let mut parts = Vec::new();
    if let Some(m) = e.marker {
        parts.push(format!("marker {m} {:.1}", e.markers));
    }
    if let Some(z) = e.zone {
        parts.push(format!("zone of {} {:.1}", data.name(z), e.markers));
    }
    if let Some(w) = e.water.filter(|_| e.from_water > 0.0) {
        parts.push(format!("{} {} {:.2}", if w.swimming { "swimming in" } else { "wading in" }, data.name(w.water), e.from_water));
    }
    parts.join(", ")
}

/// The Geiger counter: a burst a second while there is radiation.
#[allow(clippy::too_many_arguments)]
fn geiger(
    time: Res<Time>,
    state: Res<State<PlayState>>,
    script: Option<Res<ScriptState>>,
    sfx: Option<Res<Sfx>>,
    mut sounds: MessageWriter<PlaySound>,
    mut log: ResMut<RadLog>,
    mut since: Local<f32>,
    mut forms: Local<Option<[Option<FormId>; 3]>>,
) {
    let level = script.as_deref().and_then(|s| s.rt.as_ref()).map_or(0.0, |rt| rt.game.radiation().level);
    let b = if *state.get() == PlayState::Playing && !script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.in_menu()) { band(level) } else { 0 };
    if b != log.band {
        if b > 0 {
            info!("geiger: {} (level {level:.1})", SOUNDS[b - 1]);
        } else {
            info!("geiger: quiet");
        }
        log.band = b;
        // A new band clicks at once.
        *since = BURST;
    }
    if b == 0 {
        return;
    }
    *since += time.delta_secs();
    if *since < BURST {
        return;
    }
    *since = 0.0;
    let Some(sfx) = sfx.as_deref() else { return };
    let forms = forms.get_or_insert_with(|| {
        SOUNDS.map(|name| sfx.forms.sounds.iter().find(|(_, s)| s.edid.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(name))).map(|(f, _)| *f))
    });
    match forms[b - 1] {
        Some(sound) => {
            sounds.write(PlaySound { sound, at: None });
            log.bursts += 1;
        }
        None => warn_once!("geiger: sound {} not found", SOUNDS[b - 1]),
    }
}

#[derive(Component)]
struct MeterRoot;
#[derive(Component)]
struct Texts;

struct Kit {
    bracket: Option<Handle<Image>>,
    value: Face,
    word: Face,
}

/// The meter while rads are taken and the HUD is up.
#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    view: Res<HudView>,
    script: Option<Res<ScriptState>>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut kit: Local<Option<Kit>>,
    mut shown: Local<Option<String>>,
    mut root: Query<&mut Visibility, With<MeterRoot>>,
    texts: Query<Entity, With<Texts>>,
) {
    let rate = script.as_deref().and_then(|s| s.rt.as_ref()).map_or(0.0, |rt| rt.game.radiation().rate);
    let on = view.shown && rate > 0.0;
    if kit.is_none() {
        if !on {
            return;
        }
        let bc = bc_supported(formats.as_deref());
        let d = data.as_deref();
        let loaded = Kit {
            bracket: d.and_then(|d| texture(BRACKET_PATH, d, &mut images, bc)).map(|t| t.0),
            value: face(VALUE_FONT, plain_font(52.0, 22.0), d, &mut images, bc),
            word: face(WORD_FONT, plain_font(36.0, 15.0), d, &mut images, bc),
        };
        spawn(&mut commands, &loaded);
        *kit = Some(loaded);
        *shown = None;
        return;
    }
    for mut v in &mut root {
        v.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
    }
    if !on {
        return;
    }
    let Some(kit) = kit.as_ref() else { return };
    // Whole rads a second, at least 1 while any are taken [hypothesis].
    let value = format!("+{}", rate.round().max(1.0) as i64);
    if shown.as_deref() == Some(value.as_str()) {
        return;
    }
    let Ok(e) = texts.single() else { return };
    let colour = crime_colour();
    commands.entity(e).despawn_related::<Children>().with_children(|p| {
        if let Some(l) = kit.value.font.lines(&value, None).into_iter().next() {
            let x = VALUE_END - l.width;
            set_text(p, &kit.value, &Set { line: l, x, base: BASE }, Vec2::ZERO, colour, ());
        }
        if let Some(l) = kit.word.font.lines(WORD, None).into_iter().next() {
            set_text(p, &kit.word, &Set { line: l, x: WORD_X, base: BASE }, Vec2::ZERO, colour, ());
        }
    });
    *shown = Some(value);
}

fn spawn(commands: &mut Commands, kit: &Kit) {
    let piece = Node { position_type: PositionType::Absolute, left: u(LEFT), bottom: u(BOTTOM), width: u(PIECE.x), height: u(PIECE.y), ..default() };
    commands.spawn((MeterRoot, piece, Visibility::Hidden, Pickable::IGNORE)).with_children(|p| {
        if let Some(image) = &kit.bracket {
            p.spawn((at(BRACKET_AT, 0.0, BRACKET.x, BRACKET.y), ImageNode { color: crime_colour(), ..ImageNode::new(image.clone()) }));
        }
        p.spawn((Texts, at(0.0, 0.0, PIECE.x, PIECE.y)));
    });
}

/// At exit: how much radiation the run met.
fn summary(mut exit: MessageReader<AppExit>, log: Res<RadLog>) {
    if exit.read().next().is_none() {
        return;
    }
    let start = log.rads_start.unwrap_or(0.0);
    info!(
        "radiation summary: {:.1} s in radiation, highest level {:.1} rads/s, rads {start:.1} -> {:.1} (+{:.1}), {} Geiger bursts, {} explosion zone(s), {} water hit(s)",
        log.seconds,
        log.max_level,
        log.rads,
        log.rads - start,
        log.bursts,
        log.zones_left,
        log.water_hits
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_counter_rises_through_its_three_bands() {
        assert_eq!(band(0.0), 0);
        assert_eq!(band(0.2), 1);
        assert_eq!(band(4.9), 1);
        assert_eq!(band(5.0), 2);
        assert_eq!(band(10.0), 3);
        assert_eq!(band(720.0), 3);
    }
}
