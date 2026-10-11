//! Sounds of the world's objects (docs/spec-notes/audio.md §11): what a shot's impact
//! picks, loose objects knocking, items picked up and dropped, containers opened and
//! closed, activators used, and the sounds scripts ask for.
//!
//! - **Impacts** (played by `crate::combat`): [`log_impact`] tells what a hit picked,
//!   from the collision material to the IPCT and its sounds; counted by material.
//! - **Knocks** [hypothesis for which sound, design for when]: a loose (dynamic) body,
//!   placed clutter or a dropped item, that loses at least [`KNOCK_LIGHT`] units/s of
//!   its speed in a frame ([`knock_speed`]) plays the PHY sound of its collision
//!   material ([`phy_sound`]): the `…H` one from [`KNOCK_HARD`] up, else the `…L` one,
//!   at most once per [`KNOCK_GAP`] seconds per body and not in its first
//!   [`KNOCK_SETTLE`] seconds. No record links the PHY sounds to materials; the table
//!   goes by their names.
//! - **Items** [documented: xEdit, the `YNAM` Sound - Pick Up and `ZNAM` Sound - Drop of
//!   MISC, KEYM, WEAP, ARMO, AMMO, ALCH, BOOK, NOTE, IMOD, CHIP, CCRD, CMNY]: a
//!   reference taken out of the world (`RefState::deleted` on its `RefChanged`) plays
//!   its base's pick-up sound, a reference a drop creates plays its drop sound, both at
//!   the reference [hypothesis: where]; an item moved between the player and an open
//!   container plays the pick-up sound when taken and the drop sound when put in, at the
//!   listener. An item whose record names none plays `UIItemGenericUp` / `…Down`
//!   [hypothesis: by the names].
//! - **Message boxes**: `UIPopUpMessageWindow` when one opens [design: by its name; the
//!   data names no sound for it], the buttons' `UIMenuOK` when one is picked and
//!   `UIMenuFocus` when the choice moves [observed: menus\prefabs\list_box_template.xml].
//! - **Containers** [documented: xEdit CONT `SNAM` Sound - Open, `QNAM` Sound - Close]:
//!   when the container menu opens on a container and when it closes again.
//! - **Activators** [documented: xEdit ACTI `VNAM` Sound - Activation]: when the player
//!   activates one (its script's OnActivate or its default action), at the activator.
//!   Activators with a radio station (`RNAM`) are left to the radio.
//! - **Scripts** (`fnv_game::sound`): `PlaySound` at the listener, `PlaySound3D` at the
//!   calling reference. Without its SystemSound flag a `PlaySound` asked for in a menu
//!   waits until the game is back [documented: GECK wiki PlaySound].
//! - **Objects' own sounds** [documented: xEdit ACTI `SNAM` Sound - Looping, CONT `RNAM`
//!   Sound - Random/Looping, DOOR `BNAM` Sound - Looping]: every placed activator,
//!   container and door whose base names one is a [`SoundSource`] where it stands, while
//!   it is enabled; `crate::ambient` plays it as a sound marker of that sound (a Loop
//!   sound goes round, a Play At Random one sounds now and then) [design: the records'
//!   sounds carry either flag, audio.md §14].
//! - **Load doors** [hypothesis]: activating a door that leads elsewhere plays its open
//!   sound (DOOR `SNAM`), at the listener, as the player goes through.
//! - Not built: a knock between two loose objects told apart from one on the ground
//!   (both play the moving body's sound). The casings' sounds are nvforever's
//!   (`shot_fx.rs`).
//!
//! A `world sounds summary` line at the end.

use crate::menu::model::Mode;
use crate::menu::ActiveMenu;
use crate::phys::PhysState;
use crate::player::Player;
use crate::sfx::{PlaySound, Sfx, SoundSource};
use crate::sim::SimState;
use crate::{PlaySet, PlayState, PlayWorld, RefChanged};
use bevy::prelude::*;
use fnv_esm::{LoadOrder, Record};
use fnv_game::sound::SoundRequest;
use fnv_sim::FormId;
use std::collections::{BTreeMap, HashMap, HashSet};

/// A change of velocity (game units per second, gravity's share taken off) that knocks.
pub const KNOCK_LIGHT: f32 = 90.0;
/// … that knocks hard.
pub const KNOCK_HARD: f32 = 300.0;
/// Seconds between two knocks of one body.
pub const KNOCK_GAP: f64 = 0.12;
/// Seconds after a body's collision comes (its cell loads, it is dropped) in which it
/// settles without a sound; a dropped item makes its drop sound instead.
pub const KNOCK_SETTLE: f64 = 1.5;

/// The 32 Havok materials, by code mod 32 [documented: nif.xml `Fallout3HavokMaterial`,
/// `schemas/nif/havok.json`]; 32–127 are the same as platforms and stairs.
pub const HAVOK_MATERIALS: [&str; 32] = [
    "STONE",
    "CLOTH",
    "DIRT",
    "GLASS",
    "GRASS",
    "METAL",
    "ORGANIC",
    "SKIN",
    "WATER",
    "WOOD",
    "HEAVY_STONE",
    "HEAVY_METAL",
    "HEAVY_WOOD",
    "CHAIN",
    "BOTTLECAP",
    "ELEVATOR",
    "HOLLOW_METAL",
    "SHEET_METAL",
    "SAND",
    "BROKEN_CONCRETE",
    "VEHICLE_BODY",
    "VEHICLE_PART_SOLID",
    "VEHICLE_PART_HOLLOW",
    "BARREL",
    "BOTTLE",
    "SODA_CAN",
    "PISTOL",
    "RIFLE",
    "SHOPPING_CART",
    "LUNCHBOX",
    "BABY_RATTLE",
    "RUBBER_BALL",
];

/// The name of a Havok material code.
pub fn havok_name(code: u32) -> &'static str {
    HAVOK_MATERIALS[(code % 32) as usize]
}

/// The SOUN (Editor ID) a loose body of Havok material `code` plays when it knocks, hard
/// or light [hypothesis: by the names, `PHY<material>H` / `…L`, and `CWood…` whose files
/// are `fx\phy\wood\crate_*`; observed in FalloutNV.esm]. Where a family comes in sizes
/// rather than H and L, a hard knock takes the larger [design]. `None`: no sound for
/// that material (dirt, grass, water, sand).
pub fn phy_sound(code: u32, hard: bool) -> Option<&'static str> {
    let (h, l) = match code % 32 {
        0 | 10 => ("PHYGenericConcMediumH", "PHYGenericConcMediumL"),
        1 => ("PHYGenericClothH", "PHYGenericClothL"),
        3 | 24 => ("PHYBottleH", "PHYBottleL"),
        5 => ("PHYGenericMetalMediumH", "PHYGenericMetalMediumL"),
        6 | 7 => ("PHYSkinMediumH", "PHYSkinMediumL"),
        9 => ("CWoodMedium", "CWoodSmall"),
        11 | 15 => ("PHYGenericMetalLargeH", "PHYGenericMetalLargeL"),
        12 => ("CWoodLarge", "CWoodMedium"),
        13 => ("PHYChain", "PHYChain"),
        14 => ("PHYBottlecaps", "PHYBottlecaps"),
        16 => ("PHYGenericMetalHollowMedium", "PHYGenericMetalHollowSmall"),
        17 => ("PHYGenericMetalSheetMedium", "PHYGenericMetalSheetSmall"),
        19 => ("PHYGenericConcSmall", "PHYGenericConcSmall"),
        20 => ("PHYVehicleMetalBodyH", "PHYVehicleMetalBodyL"),
        21 => ("PHYVehicleMetalSolidH", "PHYVehicleMetalSolidL"),
        22 => ("PHYVehicleMetalHollowH", "PHYVehicleMetalHollowL"),
        23 => ("PHYBarrelH", "PHYBarrelL"),
        25 => ("PHYCanSodaH", "PHYCanSodaL"),
        26 => ("PHYWeaponPistolH", "PHYWeaponPistolL"),
        27 => ("PHYWeaponRifleH", "PHYWeaponRifleL"),
        28 => ("PHYShoppingCartH", "PHYShoppingCartL"),
        29 => ("PHYLunchboxH", "PHYLunchboxL"),
        30 => ("PHYBabyRattle", "PHYBabyRattle"),
        31 => ("PHYRubberBall", "PHYRubberBall"),
        _ => return None,
    };
    Some(if hard { h } else { l })
}

/// How hard a body was knocked between two velocities: the speed it lost along the way
/// it was going (all of it on landing, twice it on a bounce straight back) [design]. A
/// body gaining speed (falling, pushed by a shot) or at rest is not knocked.
pub fn knock_speed(before: [f32; 3], after: [f32; 3]) -> f32 {
    let speed = (before[0] * before[0] + before[1] * before[1] + before[2] * before[2]).sqrt();
    if speed < 1e-3 {
        return 0.0;
    }
    let along = (after[0] * before[0] + after[1] * before[1] + after[2] * before[2]) / speed;
    (speed - along).max(0.0)
}

/// The sound fields of the records that have them, by base FormID.
#[derive(Default)]
pub struct SoundForms {
    /// Every item record: (pick up, drop), as the record names them.
    pub items: HashMap<FormId, (Option<FormId>, Option<FormId>)>,
    /// Per item record type: records, those naming a pick-up sound, a drop sound.
    pub counts: BTreeMap<String, [u32; 3]>,
    /// Containers: (open, close).
    pub containers: HashMap<FormId, (Option<FormId>, Option<FormId>)>,
    /// Activators without a radio station: their activation sound.
    pub activators: HashMap<FormId, FormId>,
    /// Activators' (SNAM), containers' (RNAM) and doors' (BNAM) own sounds.
    pub object_sounds: HashMap<FormId, FormId>,
    /// … how many records of each kind name one.
    pub object_sound_counts: [u32; 3],
    /// Doors' open sounds (SNAM), for load doors.
    pub door_open: HashMap<FormId, FormId>,
}

const ITEM_SIGS: [&[u8; 4]; 12] = [b"MISC", b"KEYM", b"WEAP", b"ARMO", b"AMMO", b"ALCH", b"BOOK", b"NOTE", b"IMOD", b"CHIP", b"CCRD", b"CMNY"];

/// A FormID subrecord of a record (plugin `pi`) as a global FormID; null and unresolved:
/// `None`. The subrecords read here are all `formid` [documented: xEdit].
fn raw_fid(lo: &LoadOrder, pi: usize, r: &Record, sig: &[u8; 4]) -> Option<FormId> {
    let subs = r.subrecords().ok()?;
    let s = subs.iter().find(|s| &s.sig == sig)?;
    let local = u32::from_le_bytes(s.data.get(..4)?.try_into().ok()?);
    if local == 0 {
        return None;
    }
    fnv_sim::extract::global(lo, pi, local).ok().flatten()
}

impl SoundForms {
    pub fn build(lo: &LoadOrder) -> SoundForms {
        let mut out = SoundForms::default();
        for (&g, defs) in &lo.index {
            let Some(d) = defs.last() else { continue };
            let pi = d.plugin as usize;
            let r = lo.records[pi][d.ordinal as usize];
            if r.header.flags & 0x20 != 0 {
                continue;
            }
            let sig = &r.header.sig;
            let id = FormId(g);
            if ITEM_SIGS.contains(&sig) {
                let (up, down) = (raw_fid(lo, pi, r, b"YNAM"), raw_fid(lo, pi, r, b"ZNAM"));
                let c = out.counts.entry(String::from_utf8_lossy(sig).into_owned()).or_default();
                c[0] += 1;
                c[1] += up.is_some() as u32;
                c[2] += down.is_some() as u32;
                out.items.insert(id, (up, down));
            } else if sig == b"CONT" {
                let (open, close) = (raw_fid(lo, pi, r, b"SNAM"), raw_fid(lo, pi, r, b"QNAM"));
                if open.is_some() || close.is_some() {
                    out.containers.insert(id, (open, close));
                }
                if let Some(l) = raw_fid(lo, pi, r, b"RNAM") {
                    out.object_sounds.insert(id, l);
                    out.object_sound_counts[1] += 1;
                }
            } else if sig == b"ACTI" {
                if let Some(l) = raw_fid(lo, pi, r, b"SNAM") {
                    out.object_sounds.insert(id, l);
                    out.object_sound_counts[0] += 1;
                }
                if raw_fid(lo, pi, r, b"RNAM").is_some() {
                    continue;
                }
                if let Some(v) = raw_fid(lo, pi, r, b"VNAM") {
                    out.activators.insert(id, v);
                }
            } else if sig == b"DOOR" {
                if let Some(l) = raw_fid(lo, pi, r, b"BNAM") {
                    out.object_sounds.insert(id, l);
                    out.object_sound_counts[2] += 1;
                }
                if let Some(o) = raw_fid(lo, pi, r, b"SNAM") {
                    out.door_open.insert(id, o);
                }
            }
        }
        out
    }
}

/// The player activated a reference by hand (`crate::activate`): its activation sound.
#[derive(Message, Clone, Copy, Debug)]
pub struct Activated {
    pub reference: u32,
}

#[derive(Default, Debug)]
pub struct WorldSoundStats {
    /// Impacts by the material picked (`fnv_combat::fx::IMPACT_MATERIALS`).
    pub impacts: BTreeMap<&'static str, u32>,
    /// Impacts on a reference whose collision has no material (they count as dirt).
    pub impacts_no_material: u32,
    pub knocks: u32,
    pub knocks_hard: u32,
    /// Knocks of a material with no PHY sound.
    pub knocks_unsounded: u32,
    pub pickups: u32,
    pub drops: u32,
    /// Items moved in a container menu: taken by the player, put in.
    pub taken: u32,
    pub put: u32,
    /// Of those taken, by Take All: their sounds held back for `UIItemTakeAll`
    /// (`crate::ui_sounds`).
    pub taken_quietly: u32,
    pub boxes_opened: u32,
    pub boxes_closed: u32,
    pub container_opens: u32,
    pub container_closes: u32,
    pub activations: u32,
    /// Load doors gone through with an open sound.
    pub load_doors: u32,
    /// Placed objects with a sound of their own, and those in reach (sent to the ambient).
    pub object_sources: usize,
    pub object_sources_sent: HashSet<u32>,
    pub script_2d: u32,
    pub script_3d: u32,
    /// Script sounds held while a menu was open.
    pub script_held: u32,
}

/// A loose body being followed for knocks.
struct Tracked {
    vel: [f32; 3],
    last_knock: f64,
    /// Real time it was first seen.
    since: f64,
}

#[derive(Resource, Default)]
pub struct WorldSounds {
    forms: SoundForms,
    /// SOUN FormIDs by Editor ID (from `Sfx`, the first frame it is there).
    by_edid: Option<HashMap<String, FormId>>,
    /// The player's inventory while a container menu is open, to hear what moves.
    carried: Option<BTreeMap<FormId, i32>>,
    /// The message box on screen (its text) and its chosen button.
    message: Option<(String, usize)>,
    tracked: HashMap<u32, Tracked>,
    steps: u64,
    picked: HashSet<u32>,
    dropped: HashSet<u32>,
    container: Option<u32>,
    held: Vec<SoundRequest>,
    /// Placed objects with a sound of their own, by the space they are in: (reference,
    /// sound); found the first frame.
    sources: Option<HashMap<crate::music::Space, Vec<(u32, FormId)>>>,
    pub stats: WorldSoundStats,
}

/// Log what a shot's impact picked (`crate::combat`), for the summary's count.
pub fn log_impact(s: &Sfx, hit: Option<u32>, actor: bool, code: Option<u32>, material: &'static str, impact: Option<FormId>) -> ImpactPicked {
    let i = impact.and_then(|i| s.forms.impacts.get(&i));
    let sounds: Vec<String> = i
        .map(|i| i.sound_1.into_iter().chain(i.sound_2).map(|x| s.forms.sounds.get(&x).and_then(|x| x.edid.clone()).unwrap_or_else(|| x.to_string())).collect())
        .unwrap_or_default();
    debug!(
        "impact: {} {}: collision material {}, impact material {material}, IPCT {}, sounds [{}]",
        if actor { "actor" } else { "object" },
        hit.map_or("terrain".to_string(), |h| format!("{h:08X}")),
        code.map_or(if actor { "-".to_string() } else { "none".to_string() }, |c| format!("{c} {}", havok_name(c))),
        i.and_then(|i| i.edid.as_deref()).unwrap_or("none"),
        sounds.join(", ")
    );
    ImpactPicked { material, no_material: !actor && hit.is_some() && code.is_none() }
}

/// What an impact picked, for the summary.
#[derive(Message, Clone, Copy, Debug)]
pub struct ImpactPicked {
    pub material: &'static str,
    /// A reference whose collision has no material.
    pub no_material: bool,
}

fn init(mut commands: Commands, d: Res<PlayWorld>) {
    let t = std::time::Instant::now();
    let forms = SoundForms::build(d.lo);
    info!(
        "world sounds: items (records, with a pick-up sound, with a drop sound): {}; {} containers with open or close sounds, {} activators with an activation sound; sounds of their own: {} activators (SNAM), {} containers (RNAM), {} doors (BNAM); read in {:.0} ms",
        forms.counts.iter().map(|(s, c)| format!("{s} {} {} {}", c[0], c[1], c[2])).collect::<Vec<_>>().join(", "),
        forms.containers.len(),
        forms.activators.len(),
        forms.object_sound_counts[0],
        forms.object_sound_counts[1],
        forms.object_sound_counts[2],
        t.elapsed().as_secs_f64() * 1000.0
    );
    commands.insert_resource(WorldSounds { forms, ..Default::default() });
}

/// SOUN FormIDs by Editor ID.
fn by_edid(sfx: &Sfx) -> HashMap<String, FormId> {
    sfx.forms.sounds.iter().filter_map(|(&id, s)| s.edid.clone().map(|e| (e, id))).collect()
}

/// The sound an item makes when picked up (`up`) or put down: its record's, else the
/// generic one [hypothesis: `UIItemGenericUp` / `UIItemGenericDown`, by their names;
/// no record links them].
fn item_sound(ws: &mut WorldSounds, sfx: Option<&Sfx>, item: FormId, up: bool) -> Option<FormId> {
    let &(u, d) = ws.forms.items.get(&item)?;
    let named = if up { u } else { d };
    if named.is_some() {
        return named;
    }
    let map = ws.by_edid.get_or_insert_with(|| sfx.map(by_edid).unwrap_or_default());
    map.get(if up { "UIItemGenericUp" } else { "UIItemGenericDown" }).copied()
}

impl WorldSounds {
    /// The sound an item makes when picked up (`up`) or put down, for the menus
    /// (`crate::ui_sounds`): as [`item_sound`] picks it.
    pub fn item_sound_of(&mut self, sfx: Option<&Sfx>, item: FormId, up: bool) -> Option<FormId> {
        item_sound(self, sfx, item, up)
    }
}

/// A SOUN's Editor ID for logs.
fn sound_name(sfx: Option<&Sfx>, sound: Option<FormId>) -> String {
    match sound {
        None => "none".to_string(),
        Some(s) => sfx.and_then(|x| x.forms.sounds.get(&s)).and_then(|x| x.edid.clone()).unwrap_or_else(|| s.to_string()),
    }
}

/// Where a reference is now (game space).
fn ref_pos(sim: &SimState, id: u32) -> Option<[f32; 3]> {
    sim.world.peek(FormId(id)).map(|s| s.pos).or_else(|| sim.refs.get(FormId(id)).map(|p| p.pos))
}

/// Knocks of loose bodies (see the module notes).
fn knocks(mut ws: ResMut<WorldSounds>, sfx: Option<Res<Sfx>>, phys: Option<Res<PhysState>>, time: Res<Time<Real>>, mut out: MessageWriter<PlaySound>) {
    let (Some(sfx), Some(phys)) = (sfx, phys) else { return };
    let ws = &mut *ws;
    if ws.by_edid.is_none() {
        // Once: what the actors' records say they are made of, for the log.
        let mut n: BTreeMap<&str, u32> = BTreeMap::new();
        for &k in sfx.forms.actor_impact.values() {
            *n.entry(fnv_combat::fx::IMPACT_MATERIALS[k]).or_default() += 1;
        }
        info!("world sounds: actors' impact materials (NPC_ and CREA NAM4): {}", n.iter().map(|(m, c)| format!("{m} {c}")).collect::<Vec<_>>().join(", "));
    }
    let phy = ws.by_edid.get_or_insert_with(|| by_edid(&sfx));
    let now = time.elapsed_secs_f64();
    let steps = phys.world.steps();
    let stepped = steps != ws.steps;
    ws.steps = steps;
    let bodies = phys.placed_dynamic.keys().copied().chain(phys.created.iter().filter(|(_, c)| c.dynamic).map(|(&id, _)| id));
    let mut seen = HashSet::new();
    for id in bodies {
        let Some(b) = phys.world.ref_bodies(id).first().copied() else { continue };
        seen.insert(id);
        let vel = if b.sleeping { [0.0; 3] } else { b.linvel };
        let Some(t) = ws.tracked.get_mut(&id) else {
            ws.tracked.insert(id, Tracked { vel, last_knock: f64::NEG_INFINITY, since: now });
            continue;
        };
        let before = std::mem::replace(&mut t.vel, vel);
        if !stepped {
            continue;
        }
        let speed = knock_speed(before, vel);
        if speed < KNOCK_LIGHT || now - t.last_knock < KNOCK_GAP || now - t.since < KNOCK_SETTLE {
            continue;
        }
        t.last_knock = now;
        let hard = speed >= KNOCK_HARD;
        let code = phys.world.ref_material(id);
        let sound = code.and_then(|c| phy_sound(c, hard)).and_then(|e| phy.get(e).copied());
        ws.stats.knocks += 1;
        ws.stats.knocks_hard += hard as u32;
        match sound {
            Some(sound) => {
                out.write(PlaySound { sound, at: Some(b.translation) });
            }
            None => ws.stats.knocks_unsounded += 1,
        }
        debug!(
            "knock: {id:08X} at {speed:.0} units/s ({}), {:.1} s after its collision came, material {}, sound {}",
            if hard { "hard" } else { "light" },
            now - t.since,
            code.map_or("none".to_string(), |c| format!("{c} {}", havok_name(c))),
            code.and_then(|c| phy_sound(c, hard)).unwrap_or("none")
        );
    }
    ws.tracked.retain(|id, _| seen.contains(id));
}

/// Pick-ups, drops, containers, activators and the sounds scripts ask for.
#[allow(clippy::too_many_arguments)]
fn objects(
    mut ws: ResMut<WorldSounds>,
    mut changed: MessageReader<RefChanged>,
    mut activated: MessageReader<Activated>,
    mut impacts: MessageReader<ImpactPicked>,
    menu: Option<Res<ActiveMenu>>,
    state: Res<State<PlayState>>,
    sim: Res<SimState>,
    script: Option<ResMut<crate::script::ScriptState>>,
    player: Option<Res<Player>>,
    sfx: Option<Res<Sfx>>,
    panel: Option<Res<crate::message_menu::MessagePanel>>,
    mut ui: MessageReader<crate::ui_sounds::UiSound>,
    mut out: MessageWriter<PlaySound>,
) {
    let ws = &mut *ws;
    let sfx = sfx.as_deref();
    // Take All plays one sound of its own instead of every item's (`crate::ui_sounds`).
    let take_all = ui.read().any(|u| u.event == crate::ui_sounds::UiEvent::TakeAll);
    for i in impacts.read() {
        *ws.stats.impacts.entry(i.material).or_default() += 1;
        ws.stats.impacts_no_material += i.no_material as u32;
    }
    for &RefChanged { form_id: id } in changed.read() {
        let Some(s) = sim.world.peek(FormId(id)) else { continue };
        if !ws.forms.items.contains_key(&s.base) {
            continue;
        }
        let (base, pos) = (s.base, s.pos);
        if s.deleted {
            if ws.picked.insert(id) {
                ws.stats.pickups += 1;
                let sound = item_sound(ws, sfx, base, true);
                info!("world sounds: {id:08X} ({base}) picked up, sound {}", sound_name(sfx, sound));
                if let Some(sound) = sound {
                    out.write(PlaySound { sound, at: Some(pos) });
                }
            }
        } else if id >= 0xFF00_0000 && ws.dropped.insert(id) {
            ws.stats.drops += 1;
            let sound = item_sound(ws, sfx, base, false);
            info!("world sounds: {id:08X} ({base}) dropped, sound {}", sound_name(sfx, sound));
            if let Some(sound) = sound {
                out.write(PlaySound { sound, at: Some(pos) });
            }
        }
    }

    // Items moved between the player and an open container: what the player gains is
    // taken (its pick-up sound), what the player loses is put in (its drop sound), heard
    // at the listener [hypothesis: the same sounds as in the world].
    let in_container = menu.as_ref().and_then(|m| m.0.as_ref()).is_some_and(|m| matches!(m.mode, Mode::Container(_)));
    let now_carried: Option<BTreeMap<FormId, i32>> =
        in_container.then(|| sim.world.peek(FormId::PLAYER_REF).and_then(|p| p.inventory.as_ref()).map(|i| i.iter().collect()).unwrap_or_default());
    if let (Some(before), Some(now)) = (ws.carried.as_ref(), now_carried.as_ref()) {
        let mut moved: Vec<(FormId, bool)> = Vec::new();
        for (&item, &n) in now {
            if n > before.get(&item).copied().unwrap_or(0) {
                moved.push((item, true));
            }
        }
        for (&item, &n) in before {
            if n > now.get(&item).copied().unwrap_or(0) {
                moved.push((item, false));
            }
        }
        for (item, taken) in moved {
            let sound = item_sound(ws, sfx, item, taken);
            if taken {
                ws.stats.taken += 1;
            } else {
                ws.stats.put += 1;
            }
            let quiet = take_all && taken;
            ws.stats.taken_quietly += quiet as u32;
            info!(
                "world sounds: {item} {} the container, sound {}{}",
                if taken { "taken from" } else { "put in" },
                sound_name(sfx, sound),
                if quiet { " (held back: Take All)" } else { "" }
            );
            if let Some(sound) = sound.filter(|_| !quiet) {
                out.write(PlaySound { sound, at: None });
            }
        }
    }
    ws.carried = now_carried;

    // The message box: a sound when one opens [design: `UIPopUpMessageWindow`, by its
    // name; neither MESG nor message_menu.xml names one], the buttons' click when one is
    // picked (the box goes) and their mouse-over sound when the frame moves [observed:
    // `UIMenuOK` and `UIMenuFocus` in menus\prefabs\list_box_template.xml, which the
    // message box's buttons include].
    let shown = panel.as_ref().and_then(|p| p.view.as_ref().map(|v| (format!("{:?}|{}|{:?}", v.title, v.text, v.buttons), p.selected)));
    if shown != ws.message {
        let was = ws.message.clone();
        let (was, now) = (was.as_ref(), shown.as_ref());
        let mut ui = |edid: &str, what: &str, ws: &mut WorldSounds| {
            let map = ws.by_edid.get_or_insert_with(|| sfx.map(by_edid).unwrap_or_default());
            let sound = map.get(edid).copied();
            info!("world sounds: message box {what}, sound {edid}{}", if sound.is_some() { "" } else { " (not found)" });
            if let Some(sound) = sound {
                out.write(PlaySound { sound, at: None });
            }
        };
        let (closed, opened) = match (was, now) {
            (Some(a), Some(b)) => (a.0 != b.0, a.0 != b.0),
            (a, b) => (a.is_some(), b.is_some()),
        };
        if closed {
            ws.stats.boxes_closed += 1;
            ui("UIMenuOK", "button picked", ws);
        }
        if opened {
            ws.stats.boxes_opened += 1;
            ui("UIPopUpMessageWindow", "opened", ws);
        } else if let (Some(a), Some(b)) = (was, now) {
            if a.1 != b.1 {
                ui("UIMenuFocus", "choice moved", ws);
            }
        }
        ws.message = shown;
    }

    // The container menu.
    let open = menu.as_ref().and_then(|m| m.0.as_ref()).and_then(|m| match m.mode {
        Mode::Container(c) => Some(c.0),
        // A merchant is an actor, not a container: no container sound [design].
        Mode::Inventory | Mode::Barter(_) => None,
    });
    if open != ws.container {
        let sound_of = |c: u32, opening: bool| {
            let base = sim.base_of(c)?;
            let &(o, cl) = ws.forms.containers.get(&base)?;
            if opening {
                o
            } else {
                cl
            }
        };
        for (c, opening) in [(ws.container, false), (open, true)] {
            let Some(c) = c else { continue };
            let sound = sound_of(c, opening);
            if opening {
                ws.stats.container_opens += 1;
            } else {
                ws.stats.container_closes += 1;
            }
            info!("world sounds: container {c:08X} {}, sound {}", if opening { "opened" } else { "closed" }, sound_name(sfx, sound));
            if let Some(sound) = sound {
                out.write(PlaySound { sound, at: ref_pos(&sim, c) });
            }
        }
        ws.container = open;
    }

    for &Activated { reference } in activated.read() {
        // A load door: its open sound as the player goes through [hypothesis].
        let leads_away = sim.refs.get(FormId(reference)).is_some_and(|p| p.teleport.is_some());
        if let Some(&sound) = sim.base_of(reference).and_then(|b| ws.forms.door_open.get(&b)).filter(|_| leads_away) {
            ws.stats.load_doors += 1;
            info!("world sounds: load door {reference:08X} used, sound {}", sound_name(sfx, Some(sound)));
            out.write(PlaySound { sound, at: None });
            continue;
        }
        let Some(&sound) = sim.base_of(reference).and_then(|b| ws.forms.activators.get(&b)) else { continue };
        ws.stats.activations += 1;
        info!("world sounds: activator {reference:08X} used, sound {}", sound_name(sfx, Some(sound)));
        out.write(PlaySound { sound, at: ref_pos(&sim, reference) });
    }

    // Scripts.
    if let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) {
        ws.held.append(&mut rt.game.sound_requests);
    }
    let in_game = *state.get() == PlayState::Playing;
    let eye = player.as_deref().map(Player::eye);
    let mut held = Vec::new();
    for r in std::mem::take(&mut ws.held) {
        if !in_game && !r.system {
            held.push(r);
            continue;
        }
        let at = match r.at {
            None => None,
            Some(FormId::PLAYER_REF) => eye,
            Some(at) => ref_pos(&sim, at.0),
        };
        if r.at.is_some() {
            ws.stats.script_3d += 1;
        } else {
            ws.stats.script_2d += 1;
        }
        info!("world sounds: a script plays {}{}", r.sound, r.at.map_or(String::new(), |a| format!(" at {a}")));
        out.write(PlaySound { sound: r.sound, at });
    }
    if !held.is_empty() && held.len() != ws.stats.script_held as usize {
        ws.stats.script_held = ws.stats.script_held.max(held.len() as u32);
    }
    ws.held = held;
}

/// Placed objects' own sounds near the listener, sent to the ambient every frame (see
/// the module notes).
fn object_sources(mut ws: ResMut<WorldSounds>, sim: Res<SimState>, loc: Res<crate::Location>, player: Option<Res<Player>>, sfx: Option<Res<Sfx>>, mut out: MessageWriter<SoundSource>) {
    let (Some(player), Some(sfx)) = (player, sfx) else { return };
    let ws = &mut *ws;
    if ws.sources.is_none() {
        let mut by: HashMap<crate::music::Space, Vec<(u32, FormId)>> = HashMap::new();
        for p in sim.refs.refs.values() {
            let (Some(&sound), Some(cell)) = (ws.forms.object_sounds.get(&p.base), p.cell) else { continue };
            let space = sim.refs.cell_world.get(&cell).map_or(crate::music::Space::Interior(cell.0), |w| crate::music::Space::World(w.0));
            by.entry(space).or_default().push((p.id.0, sound));
        }
        ws.stats.object_sources = by.values().map(Vec::len).sum();
        info!("world sounds: {} placed activators, containers and doors with a sound of their own", ws.stats.object_sources);
        ws.sources = Some(by);
    }
    let eye = player.eye();
    let space = crate::music::Space::of(&loc);
    let Some(list) = ws.sources.as_ref().and_then(|s| s.get(&space)) else { return };
    for &(id, sound) in list {
        let Some(reach) = sfx.forms.sounds.get(&sound).map(|s| s.max_distance * 1.1) else { continue };
        let Some(at) = ref_pos(&sim, id) else { continue };
        let d2 = (at[0] - eye[0]).powi(2) + (at[1] - eye[1]).powi(2) + (at[2] - eye[2]).powi(2);
        if d2 > reach * reach || !sim.world.is_enabled(FormId(id)).unwrap_or(false) {
            continue;
        }
        ws.stats.object_sources_sent.insert(id);
        out.write(SoundSource { key: SoundSource::key(1, id), sound, at });
    }
}

fn report(mut exit: MessageReader<AppExit>, ws: Option<Res<WorldSounds>>) {
    let (Some(_), Some(ws)) = (exit.read().last(), ws) else { return };
    let s = &ws.stats;
    let impacts: Vec<String> = s.impacts.iter().map(|(m, n)| format!("{m} {n}")).collect();
    info!(
        "world sounds summary: impacts by material: {} ({} on collision without a material); {} knocks ({} hard, {} of a material without a sound); {} pick-ups, {} drops; {} items taken from and {} put in containers; containers opened {} and closed {} times; {} activator sounds; {} message boxes opened, {} closed; scripts played {} sounds and {} placed; {} load doors; {} objects' own sounds in reach",
        if impacts.is_empty() { "none".to_string() } else { impacts.join(", ") },
        s.impacts_no_material,
        s.knocks,
        s.knocks_hard,
        s.knocks_unsounded,
        s.pickups,
        s.drops,
        s.taken,
        s.put,
        s.container_opens,
        s.container_closes,
        s.activations,
        s.boxes_opened,
        s.boxes_closed,
        s.script_2d,
        s.script_3d,
        s.load_doors,
        s.object_sources_sent.len()
    );
}

pub struct WorldSoundsPlugin;

impl Plugin for WorldSoundsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init)
            .add_systems(Update, (knocks, objects, object_sources).in_set(PlaySet::Sync))
            .add_systems(Last, report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn havok_materials_to_knock_sounds() {
        // A bottle (24) and glass (3) clink like bottles; platform and stairs codes are
        // their material's.
        assert_eq!(phy_sound(24, true), Some("PHYBottleH"));
        assert_eq!(phy_sound(3 + 32, false), Some("PHYBottleL"));
        assert_eq!(havok_name(24 + 64), "BOTTLE");
        assert_eq!(phy_sound(9, true), Some("CWoodMedium"));
        // Dirt, grass, water and sand make no knock.
        for c in [2, 4, 8, 18] {
            assert_eq!(phy_sound(c, true), None);
        }
    }

    #[test]
    fn knocks_are_speed_lost() {
        // Falling faster, or pushed from rest: no knock; at rest: none.
        assert_eq!(knock_speed([10.0, 0.0, -100.0], [10.0, 0.0, -111.0]), 0.0);
        assert_eq!(knock_speed([0.0; 3], [0.0, 210.0, 0.0]), 0.0);
        assert_eq!(knock_speed([0.0; 3], [0.0; 3]), 0.0);
        // Landing from 300 units/s: a hard knock; bouncing straight back: twice the speed.
        assert!((knock_speed([0.0, 0.0, -300.0], [0.0; 3]) - 300.0).abs() < 1e-3);
        assert!((knock_speed([0.0, 0.0, -100.0], [0.0, 0.0, 100.0]) - 200.0).abs() < 1e-3);
    }
}
