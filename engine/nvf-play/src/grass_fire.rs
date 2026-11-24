//! Grass fires (docs/design/grass-fire.md) [design: owner request 2026-11-14, #2567; the
//! original game has no spreading fire]: a shot or a blast whose object effect burns sets
//! the grass where it lands alight, the fire spreads across the grass, sets the actors
//! standing in it on fire, and burns out within a hard limit.
//!
//! - **Patches.** The ground is split into squares of [`PATCH`] game units. A patch has
//!   fuel when grass grows on it: the renderer, which places the grass, says so per cell
//!   ([`Fires::add_fuel`]) and forgets a cell when it unloads ([`Fires::forget_cell`]).
//!   What burnt there is kept ([`Fires::kept`]) and burnt again when the cell loads, so
//!   burnt grass stays burnt for the session [design: owner 2026-11-15 via parity, co-op:
//!   a player who comes later finds the grass another burnt]. Not kept in saves.
//! - **Spread.** A burning patch lights each patch within [`FireRules::jump`] patches of
//!   it that has fuel and has not burnt (so a fire crosses the bare ground between
//!   tufts), after a random delay ([`FireRules::spread_secs`] per patch of distance),
//!   shorter downwind and longer upwind ([`FireRules::wind_bias`]). A patch burns for
//!   about [`FireRules::burn_secs`] and is then burnt out.
//! - **Limits.** A fire lights no patch further than [`FireRules::reach`] from where it
//!   started and no more than [`FireRules::max_patches`]; no more than
//!   [`FireRules::max_fires`] burn at once, nor more than [`FireRules::max_burning`]
//!   patches over all of them. A landing on grass a live fire may still reach adds to
//!   that fire instead of starting another.
//! - **Actors.** An actor whose middle is over or beside a burning patch (within
//!   [`FireRules::catch_reach`] of its middle and [`FireRules::catch_height`] of its
//!   ground) catches the fire's item, the burning
//!   object effect that started it, as from a hit by who started it
//!   (`crate::combat::hit_effects`), again every [`CATCH_EVERY`] seconds while it stays.
//! - **Burn map.** One byte a patch in a [`MAP`] × [`MAP`] map that wraps around the
//!   world (`nvforever` grass.wesl reads it): 0 untouched; up to 0.5 while it burns (the
//!   grass blackens); 0.5 to 1 once burnt (it sinks away); 1 burnt away.
//!
//! Log: `grass fire <n>: started …`, `grass fire <n>: out: …`, `grass fire: <actor>
//! caught fire from fire <n>` the first time each actor does in a fire, and at exit
//! `grass fires summary: …`.

use crate::combat::{Combat, CombatStats};
use crate::phys::PhysState;
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use bevy::prelude::*;
use fnv_combat::rng::SplitMix;
use fnv_combat::vec::V3;
use fnv_sim::FormId;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

/// The side of a patch, game units (about 0.9 m) [design].
pub const PATCH: f32 = 64.0;
/// An exterior cell's side (game units): a patch's cell for a guest's remote patches.
const CELL: f32 = 4096.0;

/// The exterior cell (grid square) a patch is in.
pub fn cell_of(k: Key) -> (i32, i32) {
    let per = (CELL / PATCH) as i32;
    (k.0.div_euclid(per), k.1.div_euclid(per))
}
/// [`LitPatch::fire`] of a patch that already burnt out (fire ids start at 1).
pub const BURNT_OUT: u32 = 0;
/// Patches along each side of the burn map: 32,768 units (eight cells) before it wraps,
/// more than the loaded cells span [design].
pub const MAP: usize = 512;
/// Seconds between the checks for actors standing in the fire [design].
pub const CATCH_EVERY: f32 = 0.5;

/// A patch: its column and row (`floor(x / PATCH)`, `floor(y / PATCH)`).
pub type Key = (i32, i32);

/// The patch a point (game space) is on.
pub fn key(x: f32, y: f32) -> Key {
    ((x / PATCH).floor() as i32, (y / PATCH).floor() as i32)
}

/// The middle of a patch (game space).
pub fn centre(k: Key) -> [f32; 2] {
    [(k.0 as f32 + 0.5) * PATCH, (k.1 as f32 + 0.5) * PATCH]
}

/// A patch's byte in the burn map.
pub fn texel(k: Key) -> usize {
    k.0.rem_euclid(MAP as i32) as usize + MAP * k.1.rem_euclid(MAP as i32) as usize
}

/// How fires behave [design: chosen to look like a grass fire in an open-world shooter,
/// a front that sweeps outward in a few seconds and leaves black ground; tuned by eye].
#[derive(Clone, Copy, Debug)]
pub struct FireRules {
    /// Seconds a patch burns (each patch ±20%).
    pub burn_secs: f32,
    /// Seconds over which a burning patch's grass turns black.
    pub char_secs: f32,
    /// Seconds over which burnt grass sinks away.
    pub fade_secs: f32,
    /// The delay before a burning patch lights a side neighbour, seconds (a random value
    /// between the two; a diagonal one × √2).
    pub spread_secs: (f32, f32),
    /// How many patches away a burning patch reaches (1: the eight around it; 2: across a
    /// patch without grass too, as desert grass grows in tufts with bare ground between).
    pub jump: i32,
    /// The furthest a fire reaches from where it started, game units.
    pub reach: f32,
    /// The most patches one fire lights.
    pub max_patches: u32,
    /// The most fires burning at once.
    pub max_fires: usize,
    /// The most patches burning at once over all fires.
    pub max_burning: usize,
    /// How much the wind speeds the spread downwind and slows it upwind at full wind
    /// (0: not at all; 0.6: 1.6 times as fast downwind, 0.4 times upwind).
    pub wind_bias: f32,
    /// An actor's middle within this height of a burning patch's ground, and within
    /// `catch_reach` of the patch's middle horizontally, catches fire.
    pub catch_height: f32,
    pub catch_reach: f32,
}

impl Default for FireRules {
    fn default() -> Self {
        FireRules {
            burn_secs: 6.0,
            char_secs: 2.0,
            fade_secs: 5.0,
            spread_secs: (0.5, 1.5),
            jump: 2,
            reach: 1024.0,
            max_patches: 500,
            max_fires: 8,
            max_burning: 1500,
            wind_bias: 0.6,
            catch_height: 160.0,
            catch_reach: 64.0,
        }
    }
}

/// A patch's state once fire reached it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Burn {
    Burning { fire: u32, since: f32, until: f32 },
    Burnt { since: f32 },
    Ash,
}

/// A patch with grass: its ground's height and the cell it belongs to.
#[derive(Clone, Copy, Debug)]
struct Fuel {
    z: f32,
    cell: (i32, i32),
}

/// One fire.
#[derive(Clone, Debug)]
pub struct Fire {
    pub id: u32,
    /// Where it started (game space).
    pub origin: V3,
    /// The burning object effect that started it, which the actors in it catch.
    pub item: FormId,
    /// Who started it.
    pub by: Option<FormId>,
    /// Patches lit, burning now, waiting to be lit.
    pub patches: u32,
    pub burning: u32,
    pending: u32,
    /// The furthest a lit patch's middle is from the origin.
    pub reached: f32,
    pub started: f32,
    /// Spread refused by the limits (reach, patches, burning over all fires).
    pub held: u32,
    /// Actors that caught fire from it.
    pub caught: HashSet<FormId>,
}

/// A neighbour due to be lit.
#[derive(Clone, Copy, Debug)]
struct Pending {
    at: f32,
    key: Key,
    fire: u32,
}

impl PartialEq for Pending {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for Pending {}
impl PartialOrd for Pending {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Pending {
    /// Earliest first out of the (max-)heap; ties by patch and fire, so runs repeat.
    fn cmp(&self, o: &Self) -> Ordering {
        o.at.total_cmp(&self.at).then_with(|| o.key.cmp(&self.key)).then_with(|| o.fire.cmp(&self.fire))
    }
}

/// Why a landing started no fire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// No grass on the patch or around it.
    NoGrass,
    /// The grass there burns or has burnt.
    Burnt,
    /// [`FireRules::max_fires`] fires burn already.
    TooMany,
}

/// What happened, for the renderer and the log.
#[derive(Clone, Debug)]
pub enum FireEvent {
    Started { fire: u32 },
    /// A landing added to a live fire.
    Fed { fire: u32 },
    /// A patch caught: its key and ground height.
    Lit { key: Key, z: f32 },
    /// A patch burnt out, or was forgotten while it burnt.
    Out { key: Key },
    /// A fire went out: nothing burns and nothing waits.
    Ended { fire: Fire },
}

/// A patch the host's fire lit, for the co-op guests ([`Fires::record`]) and lit on a
/// guest as the host's ([`Fires::light_remote`]): the host decides where a fire starts
/// and how it spreads, and every player sees the same patches burn [design: owner
/// 2026-11-14, "i want that in here too", co-op].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LitPatch {
    pub key: Key,
    /// Its ground's height on the host.
    pub z: f32,
    /// The fire it belongs to; [`BURNT_OUT`] for a patch that already burnt out (a late
    /// joiner's [`Fires::snapshot`]: ash, scorched, no flames).
    pub fire: u32,
    pub item: FormId,
    /// Who started the fire: an NPC's reference; `None` for a player (the host's own
    /// reference means another player on a guest).
    pub by: Option<FormId>,
    /// How long it burns.
    pub secs: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FireStats {
    pub fires: u32,
    pub fed: u32,
    pub patches: u32,
    pub actors_caught: u32,
    pub no_grass: u32,
    pub burnt: u32,
    pub too_many: u32,
    pub held: u32,
    pub forgotten: u32,
    /// Burnt patches burnt again when their cell loaded again.
    pub restored: u32,
    /// The most patches burning at once.
    pub peak_burning: usize,
}

/// The fires and the grass they burn (module notes).
pub struct Fires {
    pub rules: FireRules,
    /// Seconds since the start.
    pub now: f32,
    /// The wind on the ground: its direction (game space, horizontal) times its strength
    /// (0 to 1).
    pub wind: [f32; 2],
    fuel: HashMap<Key, Fuel>,
    by_cell: HashMap<(i32, i32), Vec<Key>>,
    burns: HashMap<Key, Burn>,
    /// Patches fire reached in cells not loaded now, with their ground's height: burnt
    /// out, kept for when the cell loads again ([`Fires::add_fuel`]) and for a guest who
    /// joins ([`Fires::snapshot`]).
    kept: HashMap<Key, f32>,
    /// The patches burning or sinking: their byte in the map still changes.
    active: HashSet<Key>,
    fires: Vec<Fire>,
    queue: BinaryHeap<Pending>,
    next_id: u32,
    burning: usize,
    /// The burn map (module notes), and whether it changed since the renderer took it.
    pub map: Vec<u8>,
    pub dirty: bool,
    pub events: Vec<FireEvent>,
    pub stats: FireStats,
    /// The host's in co-op: every patch lit is also kept in [`Fires::lit`] for the guests.
    pub record: bool,
    pub lit: Vec<LitPatch>,
}

impl Default for Fires {
    fn default() -> Self {
        Fires::new(FireRules::default())
    }
}

impl Fires {
    pub fn new(rules: FireRules) -> Fires {
        Fires {
            rules,
            now: 0.0,
            wind: [0.0, 0.0],
            fuel: HashMap::new(),
            by_cell: HashMap::new(),
            burns: HashMap::new(),
            kept: HashMap::new(),
            active: HashSet::new(),
            fires: Vec::new(),
            queue: BinaryHeap::new(),
            next_id: 1,
            burning: 0,
            map: vec![0; MAP * MAP],
            dirty: false,
            events: Vec::new(),
            stats: FireStats::default(),
            record: false,
            lit: Vec::new(),
        }
    }

    /// The patches with grass in `cell` (grid square), with their ground's height. What
    /// burnt in the cell while it was away is burnt out here again (ash, scorched ground),
    /// including patches with no grass here (a guest's grass may be thinner than the
    /// host's): returns how many.
    pub fn add_fuel(&mut self, cell: (i32, i32), patches: impl IntoIterator<Item = (Key, f32)>) -> usize {
        let list = self.by_cell.entry(cell).or_default();
        for (k, z) in patches {
            if let std::collections::hash_map::Entry::Vacant(e) = self.fuel.entry(k) {
                e.insert(Fuel { z, cell });
                list.push(k);
            }
        }
        let back: Vec<(Key, f32)> = self.kept.iter().filter(|(k, _)| cell_of(**k) == cell).map(|(k, z)| (*k, *z)).collect();
        for &(k, z) in &back {
            self.kept.remove(&k);
            self.remember_ground(k, z);
            if let std::collections::hash_map::Entry::Vacant(e) = self.burns.entry(k) {
                e.insert(Burn::Ash);
                self.set_texel(k, 255);
            }
        }
        self.stats.restored += back.len() as u32;
        back.len()
    }

    /// Patches burnt out in cells not loaded now (module notes).
    pub fn kept(&self) -> usize {
        self.kept.len()
    }

    /// The cells whose grass is known.
    pub fn cells(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.by_cell.keys().copied()
    }

    /// Whether a patch has grass.
    pub fn has_fuel(&self, k: Key) -> bool {
        self.fuel.contains_key(&k)
    }

    /// The patch with grass nearest a point within `within` units, and its distance.
    pub fn nearest_fuel(&self, p: V3, within: f32) -> Option<(Key, f32)> {
        self.fuel.keys().map(|&k| (k, dist2(p, centre(k)))).filter(|(_, d)| *d <= within).min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// The state of a patch fire reached.
    pub fn burn(&self, k: Key) -> Option<Burn> {
        self.burns.get(&k).copied()
    }

    pub fn fires(&self) -> &[Fire] {
        &self.fires
    }

    /// Patches burning now.
    pub fn burning(&self) -> usize {
        self.burning
    }

    /// Forget a cell (it unloaded): its grass, and its fires. What fire reached there is
    /// kept as burnt out (one still burning too: it would have burnt out by the time
    /// anyone comes back), for the next time the cell loads. Returns the patches kept.
    pub fn forget_cell(&mut self, cell: (i32, i32)) -> usize {
        let Some(keys) = self.by_cell.remove(&cell) else { return 0 };
        let mut n = 0;
        for k in keys {
            let z = self.fuel.get(&k).map(|f| f.z);
            if self.fuel.get(&k).is_some_and(|f| f.cell == cell) {
                self.fuel.remove(&k);
            }
            let Some(b) = self.burns.remove(&k) else { continue };
            n += 1;
            self.kept.insert(k, z.unwrap_or(0.0));
            self.active.remove(&k);
            if let Burn::Burning { fire, .. } = b {
                self.burning -= 1;
                if let Some(f) = self.fires.iter_mut().find(|f| f.id == fire) {
                    f.burning -= 1;
                }
                self.events.push(FireEvent::Out { key: k });
            }
            self.set_texel(k, 0);
        }
        self.stats.forgotten += n as u32;
        n
    }

    /// A shot or a blast whose effect burns landed at `point`: set the grass there alight
    /// (or on a patch next to it, when it landed just off the grass), adding to a live
    /// fire that may still reach it. Returns the fire.
    pub fn ignite(&mut self, point: V3, item: FormId, by: Option<FormId>) -> Result<u32, Refused> {
        let at = key(point[0], point[1]);
        let k = if self.has_fuel(at) {
            at
        } else {
            // The nearest neighbour with grass (side ones first).
            let mut around: Vec<Key> = (-1..=1).flat_map(|dx| (-1..=1).map(move |dy| (at.0 + dx, at.1 + dy))).filter(|k| *k != at && self.has_fuel(*k)).collect();
            around.sort_by_key(|k| ((k.0 - at.0).abs() + (k.1 - at.1).abs(), *k));
            match around.first() {
                Some(&k) => k,
                None => {
                    self.stats.no_grass += 1;
                    return Err(Refused::NoGrass);
                }
            }
        };
        if self.burns.contains_key(&k) {
            self.stats.burnt += 1;
            return Err(Refused::Burnt);
        }
        let c = centre(k);
        let reach = self.rules.reach;
        let live = self.fires.iter().position(|f| f.patches < self.rules.max_patches && dist2(f.origin, c) <= reach);
        let i = match live {
            Some(i) => {
                self.stats.fed += 1;
                self.events.push(FireEvent::Fed { fire: self.fires[i].id });
                i
            }
            None if self.fires.len() >= self.rules.max_fires => {
                self.stats.too_many += 1;
                return Err(Refused::TooMany);
            }
            None => {
                let id = self.next_id;
                self.next_id += 1;
                let z = self.fuel[&k].z;
                self.fires.push(Fire { id, origin: [c[0], c[1], z], item, by, patches: 0, burning: 0, pending: 0, reached: 0.0, started: self.now, held: 0, caught: HashSet::new() });
                self.stats.fires += 1;
                self.events.push(FireEvent::Started { fire: id });
                self.fires.len() - 1
            }
        };
        let id = self.fires[i].id;
        if self.burning >= self.rules.max_burning {
            self.fires[i].held += 1;
            self.stats.held += 1;
            return Ok(id);
        }
        self.light(k, i);
        Ok(id)
    }

    /// Light patch `k` for fire `i` and plan its neighbours.
    fn light(&mut self, k: Key, i: usize) {
        let rules = self.rules;
        let id = self.fires[i].id;
        let z = self.fuel.get(&k).map_or(0.0, |f| f.z);
        let until = self.now + rules.burn_secs * (0.8 + 0.4 * roll(k, id, 1));
        self.burns.insert(k, Burn::Burning { fire: id, since: self.now, until });
        self.active.insert(k);
        self.burning += 1;
        self.stats.patches += 1;
        self.stats.peak_burning = self.stats.peak_burning.max(self.burning);
        let f = &mut self.fires[i];
        f.patches += 1;
        f.burning += 1;
        f.reached = f.reached.max(dist2(f.origin, centre(k)));
        let origin = f.origin;
        self.set_texel(k, 2);
        self.events.push(FireEvent::Lit { key: k, z });
        if self.record {
            let f = &self.fires[i];
            self.lit.push(LitPatch { key: k, z, fire: id, item: f.item, by: f.by, secs: until - self.now });
        }
        let wind_len = (self.wind[0] * self.wind[0] + self.wind[1] * self.wind[1]).sqrt();
        let jump = rules.jump;
        for dx in -jump..=jump {
            for dy in -jump..=jump {
                let n = (k.0 + dx, k.1 + dy);
                if n == k || !self.fuel.contains_key(&n) || self.burns.contains_key(&n) {
                    continue;
                }
                if dist2(origin, centre(n)) > rules.reach {
                    self.fires[i].held += 1;
                    self.stats.held += 1;
                    continue;
                }
                // Patches away (1, √2, 2, √5, √8).
                let step = ((dx * dx + dy * dy) as f32).sqrt();
                let (a, b) = rules.spread_secs;
                let mut delay = (a + (b - a) * roll(n, id, 2)) * step;
                if wind_len > 1e-4 {
                    let d = [dx as f32 / step, dy as f32 / step];
                    let along = (d[0] * self.wind[0] + d[1] * self.wind[1]).clamp(-1.0, 1.0);
                    delay /= (1.0 + rules.wind_bias * along).max(0.1);
                }
                self.queue.push(Pending { at: self.now + delay, key: n, fire: id });
                self.fires[i].pending += 1;
            }
        }
    }

    /// A co-op guest's: light a patch as the host's fire did. No spread is planned here
    /// (the host sends each patch it lights) and no grass need grow here (a guest's grass
    /// may be thinner). Returns whether this started the fire here; `None` when the patch
    /// burns already.
    pub fn light_remote(&mut self, p: LitPatch) -> Option<bool> {
        let k = p.key;
        if self.burns.contains_key(&k) || self.kept.contains_key(&k) {
            return None;
        }
        if p.fire == BURNT_OUT {
            // Burnt out on the host before this guest came: ash, its grass gone and its
            // ground scorched, with no fire and no flames.
            self.remember_ground(k, p.z);
            self.burns.insert(k, Burn::Ash);
            self.set_texel(k, 255);
            return Some(false);
        }
        let i = match self.fires.iter().position(|f| f.id == p.fire) {
            Some(i) => (i, false),
            None => {
                let c = centre(k);
                self.fires.push(Fire { id: p.fire, origin: [c[0], c[1], p.z], item: p.item, by: p.by, patches: 0, burning: 0, pending: 0, reached: 0.0, started: self.now, held: 0, caught: HashSet::new() });
                self.stats.fires += 1;
                self.events.push(FireEvent::Started { fire: p.fire });
                (self.fires.len() - 1, true)
            }
        };
        let (i, new) = i;
        self.remember_ground(k, p.z);
        self.burns.insert(k, Burn::Burning { fire: p.fire, since: self.now, until: self.now + p.secs.max(0.0) });
        self.active.insert(k);
        self.burning += 1;
        self.stats.patches += 1;
        self.stats.peak_burning = self.stats.peak_burning.max(self.burning);
        let f = &mut self.fires[i];
        f.patches += 1;
        f.burning += 1;
        f.reached = f.reached.max(dist2(f.origin, centre(k)));
        self.set_texel(k, 2);
        self.events.push(FireEvent::Lit { key: k, z: p.z });
        Some(new)
    }

    /// The ground of a patch the host sent, for `burning_at`, kept with its cell so it goes
    /// when the cell does.
    fn remember_ground(&mut self, k: Key, z: f32) {
        if let std::collections::hash_map::Entry::Vacant(v) = self.fuel.entry(k) {
            let cell = cell_of(k);
            v.insert(Fuel { z, cell });
            self.by_cell.entry(cell).or_default().push(k);
        }
    }

    /// The host's, for a guest that joins (or whose stream starts again) while fires
    /// burn: every patch burning now, with the seconds it has left, and every patch burnt
    /// out and still remembered, as [`BURNT_OUT`] (owner 2026-11-14 via parity: a late
    /// joiner sees the fires). Nearest the fires' patches first is not needed: the guest
    /// takes them all at once ([`Fires::light_remote`]).
    pub fn snapshot(&self) -> Vec<LitPatch> {
        let mut out = Vec::new();
        for (&k, b) in &self.burns {
            let Some(z) = self.fuel.get(&k).map(|f| f.z) else { continue };
            match *b {
                Burn::Burning { fire, until, .. } => {
                    let Some(f) = self.fires.iter().find(|f| f.id == fire) else { continue };
                    out.push(LitPatch { key: k, z, fire, item: f.item, by: f.by, secs: (until - self.now).max(0.0) });
                }
                Burn::Burnt { .. } | Burn::Ash => out.push(LitPatch { key: k, z, fire: BURNT_OUT, item: FormId(0), by: None, secs: 0.0 }),
            }
        }
        // Burnt where no one is now: a guest keeps them for when it gets there.
        out.extend(self.kept.iter().map(|(&k, &z)| LitPatch { key: k, z, fire: BURNT_OUT, item: FormId(0), by: None, secs: 0.0 }));
        out.sort_by_key(|p| (p.fire, p.key));
        out
    }

    /// Advance the fires by `dt` seconds.
    pub fn step(&mut self, dt: f32) {
        self.now += dt.max(0.0);
        let now = self.now;
        // Neighbours due.
        while self.queue.peek().is_some_and(|p| p.at <= now) {
            let p = self.queue.pop().expect("peeked");
            let Some(i) = self.fires.iter().position(|f| f.id == p.fire) else { continue };
            self.fires[i].pending -= 1;
            if !self.fuel.contains_key(&p.key) || self.burns.contains_key(&p.key) {
                continue;
            }
            if self.fires[i].patches >= self.rules.max_patches || self.burning >= self.rules.max_burning {
                self.fires[i].held += 1;
                self.stats.held += 1;
                continue;
            }
            self.light(p.key, i);
        }
        // Burning, burnt, burnt away.
        let rules = self.rules;
        let active: Vec<Key> = self.active.iter().copied().collect();
        for k in active {
            let Some(b) = self.burns.get(&k).copied() else { continue };
            match b {
                Burn::Burning { fire, since, until } => {
                    if now >= until {
                        self.burns.insert(k, Burn::Burnt { since: now });
                        self.burning -= 1;
                        if let Some(f) = self.fires.iter_mut().find(|f| f.id == fire) {
                            f.burning -= 1;
                        }
                        self.events.push(FireEvent::Out { key: k });
                        self.set_texel(k, 128);
                    } else {
                        let charred = ((now - since) / rules.char_secs.max(1e-3)).clamp(0.0, 1.0);
                        self.set_texel(k, 2 + (charred * 125.0).round() as u8);
                    }
                }
                Burn::Burnt { since } => {
                    let gone = ((now - since) / rules.fade_secs.max(1e-3)).clamp(0.0, 1.0);
                    if gone >= 1.0 {
                        self.burns.insert(k, Burn::Ash);
                        self.active.remove(&k);
                        self.set_texel(k, 255);
                    } else {
                        self.set_texel(k, 128 + (gone * 127.0).round() as u8);
                    }
                }
                Burn::Ash => {
                    self.active.remove(&k);
                }
            }
        }
        // Fires with nothing burning and nothing to come.
        let mut i = 0;
        while i < self.fires.len() {
            if self.fires[i].burning == 0 && self.fires[i].pending == 0 {
                let fire = self.fires.remove(i);
                self.events.push(FireEvent::Ended { fire });
            } else {
                i += 1;
            }
        }
    }

    /// The fire burning under or beside a point (an actor's middle, game space): the
    /// nearest burning patch whose middle is within [`FireRules::catch_reach`] of it and
    /// whose ground is within [`FireRules::catch_height`] (the patch it stands on may be
    /// bare ground between burning tufts).
    pub fn burning_at(&self, p: V3) -> Option<&Fire> {
        let at = key(p[0], p[1]);
        let mut best: Option<(f32, u32)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                let k = (at.0 + dx, at.1 + dy);
                let Some(Burn::Burning { fire, .. }) = self.burns.get(&k) else { continue };
                let Some(z) = self.fuel.get(&k).map(|f| f.z) else { continue };
                let d = dist2(p, centre(k));
                if d <= self.rules.catch_reach && (p[2] - z).abs() <= self.rules.catch_height && best.is_none_or(|b| d < b.0) {
                    best = Some((d, *fire));
                }
            }
        }
        let fire = best?.1;
        self.fires.iter().find(|f| f.id == fire)
    }

    fn set_texel(&mut self, k: Key, v: u8) {
        let t = &mut self.map[texel(k)];
        if *t != v {
            *t = v;
            self.dirty = true;
        }
    }

    /// The summary line's figures.
    pub fn summary(&self) -> String {
        let s = &self.stats;
        format!(
            "grass fires summary: {} started, {} landings fed a live fire, {} patches burnt (at most {} at once), {} actors caught fire, refused: {} no grass, {} already burnt, {} too many fires; {} spreads held by the limits, {} patches forgotten with their cells, {} still burning; {} burnt patches kept away from their cells, {} burnt again when a cell came back",
            s.fires, s.fed, s.patches, s.peak_burning, s.actors_caught, s.no_grass, s.burnt, s.too_many, s.held, s.forgotten, self.fires.len(), self.kept.len(), s.restored
        )
    }
}

/// Horizontal distance from `a` to `b`.
fn dist2(a: V3, b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// A random value in [0, 1) fixed by the patch, the fire and what it is for: the same
/// fire spreads the same way every run.
fn roll(k: Key, fire: u32, what: u64) -> f32 {
    let seed = ((k.0 as u32 as u64) << 32 | k.1 as u32 as u64) ^ (u64::from(fire) << 17) ^ what.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    SplitMix(seed).next_f32()
}

/// A shot or blast with an object effect that landed on dry ground (`crate::combat`).
#[derive(Clone, Copy, Debug)]
pub struct Landed {
    pub point: V3,
    pub item: FormId,
    pub by: Option<FormId>,
}

/// The grass fires and what the renderer needs of them (module notes).
#[derive(Resource, Default)]
pub struct GrassFires {
    pub fires: Fires,
    /// Whether each object effect burns (one of its effects is resisted by Fire
    /// Resistance).
    burns: HashMap<FormId, bool>,
    catch_clock: f32,
    /// A landing found no grass: said once, with the nearest grass.
    told_no_grass: bool,
    /// A co-op guest's (set by `nvforever`'s co-op): the host decides where fires start
    /// and spread; this game lights nothing itself and burns the patches the host sends
    /// ([`GrassFires::remote`]), and only its own player catches fire here (the host's
    /// fire burns its NPCs).
    pub guest: bool,
    /// The host's patches to light here this frame (a guest's).
    pub remote: Vec<LitPatch>,
    /// A guest's: its own burning shots and blasts that landed, for the host to light
    /// (or not) as its fire rules say.
    pub ask: Vec<Landed>,
    /// The host's: the guests' burning landings, lit here as the host's own are.
    pub asked: Vec<Landed>,
}

pub struct GrassFirePlugin;

impl Plugin for GrassFirePlugin {
    fn build(&self, app: &mut App) {
        crate::settings::add_setting(
            app,
            crate::settings::Setting {
                key: "grass-fire",
                group: crate::pause::GROUP_MENUS,
                label: "Grass fires",
                note: "On: flamers, incendiary rounds and fire blasts set grass alight; the fire spreads and burns whoever stands in it. Off: fire stays where it hits.",
                switches: "",
                get: |_| grass_fire_on(),
                set: |_, on| set_grass_fire(on),
                slider: None,
            },
        );
        // `tick` runs in `crate::combat`'s chain, after the projectiles fly.
        app.init_resource::<GrassFires>().add_systems(Last, report);
    }
}

/// The `grass-fire` switch; on by default [design: owner 2026-11-14].
static GRASS_FIRE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn grass_fire_on() -> bool {
    GRASS_FIRE.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_grass_fire(on: bool) {
    GRASS_FIRE.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Whether an object effect burns: one of its effects is resisted by Fire Resistance
/// (the flamer's and the incinerator's "Burning", hit-effects.md §1) [design].
fn item_burns(game: &fnv_game::game::Game, item: FormId) -> bool {
    game.applied_effects(item).iter().any(|e| game.data.magic_effects.get(&e.effect).is_some_and(|m| m.resistance == Some(fnv_game::combat_lines::FIRE_RESISTANCE)))
}

/// Start fires where burning shots and blasts landed, advance them, and set the actors
/// standing in them alight.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tick(
    time: Res<Time>,
    mut gf: ResMut<GrassFires>,
    mut c: ResMut<Combat>,
    mut sim: ResMut<SimState>,
    phys: Res<PhysState>,
    mut script: Option<ResMut<ScriptState>>,
    mut stats: ResMut<CombatStats>,
) {
    let gf = &mut *gf;
    // The renderer took last frame's patch events (`nvforever` grass.rs); none pile up
    // without one.
    gf.fires.events.clear();
    let landed = c.take_fire_landed();
    if gf.guest {
        // The host's fires only, whatever this game's own switch says.
        let mut ash = 0;
        for p in std::mem::take(&mut gf.remote) {
            match gf.fires.light_remote(p) {
                Some(true) => info!("grass fire {}: the host's fire, first patch here at ({:.0}, {:.0}, {:.0}) with {}", p.fire, centre(p.key)[0], centre(p.key)[1], p.z, item_name(script.as_deref(), p.item)),
                Some(false) if p.fire == BURNT_OUT => ash += 1,
                _ => {}
            }
        }
        if ash > 0 {
            info!("grass fire: {ash} patches the host's fires burnt out before this player came are burnt here too");
        }
    } else if !grass_fire_on() {
        gf.asked.clear();
        return;
    }
    let mut landed = landed;
    landed.append(&mut gf.asked);
    for l in landed {
        let burns = match gf.burns.get(&l.item) {
            Some(&b) => b,
            None => {
                let Some(rt) = script.as_deref().and_then(|s| s.rt.as_ref()) else { continue };
                let b = item_burns(&rt.game, l.item);
                gf.burns.insert(l.item, b);
                b
            }
        };
        if !burns {
            continue;
        }
        // A guest's own burning shots go to the host, which decides.
        if gf.guest {
            gf.ask.push(Landed { by: None, ..l });
            continue;
        }
        match gf.fires.ignite(l.point, l.item, l.by) {
            Ok(_) => {}
            Err(Refused::NoGrass) if !gf.told_no_grass => {
                gf.told_no_grass = true;
                let near = gf.fires.nearest_fuel(l.point, 4096.0).map_or("none within 4096 units".to_string(), |(k, d)| {
                    let c = centre(k);
                    format!("the nearest is {d:.0} units away at ({:.0}, {:.0})", c[0], c[1])
                });
                info!("grass fire: a burning landing at ({:.0}, {:.0}, {:.0}) found no grass to light; {near} (said once)", l.point[0], l.point[1], l.point[2]);
            }
            Err(why) => debug!("grass fire: a burning landing at ({:.0}, {:.0}, {:.0}) lit nothing: {why:?}", l.point[0], l.point[1], l.point[2]),
        }
    }
    gf.fires.step(time.delta_secs());
    for e in &gf.fires.events {
        match e {
            FireEvent::Started { fire } if !gf.guest => {
                if let Some(f) = gf.fires.fires().iter().find(|f| f.id == *fire) {
                    let by = f.by.map_or("no one".to_string(), |b| who(script.as_deref(), b));
                    info!("grass fire {fire}: started at ({:.0}, {:.0}, {:.0}) by {by} with {}", f.origin[0], f.origin[1], f.origin[2], item_name(script.as_deref(), f.item));
                }
            }
            // A guest's copy of the host's fire (the host logs the fire): none, as for its start.
            FireEvent::Ended { fire } if !gf.guest => info!(
                "grass fire {}: out after {:.1} s: {} patches burnt, reached {:.0} units of {:.0}, {} spreads held by the limits, {} actors caught fire",
                fire.id,
                gf.fires.now - fire.started,
                fire.patches,
                fire.reached,
                gf.fires.rules.reach,
                fire.held,
                fire.caught.len()
            ),
            _ => {}
        }
    }
    catch(gf, time.delta_secs(), &phys, script.as_deref_mut(), &mut sim, &mut stats);
}

/// Every [`CATCH_EVERY`] seconds: the actors over a burning patch catch its fire's item.
fn catch(gf: &mut GrassFires, dt: f32, phys: &PhysState, mut script: Option<&mut ScriptState>, sim: &mut SimState, stats: &mut CombatStats) {
    if gf.fires.fires().is_empty() {
        gf.catch_clock = 0.0;
        return;
    }
    gf.catch_clock += dt;
    if gf.catch_clock < CATCH_EVERY {
        return;
    }
    gf.catch_clock = 0.0;
    // A guest's NPCs are the host's: its fire burns them there.
    let mut actors: Vec<(FormId, V3)> = if gf.guest { Vec::new() } else { phys.world.actor_volumes().filter_map(|id| phys.world.actor_volume(id).map(|v| (FormId(id), v.center()))).collect() };
    if let Some(ch) = phys.player.as_ref() {
        actors.push((FormId::PLAYER_REF, [ch.feet[0], ch.feet[1], ch.feet[2] + ch.params.height.max(0.0) / 2.0]));
    }
    for (target, at) in actors {
        // The player in god mode takes no hit effects (`crate::combat`'s player hits).
        if target == FormId::PLAYER_REF && crate::combat::god_mode() {
            continue;
        }
        let Some((id, item, by)) = gf.fires.burning_at(at).map(|f| (f.id, f.item, f.by)) else { continue };
        let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
        let before = (stats.hit_effects.applied, stats.hit_effects.refreshed);
        with_world(rt, sim, |rt| crate::combat::hit_effects::apply(&mut rt.game, &mut stats.hit_effects, target, &[item], by));
        // Nothing took (dead, god mode, resisted): not caught.
        if (stats.hit_effects.applied, stats.hit_effects.refreshed) == before {
            continue;
        }
        if let Some(f) = gf.fires.fires.iter_mut().find(|f| f.id == id) {
            if f.caught.insert(target) {
                gf.fires.stats.actors_caught += 1;
                info!("grass fire: {} caught fire from fire {id}", who(script.as_deref(), target));
            }
        }
    }
}

fn who(script: Option<&ScriptState>, r: FormId) -> String {
    if r == FormId::PLAYER_REF {
        return "the player".to_string();
    }
    let name = script.and_then(|s| s.rt.as_ref()).map(|rt| rt.game.data.name(r).to_string()).unwrap_or_default();
    if name.is_empty() {
        format!("{:08X}", r.0)
    } else {
        format!("{name} ({:08X})", r.0)
    }
}

fn item_name(script: Option<&ScriptState>, item: FormId) -> String {
    let name = script.and_then(|s| s.rt.as_ref()).map(|rt| rt.game.data.name(item).to_string()).unwrap_or_default();
    if name.is_empty() {
        format!("{:08X}", item.0)
    } else {
        format!("{name} ({:08X})", item.0)
    }
}

fn report(mut exit: MessageReader<AppExit>, gf: Res<GrassFires>) {
    if exit.read().next().is_some() {
        info!("{}", gf.fires.summary());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEM: FormId = FormId(0x1234);

    /// A square field of grass `n` patches a side from the origin, all in cell (0, 0).
    fn field(n: i32) -> Fires {
        let mut f = Fires::default();
        f.add_fuel((0, 0), (0..n).flat_map(|x| (0..n).map(move |y| ((x, y), 100.0))));
        f
    }

    fn run(f: &mut Fires, secs: f32) {
        let mut t = 0.0;
        while t < secs {
            f.step(0.1);
            t += 0.1;
        }
    }

    fn point(k: Key) -> V3 {
        let c = centre(k);
        [c[0], c[1], 100.0]
    }

    /// Co-op: a guest with no grass of its own, lit only from the host's recorded
    /// patches as they come, burns the same patches and ends up with the same map.
    #[test]
    fn a_guest_burns_what_the_host_sends() {
        let mut host = field(8);
        host.record = true;
        let mut guest = Fires::default();
        host.ignite(point((4, 4)), ITEM, None).unwrap();
        let mut t = 0.0;
        while t < 60.0 {
            for p in std::mem::take(&mut host.lit) {
                assert!(guest.light_remote(p).is_some(), "each patch is sent once");
            }
            host.step(0.1);
            guest.step(0.1);
            t += 0.1;
        }
        assert_eq!(host.stats.patches, 64);
        assert_eq!(guest.stats.patches, 64);
        assert!(guest.fires().is_empty(), "the guest's copy of the fire ends too");
        assert_eq!(guest.map, host.map);
        // A patch already burning is not lit twice; nothing is recorded unless asked.
        let mut g = Fires::default();
        let p = LitPatch { key: (1, 1), z: 100.0, fire: 7, item: ITEM, by: None, secs: 5.0 };
        assert_eq!(g.light_remote(p), Some(true));
        assert_eq!(g.light_remote(LitPatch { key: (1, 2), ..p }), Some(false));
        assert_eq!(g.light_remote(p), None);
        assert!(g.lit.is_empty());
        // The guest's player standing in it is found burning (its own catch).
        assert_eq!(g.burning_at([centre((1, 1))[0], centre((1, 1))[1], 150.0]).map(|f| f.id), Some(7));
        // The patch goes with its cell.
        assert_eq!(g.forget_cell((0, 0)), 2);
    }

    #[test]
    fn a_guest_who_joins_late_gets_the_fire_as_it_is() {
        let mut host = field(8);
        host.ignite(point((4, 4)), ITEM, Some(FormId::PLAYER_REF)).unwrap();
        // Long enough that the middle burnt out and the edge still burns.
        run(&mut host, 9.0);
        let snap = host.snapshot();
        let burning = snap.iter().filter(|p| p.fire != BURNT_OUT).count();
        let out = snap.iter().filter(|p| p.fire == BURNT_OUT).count();
        assert!(burning > 0 && out > 0, "burning {burning}, burnt out {out}");
        assert_eq!(burning + out, host.stats.patches as usize, "every patch reached is in it");
        let mut guest = Fires::default();
        for p in &snap {
            assert!(guest.light_remote(*p).is_some());
        }
        // The guest has every patch: the burning ones burning (as fire 1, which the guest now
        // has), the burnt ones as ash.
        for p in &snap {
            match guest.burn(p.key) {
                Some(Burn::Burning { fire, .. }) => assert_eq!(fire, p.fire),
                Some(Burn::Ash) => assert_eq!(p.fire, BURNT_OUT),
                other => panic!("{:?}: {other:?}", p.key),
            }
            assert_ne!(guest.map[texel(p.key)], 0, "its grass and ground show the burn");
        }
        assert_eq!(guest.fires().len(), 1);
        assert_eq!(guest.events.iter().filter(|e| matches!(e, FireEvent::Lit { .. })).count(), burning, "flames only where it burns");
        // Both burn out alike: the same burnt-out patches at the end.
        run(&mut host, 60.0);
        guest.step(60.0);
        guest.step(60.0);
        for p in &snap {
            assert_eq!(guest.burn(p.key), Some(Burn::Ash));
        }
        assert!(guest.fires().is_empty());
        // Sent twice (a stream that starts again): nothing is lit twice.
        assert!(snap.iter().all(|p| guest.light_remote(*p).is_none()));
    }

    #[test]
    fn keys_and_texels_wrap() {
        assert_eq!(key(0.0, 63.9), (0, 0));
        assert_eq!(key(-0.1, 64.0), (-1, 1));
        assert_eq!(texel((0, 0)), 0);
        assert_eq!(texel((-1, 0)), MAP - 1);
        assert_eq!(texel((MAP as i32, 1)), MAP);
    }

    #[test]
    fn no_grass_no_fire() {
        let mut f = field(4);
        assert_eq!(f.ignite(point((40, 40)), ITEM, None), Err(Refused::NoGrass));
        // Just off the grass: the patch next to it catches.
        assert!(f.ignite(point((4, 2)), ITEM, None).is_ok());
        assert!(matches!(f.burn((3, 2)), Some(Burn::Burning { .. })));
    }

    #[test]
    fn a_fire_spreads_over_the_grass_burns_out_and_ends() {
        let mut f = field(8);
        let id = f.ignite(point((4, 4)), ITEM, Some(FormId::PLAYER_REF)).unwrap();
        run(&mut f, 60.0);
        // Every patch of the field burnt away (8 × 8 = 64, all within the reach).
        assert_eq!(f.stats.patches, 64);
        assert!((0..8).all(|x| (0..8).all(|y| f.burn((x, y)) == Some(Burn::Ash))));
        assert!(f.map.iter().filter(|&&v| v == 255).count() == 64);
        assert!(f.fires().is_empty());
        assert!(f.events.iter().any(|e| matches!(e, FireEvent::Ended { fire } if fire.id == id && fire.patches == 64)));
    }

    #[test]
    fn the_map_blackens_then_sinks() {
        let mut f = field(1);
        f.ignite(point((0, 0)), ITEM, None).unwrap();
        f.step(1.0);
        let v = f.map[texel((0, 0))];
        assert!(v > 2 && v < 128, "{v}");
        run(&mut f, 7.5);
        // Burnt out (at most 6 × 1.2 s), sinking.
        let v = f.map[texel((0, 0))];
        assert!((128..255).contains(&v), "{v}");
        run(&mut f, 6.0);
        assert_eq!(f.map[texel((0, 0))], 255);
    }

    #[test]
    fn the_reach_and_the_patch_count_hold() {
        // A field far wider than the reach: nothing lights beyond it.
        let mut f = field(60);
        f.ignite(point((30, 30)), ITEM, None).unwrap();
        run(&mut f, 200.0);
        let reach = f.rules.reach;
        let o = centre((30, 30));
        for x in 0..60 {
            for y in 0..60 {
                if f.burn((x, y)).is_some() {
                    let c = centre((x, y));
                    assert!((c[0] - o[0]).hypot(c[1] - o[1]) <= reach + 1e-3);
                }
            }
        }
        assert!(f.stats.held > 0);
        // A patch cap of 20.
        let mut f = Fires::new(FireRules { max_patches: 20, ..FireRules::default() });
        f.add_fuel((0, 0), (0..20).flat_map(|x| (0..20).map(move |y| ((x, y), 0.0))));
        f.ignite([640.0, 640.0, 0.0], ITEM, None).unwrap();
        run(&mut f, 100.0);
        assert_eq!(f.stats.patches, 20);
    }

    #[test]
    fn landings_feed_a_live_fire_and_the_fire_count_holds() {
        let mut f = Fires::new(FireRules { max_fires: 2, ..FireRules::default() });
        f.add_fuel((0, 0), (0..200).map(|x| ((x, 0), 0.0)));
        let a = f.ignite([32.0, 32.0, 0.0], ITEM, None).unwrap();
        // Within the reach of the first: the same fire.
        assert_eq!(f.ignite([32.0 + 640.0, 32.0, 0.0], ITEM, None), Ok(a));
        let b = f.ignite([32.0 + 64.0 * 50.0, 32.0, 0.0], ITEM, None).unwrap();
        assert_ne!(a, b);
        assert_eq!(f.ignite([32.0 + 64.0 * 100.0, 32.0, 0.0], ITEM, None), Err(Refused::TooMany));
        // Where it burns already.
        assert_eq!(f.ignite([32.0, 32.0, 0.0], ITEM, None), Err(Refused::Burnt));
    }

    #[test]
    fn the_wind_drives_the_fire() {
        // A line of grass both ways from the start; wind towards +x.
        let mut f = Fires { wind: [1.0, 0.0], ..Fires::default() };
        f.add_fuel((0, 0), (-12..=12).map(|x| ((x, 0), 0.0)));
        f.ignite([32.0, 32.0, 0.0], ITEM, None).unwrap();
        run(&mut f, 6.0);
        let lit = |x: i32| f.burn((x, 0)).is_some();
        let down = (1..=12).take_while(|&x| lit(x)).count();
        let up = (1..=12).take_while(|&x| lit(-x)).count();
        assert!(down > up, "downwind {down}, upwind {up}");
    }

    #[test]
    fn a_fire_crosses_one_bare_patch_but_not_two() {
        // Tufts at x = 0, 2 (one bare patch between), then 5 (two bare patches).
        let mut f = Fires::default();
        f.add_fuel((0, 0), [0, 2, 5].map(|x| ((x, 0), 0.0)));
        f.ignite([32.0, 32.0, 0.0], ITEM, None).unwrap();
        run(&mut f, 30.0);
        assert!(f.burn((2, 0)).is_some());
        assert!(f.burn((5, 0)).is_none());
    }

    #[test]
    fn a_forgotten_cell_keeps_what_burnt() {
        let mut f = Fires::default();
        f.add_fuel((0, 0), (0..4).map(|x| ((x, 0), 0.0)));
        f.add_fuel((1, 0), (64..68).map(|x| ((x, 0), 0.0)));
        f.ignite([32.0, 32.0, 0.0], ITEM, None).unwrap();
        run(&mut f, 2.0);
        assert!(f.burning() > 0);
        let reached = f.forget_cell((0, 0));
        assert!(reached > 0);
        assert_eq!(f.burning(), 0);
        // Away: nothing of it in the map (another cell may share its texels), no fuel.
        assert!(f.map.iter().all(|&v| v == 0));
        assert!(!f.has_fuel((0, 0)) && f.has_fuel((64, 0)));
        assert_eq!(f.kept(), reached);
        run(&mut f, 3.0);
        assert!(f.fires().is_empty());
        // Loaded again: what burnt is ash again, and does not burn twice.
        assert_eq!(f.add_fuel((0, 0), (0..4).map(|x| ((x, 0), 0.0))), reached);
        assert_eq!(f.kept(), 0);
        assert_eq!(f.burn((0, 0)), Some(Burn::Ash));
        assert_eq!(f.map[texel((0, 0))], 255);
        assert_eq!(f.ignite([32.0, 32.0, 0.0], ITEM, None), Err(Refused::Burnt));
    }

    /// Co-op (owner 2026-11-15): grass the host burnt where a guest was not is burnt when
    /// the guest gets there, whether the guest saw it burn from afar or joined later, and
    /// the host still has it after it walked away.
    #[test]
    fn burnt_grass_waits_for_a_player_who_comes_later() {
        let mut host = field(8);
        host.record = true;
        let mut guest = Fires::default();
        let mut late = Fires::default();
        host.ignite(point((4, 4)), ITEM, None).unwrap();
        let mut t = 0.0;
        while t < 60.0 {
            for p in std::mem::take(&mut host.lit) {
                guest.light_remote(p);
            }
            host.step(0.1);
            guest.step(0.1);
            // The guest is elsewhere: the cell is not loaded there, so it is forgotten
            // each frame (`grass_fire_fx`).
            guest.forget_cell((0, 0));
            t += 0.1;
        }
        assert_eq!(host.stats.patches, 64);
        assert_eq!(guest.kept(), 64);
        assert!(guest.map.iter().all(|&v| v == 0));
        // The host walks away, then a guest joins: the snapshot still holds them.
        assert_eq!(host.forget_cell((0, 0)), 64);
        let snap = host.snapshot();
        assert_eq!(snap.len(), 64);
        assert!(snap.iter().all(|p| p.fire == BURNT_OUT && p.z == 100.0));
        for p in snap {
            assert_eq!(late.light_remote(p), Some(false));
        }
        late.forget_cell((0, 0));
        // Each arrives (the cell loads, with less grass on the guests): all 64 burnt.
        host.add_fuel((0, 0), (0..8).flat_map(|x| (0..8).map(move |y| ((x, y), 100.0))));
        guest.add_fuel((0, 0), [((0, 0), 100.0)]);
        late.add_fuel((0, 0), []);
        for f in [&host, &guest, &late] {
            assert_eq!((0..8).flat_map(|x| (0..8).map(move |y| (x, y))).filter(|k| f.burn(*k) == Some(Burn::Ash)).count(), 64);
            assert_eq!(f.kept(), 0);
        }
        assert_eq!(guest.map, host.map);
        assert_eq!(late.map, host.map);
        // A patch kept is not lit twice by a digest sent again.
        let mut g = Fires::default();
        let p = LitPatch { key: (1, 1), z: 100.0, fire: BURNT_OUT, item: ITEM, by: None, secs: 0.0 };
        assert_eq!(g.light_remote(p), Some(false));
        g.forget_cell((0, 0));
        assert_eq!(g.light_remote(p), None);
    }

    #[test]
    fn actors_over_a_burning_patch_catch_its_fire() {
        let mut f = field(2);
        let id = f.ignite(point((0, 0)), ITEM, None).unwrap();
        // Standing on it (middle 60 above the ground), and high above it.
        assert_eq!(f.burning_at([32.0, 32.0, 160.0]).map(|f| f.id), Some(id));
        // Beside it, on the bare patch next door (60 units from its middle).
        assert_eq!(f.burning_at([92.0, 32.0, 160.0]).map(|f| f.id), Some(id));
        assert!(f.burning_at([32.0, 32.0, 600.0]).is_none());
        assert!(f.burning_at([32.0 + 640.0, 32.0, 160.0]).is_none());
    }
}
