//! The simulation as a resource: `fnv_sim::World` over the loaded plugins, its game clock
//! running in real time while the player is in the world.

use crate::PlaySet;
use bevy::prelude::*;
use fnv_esm::{LoadOrder, Schema};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use fnv_world::WorldIndex;

#[derive(Resource)]
pub struct SimState {
    pub forms: &'static FormStore,
    pub refs: &'static RefTable,
    pub world: World<'static>,
}

/// The run's one table of placed references for load order `lo` and world index
/// `index`, built on the first call and shared by every later one (they live for the whole
/// run). nvforever's residents (the AI) and the simulation ([`SimState::shared`]) each
/// built their own before, the same table twice: about 265 MB of heap each with the
/// masters and DLCs [observed 2026-11-20, `NVF_HEAP_STATS`, split screen's RAM work]. The
/// table is read only (`&'static`), so sharing it changes nothing they see.
pub fn world_refs(lo: &'static LoadOrder<'static>, schema: &'static Schema, index: &'static WorldIndex) -> &'static RefTable {
    type Key = (usize, usize, usize);
    static TABLES: std::sync::Mutex<Vec<(Key, &'static RefTable)>> = std::sync::Mutex::new(Vec::new());
    // Leaked inputs are never freed, so their addresses name them for the whole run.
    let key = (std::ptr::from_ref(lo) as usize, std::ptr::from_ref(schema) as usize, std::ptr::from_ref(index) as usize);
    let mut tables = TABLES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, t)) = tables.iter().find(|(k, _)| *k == key) {
        return t;
    }
    let t: &'static RefTable = Box::leak(Box::new(RefTable::from_index(lo, schema, index)));
    tables.push((key, t));
    t
}

impl SimState {
    /// Build the simulation's tables from the plugins (they live for the whole run).
    pub fn new(lo: &LoadOrder, schema: &Schema, index: &WorldIndex) -> SimState {
        let refs: &'static RefTable = Box::leak(Box::new(RefTable::from_index(lo, schema, index)));
        Self::with_refs(lo, schema, refs)
    }

    /// [`SimState::new`] over the run's one reference table ([`world_refs`]): the game's
    /// (`PlayPlugin`), which nvforever's residents share.
    pub fn shared(lo: &'static LoadOrder<'static>, schema: &'static Schema, index: &'static WorldIndex) -> SimState {
        Self::with_refs(lo, schema, world_refs(lo, schema, index))
    }

    fn with_refs(lo: &LoadOrder, schema: &Schema, refs: &'static RefTable) -> SimState {
        let mut forms = FormStore::build(lo, schema);
        // The mods' settings NVForever ships different (`crate::mod_defaults`).
        crate::mod_defaults::apply(&mut forms);
        let forms: &'static FormStore = Box::leak(Box::new(forms));
        crate::pipboy::labels::set(&forms.settings);
        let moved = refs.issues.count(fnv_sim::refs::XTEL_MOVED);
        if moved > 0 {
            bevy::log::info!("doors: {moved} XTEL marker(s) moved with their destination door (a later plugin moved the door): {}", refs.issues.samples(fnv_sim::refs::XTEL_MOVED).join("; "));
        }
        SimState { forms, refs, world: World::new(forms, refs, SimConfig::default()) }
    }

    /// The level `r` reads leveled lists at: fixed when its cell loaded, else what it would
    /// be fixed at now (`fnv_game::Game::leveled_at`). Read from this world, the one played:
    /// outside a game turn `game.world` is an empty stand-in (`script::with_world`), so
    /// `game.leveled_at` there would miss every fixed level.
    pub fn leveled_at(&self, game: &fnv_game::Game<'_>, r: FormId) -> fnv_sim::zones::LevelAt {
        let player = game.player.level.map_or(self.world.config().player_level, |l| l.clamp(1, i32::from(u16::MAX)) as u16);
        self.world.level_at(r, player)
    }

    /// The seed and level `r`'s leveled picks are made with, as the game makes them in a
    /// turn ([`fnv_game::Game::pick_seed`], [`SimState::leveled_at`]): for the data's
    /// template lookups outside a turn (docs/spec-notes/actors.md §1.3).
    pub fn pick(&self, game: &fnv_game::Game<'_>, r: FormId) -> (FormId, fnv_sim::zones::LevelAt) {
        (game.pick_seed(r), self.leveled_at(game, r))
    }

    /// The base form of a reference as the simulation sees it now.
    pub fn base_of(&self, form_id: u32) -> Option<FormId> {
        let id = FormId(form_id);
        self.world.peek(id).map(|s| s.base).or_else(|| self.refs.get(id).map(|p| p.base))
    }

    /// Whether a reference is an actor: a placed ACHR or ACRE, or a reference created at
    /// run time (PlaceAtMe) of an actor base. Created actors are not among the placed
    /// references; tests of the placed table alone left them without a hit volume, so no
    /// shot could hit them (the stress tests' battle, 2026-10-18: the NCR troopers' rifle
    /// fire went through the created Legion raiders into the ground).
    pub fn is_actor(&self, form_id: u32) -> bool {
        let id = FormId(form_id);
        let placed = self.refs.get(id).map(|p| p.sig);
        let created = id.is_dynamic().then(|| self.world.peek(id).and_then(|s| self.forms.sig(s.base))).flatten();
        actor_ref(placed, created)
    }

    /// Whether a reference could pass [`SimState::is_actor`] now or later, whatever the
    /// simulation does: a placed ACHR or ACRE (a placed reference's record type never
    /// changes) or any id without a placement that is dynamic (a created reference's base
    /// is read from the world, and the id may be given to another base). Everything
    /// `is_actor` accepts passes this; the reverse is not so, it is no substitute for it.
    pub fn may_be_actor(&self, form_id: u32) -> bool {
        let id = FormId(form_id);
        match self.refs.get(id) {
            Some(p) => actor_ref(Some(p.sig), None),
            None => id.is_dynamic(),
        }
    }

    /// Display name of a base form (FULL), else its Editor ID.
    pub fn name(&self, base: FormId) -> Option<&str> {
        let b = self.forms.get(base)?;
        b.name.as_deref().or(b.edid.as_deref())
    }

    /// The name a placed reference is shown by: for an actor, its "Base Data" template
    /// source's ([`fnv_game::GameData::display_base`]: an actor placed from a template
    /// actor with no name of its own shows the name of the actor its template, through a
    /// leveled list, leads to; owner's playtest list 4, item 20); otherwise, and without
    /// the game data, its base form's ([`SimState::name`]). A name a script gave the base
    /// (SetActorFullName, [`fnv_game::Game::script_name`]) comes first: it renames every
    /// reference of the base [documented: GECK wiki SetActorFullName].
    pub fn ref_name<'s>(&'s self, game: Option<&'s fnv_game::Game<'_>>, form_id: u32) -> Option<&'s str> {
        // Another player of a co-op game goes by its player's name.
        if let Some(n) = game.and_then(|g| g.coop_actors.get(&FormId(form_id))) {
            return Some(n);
        }
        // NVForever's poker tables (blackjack tables in the plugins).
        if crate::poker_tables::is_poker_table(self, FormId(form_id)) {
            return Some(crate::poker_tables::NAME);
        }
        let base = self.base_of(form_id)?;
        let shown = game.map_or(base, |g| g.display_base(base, FormId(form_id)));
        let renamed = game.and_then(|g| g.script_name(base).or_else(|| g.script_name(shown)));
        renamed.or_else(|| self.name(shown)).or_else(|| self.name(base))
    }

    /// A placed reference's pose in the simulation (position, Euler angles in radians)
    /// when it differs from its placement: moved by physics (pushed or knocked clutter,
    /// `crate::phys::PhysState::settle_placed`) or by a script. `None` for references the
    /// plugins do not place (the player, dropped items) and for those still where the
    /// plugins put them.
    pub fn moved_pose(&self, form_id: u32) -> Option<([f32; 3], [f32; 3])> {
        let id = FormId(form_id);
        let (s, p) = (self.world.peek(id)?, self.refs.get(id)?);
        (s.pos != p.pos || s.rot != p.rot).then_some((s.pos, s.rot))
    }

    /// The scale a reference is drawn and collides at: a script's (SetScale), else its
    /// placement's (XSCL), else 1; never below [`MIN_DRAWN_SCALE`].
    pub fn ref_scale(&self, form_id: u32) -> f32 {
        let id = FormId(form_id);
        self.script_scale(form_id).or_else(|| self.refs.get(id).and_then(|p| p.scale)).unwrap_or(1.0).max(MIN_DRAWN_SCALE)
    }

    /// The scale a script set on a reference (SetScale), if any.
    pub fn script_scale(&self, form_id: u32) -> Option<f32> {
        self.world.peek(FormId(form_id)).and_then(|s| s.scale)
    }

    /// Whether a placed reference is in the world now (not taken or deleted, enabled).
    pub fn present(&self, form_id: u32) -> bool {
        let id = FormId(form_id);
        !self.world.peek(id).is_some_and(|s| s.deleted()) && self.world.is_enabled(id).unwrap_or(true)
    }
}

#[cfg(test)]
impl SimState {
    /// A simulation of these placed references, with no base forms (no game data).
    pub(crate) fn over_placed(placed: impl IntoIterator<Item = fnv_sim::refs::PlacedRef>) -> SimState {
        let mut refs = RefTable::default();
        refs.refs.extend(placed.into_iter().map(|p| (p.id, p)));
        let forms: &'static FormStore = Box::leak(Box::default());
        let refs: &'static RefTable = Box::leak(Box::new(refs));
        SimState { forms, refs, world: World::new(forms, refs, SimConfig::default()) }
    }
}

/// A placed reference of nothing in particular, for [`SimState::over_placed`].
#[cfg(test)]
pub(crate) fn placed_ref(id: u32) -> fnv_sim::refs::PlacedRef {
    fnv_sim::refs::PlacedRef {
        id: FormId(id),
        sig: *b"REFR",
        edid: None,
        base: FormId(0x100),
        cell: Some(FormId(0x200)),
        stored_in: Some(FormId(0x200)),
        plugin: 0,
        flags: 0,
        in_persistent_group: false,
        pos: [0.0; 3],
        rot: [0.0; 3],
        scale: None,
        count: None,
        ammo: None,
        health: None,
        owner: None,
        lock: None,
        teleport: None,
        enable_parent: None,
        linked: None,
        patrol_idle: None,
        patrol_topic: None,
        merchant_container: None,
        marker_reputation: None,
        open_by_default: false,
        primitive: None,
        activate_parents: Vec::new(),
        parent_activate_only: false,
        prompt: None,
        radiation: None,
        radius: None,
        encounter_zone: None,
        level_modifier: None,
        map_marker: None,
        radio: None,
    }
}

/// The smallest scale a reference is drawn at: `SetScale 0` (an artillery piece the
/// scripts hide that way, FalloutNV.esm SCPT 0x001043CC) draws it this small, too small
/// to see, so that the drawn transform stays invertible; below it the reference has no
/// collision (`crate::phys`) [design].
pub const MIN_DRAWN_SCALE: f32 = 1e-3;

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        // Before the scripts, so a frame's scripts all see that frame's game time (one
        // order every frame; which comes first in the game is [hypothesis]: the capture of
        // docs/spec-notes/layer5-captures.md §9 only shows a quest script's own clock
        // change reaching object scripts in the same frame).
        // In co-op the clock goes on behind a minigame table (`crate::coop_tables`).
        app.add_systems(Update, advance_clock.run_if(crate::coop_tables::world_goes_on).before(PlaySet::Scripts));
    }
}

/// Game time runs at TimeScale while the player is in the world, not in the container
/// and inventory menus (`PlayState::Menu`) [observed for the container menu: the game
/// clock stopped there, docs/spec-notes/layer5-captures.md §8.3 e; it kept running in the
/// console and Wait menus, docs/spec-notes/simulation.md §5b]. It stops in a conversation
/// with the player as well (`PlayState::Dialogue`) [owner ruling 2026-10-10: a
/// conversation with the player pauses the game; `crate::dialogue`].
fn advance_clock(time: Res<Time>, mut sim: ResMut<SimState>) {
    if let Err(e) = sim.world.advance_real_seconds(time.delta_secs_f64()) {
        warn_once!("game clock: {e}");
    }
}

/// [`SimState::is_actor`]'s rule: a placed reference by its record type (ACHR, ACRE), a
/// created one by its base's (NPC_, CREA, or a leveled list of them).
pub fn actor_ref(placed_sig: Option<[u8; 4]>, created_base_sig: Option<[u8; 4]>) -> bool {
    match (placed_sig, created_base_sig) {
        (Some(s), _) => matches!(&s, b"ACHR" | b"ACRE"),
        (None, Some(b)) => matches!(&b, b"NPC_" | b"CREA" | b"LVLN" | b"LVLC"),
        (None, None) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_actors_are_actors_as_placed_ones_are() {
        assert!(actor_ref(Some(*b"ACHR"), None) && actor_ref(Some(*b"ACRE"), None));
        assert!(!actor_ref(Some(*b"REFR"), None));
        // PlaceAtMe of an NPC, a creature or a leveled list of either.
        for base in [b"NPC_", b"CREA", b"LVLN", b"LVLC"] {
            assert!(actor_ref(None, Some(*base)), "{base:?}");
        }
        // A created item or container is not.
        assert!(!actor_ref(None, Some(*b"MISC")) && !actor_ref(None, Some(*b"CONT")) && !actor_ref(None, None));
    }
}

/// A simulation made by hand for tests: forms and references in memory, no game data.
#[cfg(test)]
pub(crate) mod test_world {
    use super::*;
    use fnv_sim::forms::{ActorBase, BaseData, BaseForm, Door, Item, ItemKind};
    use fnv_sim::refs::PlacedRef;

    fn base(id: u32, data: BaseData) -> BaseForm {
        BaseForm { id: FormId(id), sig: *b"MISC", edid: Some(format!("Edid{id:X}")), name: None, script: None, prompt: None, voice_filter: false, data }
    }

    pub(crate) fn item_base(id: u32) -> BaseForm {
        base(id, BaseData::Item(Item { kind: ItemKind::Aid, value: Some(1), money: None, weight: Some(0.5), slots: None, projectile: None, health: None, non_playable: false, cant_drop: false, armor_class: Default::default(), power_armor: false, armor_dr: None, armor_dt: None, weapon_damage: None, weight_mods: [0.0; 3], drawable: false, not_for_npcs: false, thrown: false, quest_item: false, icon: None, placed_count: None, teaches: None, cant_be_taken: false, modulates_voice: false, weapon_traits: None }))
    }

    pub(crate) fn door_base(id: u32) -> BaseForm {
        base(id, BaseData::Door(Door { automatic: false, minimal_use: false }))
    }

    /// A plain reference of `base` nowhere in particular.
    pub(crate) fn placed(id: u32, base: u32) -> PlacedRef {
        PlacedRef {
            id: FormId(id),
            sig: *b"REFR",
            edid: None,
            base: FormId(base),
            cell: None,
            stored_in: None,
            plugin: 0,
            flags: 0,
            in_persistent_group: false,
            pos: [0.0; 3],
            rot: [0.0; 3],
            scale: None,
            count: None,
            ammo: None,
            health: None,
            owner: None,
            lock: None,
            teleport: None,
            enable_parent: None,
            linked: None,
            patrol_idle: None,
            patrol_topic: None,
            merchant_container: None,
            marker_reputation: None,
            open_by_default: false,
            primitive: None,
            activate_parents: Vec::new(),
            parent_activate_only: false,
            prompt: None,
            radiation: None,
            radius: None,
            encounter_zone: None,
            level_modifier: None,
            map_marker: None,
            radio: None,
        }
    }

    /// These bases (and the player's) and references.
    pub(crate) fn sim(bases: Vec<BaseForm>, placed: Vec<PlacedRef>) -> SimState {
        let mut forms = FormStore::default();
        for b in bases.into_iter().chain([base(FormId::PLAYER_BASE.0, BaseData::Actor(ActorBase { entries: Vec::new(), attributes: None }))]) {
            // `World::place` knows a form by its signature.
            forms.sigs.insert(b.id, b.sig);
            forms.bases.insert(b.id, b);
        }
        let mut refs = RefTable::default();
        for r in placed {
            // In its cell's list, as `RefTable::from_index` puts it (what a cell reset
            // walks).
            if let Some(c) = r.cell {
                refs.by_cell.entry(c).or_default().push(r.id);
            }
            refs.refs.insert(r.id, r);
        }
        let forms: &'static FormStore = Box::leak(Box::new(forms));
        let refs: &'static RefTable = Box::leak(Box::new(refs));
        SimState { forms, refs, world: World::new(forms, refs, SimConfig::default()) }
    }
}
