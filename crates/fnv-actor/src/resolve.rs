//! From a placed NPC to the list of parts that make it up.
//!
//! Every rule here is [hypothesis] unless noted, written down in
//! docs/spec-notes/actors.md §2 so the in-game comparisons can correct it:
//! 1. An LVLN base is resolved to one actor base (§2.1); templates are followed per
//!    template-flag group (§2.2).
//! 2. Worn armour: the carried ARMO items, best protection first (DT, then DR), each
//!    worn unless a better one already covers one of its body slots (§2.3; the
//!    "best protection by DT and DR" part is the owner's statement, 2026-10-08).
//! 3. Race parts are hidden where worn armour covers their slot; headgear covering
//!    Hat selects the hair model's `Hat` shape (§2.4).
//! 4. The weapon an actor holds: among the same carried items, the drawable weapon with
//!    the highest base damage (§9; `crate::weapon::choose_weapon`).

use std::collections::{BTreeMap, HashMap};

use fnv_sim::forms::{BaseData, ItemKind};
use fnv_sim::{leveled, FormId, FormStore};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use serde::Serialize;

use crate::records::{template, ActorForms, Creature, FaceGen, Npc, RacePart};

/// BMDT biped slot bits [documented: xEdit wbBMDT].
pub mod slot {
    pub const HEAD: u32 = 1 << 0;
    pub const HAIR: u32 = 1 << 1;
    pub const UPPER_BODY: u32 = 1 << 2;
    pub const LEFT_HAND: u32 = 1 << 3;
    pub const RIGHT_HAND: u32 = 1 << 4;
    pub const PIPBOY: u32 = 1 << 6;
    pub const HAT: u32 = 1 << 10;
}

/// Template chains and nested leveled NPCs deeper than this are reported as cycles.
const MAX_DEPTH: u32 = 16;

/// The skeleton every NPC_ that names a real one uses [observed: 3,534 of 3,534 in the
/// ten masters; `fnvtool actor forms`].
pub const HUMAN_SKELETON: &str = r"meshes\characters\_male\skeleton.nif";

/// Editor placeholder models (`marker_creature.nif`, `marker_*.nif`).
pub fn is_editor_marker(model: &str) -> bool {
    let file = model.rsplit(['\\', '/']).next().unwrap_or(model).to_ascii_lowercase();
    file.starts_with("marker")
}

/// The level leveled lists resolve against for a static display (a level-1 player).
pub const DISPLAY_LEVEL: u16 = 1;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Part {
    /// What the part is: `body:upper_body`, `body:left_hand`, `body:right_hand`,
    /// `head:<index name>`, `hair`, `head_part`, `armor`.
    pub role: String,
    /// The form the part comes from (race, HAIR, HDPT or ARMO).
    pub form: FormId,
    /// Archive path, lower case (`meshes\...`).
    pub model: String,
    /// Texture that replaces the model's own diffuse texture, archive path (`textures\...`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Appearance {
    /// The actor base the appearance was taken from (after leveled resolution).
    pub npc: FormId,
    pub edid: Option<String>,
    pub female: bool,
    pub race: FormId,
    /// Skeleton, archive path.
    pub skeleton: String,
    /// The race's height for the actor's sex (DATA).
    pub race_height: f64,
    pub parts: Vec<Part>,
    /// Worn armour, in the order chosen.
    pub worn: Vec<FormId>,
    /// Whether worn headgear selects the hair model's `Hat` shape (§2.4).
    pub hat: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hair_color: Option<[u8; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facegen: Option<FaceGen>,
    /// Race FaceGen data for the actor's sex.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub race_facegen: Option<FaceGen>,
    /// Body index 3, "Upper Body Texture": an `.egt` for the body skin (part C).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_egt: Option<String>,
    /// The race's upper body skin texture (body index 0 ICON), whether or not the
    /// upper body is drawn: the texture the body `.egt` tints (engine use; not part of
    /// the appearance document).
    #[serde(skip)]
    pub body_texture: Option<String>,
    /// The weapon the actor draws (part H, §9). Not part of `fnv.actor.appearance/1`.
    #[serde(skip)]
    pub weapon: Option<FormId>,
    /// Whether a leveled choice that could have gone another way decided the actor base
    /// or its worn armour (§2.1): such actors depend on the random stream, the others are
    /// fully determined by the data.
    pub random: bool,
}

/// What a placed creature is drawn from (docs/spec-notes/actors.md §10).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreatureAppearance {
    /// The creature base after leveled resolution.
    pub creature: FormId,
    pub edid: Option<String>,
    /// DATA type of the Model/Animation source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Skeleton, archive path.
    pub skeleton: String,
    /// BNAM of the Stats source; 1 when absent.
    pub base_scale: f64,
    /// The NIFZ models, role `body`, in record order.
    pub parts: Vec<Part>,
    /// KFFZ names, lower case.
    pub anims: Vec<String>,
    /// Whether a leveled pick could have gone another way.
    pub random: bool,
}

/// Archive path of a creature's NIFZ entry: the name in the skeleton's folder
/// [observed: docs/spec-notes/animation.md §4].
pub fn creature_part_path(skeleton: &str, name: &str) -> String {
    let folder = skeleton.rsplit_once('\\').map_or("", |(f, _)| f);
    let name = name.trim().replace('/', "\\").to_ascii_lowercase();
    format!("{folder}\\{}", name.trim_start_matches('\\'))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ResolveError {
    NotAnActor(FormId),
    /// A creature base given to the NPC resolver ([`Resolver::resolve_creature`]
    /// handles it).
    Creature(FormId),
    LeveledEmpty(FormId),
    TooDeep(FormId),
    NoRace(FormId),
    UnknownRace(FormId),
    /// A creature was asked for, but the base is an NPC (or leveled NPC).
    NotACreature(FormId),
    /// A creature whose Model/Animation source names no skeleton, or an editor
    /// placeholder.
    NoSkeleton(FormId),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::NotAnActor(id) => write!(f, "{id} is not an actor base"),
            ResolveError::Creature(id) => write!(f, "{id} is a creature (not handled yet)"),
            ResolveError::LeveledEmpty(id) => write!(f, "leveled actor {id} has no eligible entry"),
            ResolveError::TooDeep(id) => write!(f, "template or leveled chain too deep at {id} (cycle?)"),
            ResolveError::NoRace(id) => write!(f, "{id} has no race"),
            ResolveError::UnknownRace(id) => write!(f, "race {id} is not loaded"),
            ResolveError::NotACreature(id) => write!(f, "{id} is not a creature base"),
            ResolveError::NoSkeleton(id) => write!(f, "creature {id} names no skeleton"),
        }
    }
}

/// Archive path of a model given relative to `meshes\` (or already under it).
pub fn mesh_path(p: &str) -> String {
    archive_path("meshes\\", p)
}

/// Archive path of a texture given relative to `textures\` (or already under it).
pub fn texture_path(p: &str) -> String {
    archive_path("textures\\", p)
}

fn archive_path(root: &str, p: &str) -> String {
    let p = p.trim().replace('/', "\\").to_ascii_lowercase();
    let p = p.trim_start_matches('\\');
    if p.starts_with(root) {
        p.to_string()
    } else {
        format!("{root}{p}")
    }
}

/// Deterministic per-reference random stream for leveled choices (not the game's roll).
pub fn rng_for(reference: FormId) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(u64::from(reference.0))
}

/// The leveled choices of one actor: each LVLN is picked once (all template groups that
/// name it see the same actor), and whether any choice could have gone another way.
struct Choices {
    rng: ChaCha8Rng,
    picks: HashMap<FormId, FormId>,
    random: bool,
}

pub struct Resolver<'a> {
    pub actors: &'a ActorForms,
    pub forms: &'a FormStore,
}

impl<'a> Resolver<'a> {
    /// Resolve an actor base placed by `reference` (the seed of its leveled choices).
    pub fn resolve(&self, base: FormId, reference: FormId) -> Result<Appearance, ResolveError> {
        self.resolve_wearing(base, reference, None)
    }

    /// [`Self::resolve`], with the armour worn given (`Some`: e.g. what the simulation
    /// has equipped on the player) instead of chosen from the carried items; entries that
    /// are not ARMO records are ignored.
    pub fn resolve_wearing(&self, base: FormId, reference: FormId, wearing: Option<&[FormId]>) -> Result<Appearance, ResolveError> {
        let mut ch = Choices { rng: rng_for(reference), picks: HashMap::new(), random: false };
        let npc = self.actor_base(base, &mut ch, 0)?;
        let traits = self.template_source(npc, template::TRAITS, &mut ch)?;
        let model = self.template_source(npc, template::MODEL_ANIMATION, &mut ch)?;
        let inventory = self.template_source(npc, template::INVENTORY, &mut ch)?;

        let race_id = traits.race.ok_or(ResolveError::NoRace(traits.id))?;
        let race = self.actors.races.get(&race_id).ok_or(ResolveError::UnknownRace(race_id))?;
        let female = traits.female;
        let sex = usize::from(female);
        // Skeleton (§2.2): an NPC_ naming no skeleton or the editor placeholder
        // `marker_creature.nif` takes the traits source's, else the human skeleton
        // [hypothesis: every other NPC_ names `characters\_male\skeleton.nif`].
        let named = |n: &Npc| n.skeleton.as_deref().filter(|s| !is_editor_marker(s)).map(mesh_path);
        let skeleton = named(model).or_else(|| named(traits)).unwrap_or_else(|| HUMAN_SKELETON.to_string());

        let carried = self.carried(inventory.id, &mut ch);
        let worn = match wearing {
            Some(w) => w.iter().copied().filter(|id| self.actors.armors.contains_key(id)).collect(),
            None => self.choose_armor(&carried),
        };
        let weapon = crate::weapon::choose_weapon(&carried, &self.actors.weapons);
        let covered = worn.iter().filter_map(|w| self.actors.armors.get(w)).fold(0, |acc, a| acc | a.slots);

        let mut parts = Vec::new();
        let mut body_egt = None;
        let mut body_texture = None;
        for p in &race.body[sex] {
            if p.index == 0 {
                body_texture = p.icon.as_deref().map(texture_path);
            }
            let (role, hidden_by) = match p.index {
                0 => ("body:upper_body", slot::UPPER_BODY),
                1 => ("body:left_hand", slot::LEFT_HAND),
                2 => ("body:right_hand", slot::RIGHT_HAND),
                3 => {
                    body_egt = p.model.as_deref().map(mesh_path);
                    continue;
                }
                _ => continue,
            };
            if covered & hidden_by == 0 {
                push_race_part(&mut parts, role.to_string(), race_id, p, None);
            }
        }
        let head_covered = covered & slot::HEAD != 0;
        let eye_texture = model.eyes.and_then(|e| self.actors.eyes.get(&e)).and_then(|e| e.texture.as_deref()).map(texture_path);
        if !head_covered {
            for p in &race.head[sex] {
                let role = format!("head:{}", head_part_name(p.index));
                // The eye texture comes from the actor's EYES (the race's eye parts have
                // no ICON [observed]).
                let texture = if matches!(p.index, 6 | 7) { eye_texture.clone() } else { None };
                push_race_part(&mut parts, role, race_id, p, texture);
            }
            for &hp in &model.head_parts {
                self.push_head_part(&mut parts, hp, 0);
            }
        }
        // Hair (§2.4): gone with the head; under headgear covering Hat, the hair model's
        // `Hat` shape; replaced (e.g. a wig) when Hair is covered without Hat.
        let hair = model.hair.or(race.default_hair[sex]);
        let hat = covered & slot::HAT != 0;
        if !head_covered && (hat || covered & slot::HAIR == 0) {
            if let Some((id, h)) = hair.and_then(|id| Some((id, self.actors.hairs.get(&id)?))) {
                if let Some(m) = &h.model {
                    parts.push(Part { role: "hair".into(), form: id, model: mesh_path(m), texture: h.texture.as_deref().map(texture_path) });
                }
            }
        }
        for &w in &worn {
            let Some(a) = self.actors.armors.get(&w) else { continue };
            // A female actor wears MOD3, falling back to the male model when the item has
            // none.
            if let Some(m) = a.worn[sex].as_ref().or(a.worn[0].as_ref()) {
                parts.push(Part { role: "armor".into(), form: w, model: mesh_path(m), texture: None });
            }
        }
        Ok(Appearance {
            npc: npc.id,
            edid: npc.edid.clone(),
            female,
            race: race_id,
            skeleton,
            race_height: race.height[sex],
            parts,
            worn,
            hat,
            hair_color: model.hair_color,
            facegen: model.facegen.clone(),
            race_facegen: race.facegen[sex].clone(),
            body_egt,
            body_texture,
            weapon,
            random: ch.random,
        })
    }

    fn push_head_part(&self, parts: &mut Vec<Part>, id: FormId, depth: u32) {
        let Some(h) = self.actors.head_parts.get(&id) else { return };
        if let Some(m) = &h.model {
            parts.push(Part { role: "head_part".into(), form: id, model: mesh_path(m), texture: None });
        }
        if depth < MAX_DEPTH {
            for &e in &h.extra {
                self.push_head_part(parts, e, depth + 1);
            }
        }
    }

    /// An NPC_ base for `id`, resolving leveled NPCs (LVLN) once per actor.
    fn actor_base(&self, id: FormId, ch: &mut Choices, depth: u32) -> Result<&'a Npc, ResolveError> {
        if depth > MAX_DEPTH {
            return Err(ResolveError::TooDeep(id));
        }
        if let Some(n) = self.actors.npcs.get(&id) {
            return Ok(n);
        }
        if let Some(l) = self.actors.leveled.get(&id) {
            let next = match ch.picks.get(&id) {
                Some(&p) => p,
                None => {
                    let (pool, _) = leveled_pool(&l.entries, l.all_levels, DISPLAY_LEVEL);
                    ch.random |= distinct(pool.iter().map(|e| e.1)) > 1;
                    let p = pick_leveled(&l.entries, l.all_levels, DISPLAY_LEVEL, &mut ch.rng).ok_or(ResolveError::LeveledEmpty(id))?;
                    ch.picks.insert(id, p);
                    p
                }
            };
            return self.actor_base(next, ch, depth + 1);
        }
        match self.actors.sigs.get(&id) {
            Some(b"CREA") | Some(b"LVLC") => Err(ResolveError::Creature(id)),
            _ => Err(ResolveError::NotAnActor(id)),
        }
    }

    /// The NPC whose data a template-flag group comes from: follow `TPLT` while the
    /// current NPC has the group's flag set (§2.2).
    fn template_source(&self, npc: &'a Npc, group: u16, ch: &mut Choices) -> Result<&'a Npc, ResolveError> {
        let mut cur = npc;
        for _ in 0..MAX_DEPTH {
            match cur.template {
                Some(t) if cur.template_flags & group != 0 => cur = self.actor_base(t, ch, 0)?,
                _ => return Ok(cur),
            }
        }
        Err(ResolveError::TooDeep(npc.id))
    }

    /// Resolve a creature base placed by `reference` (actors.md §10): an LVLC is picked
    /// like an LVLN (§2.1); the skeleton, model list and animation list come from the
    /// Model/Animation source of the template chain, the base scale from the Stats
    /// source.
    pub fn resolve_creature(&self, base: FormId, reference: FormId) -> Result<CreatureAppearance, ResolveError> {
        let mut ch = Choices { rng: rng_for(reference), picks: HashMap::new(), random: false };
        let crea = self.creature_base(base, &mut ch, 0)?;
        let model = self.creature_source(crea, template::MODEL_ANIMATION, &mut ch)?;
        let stats = self.creature_source(crea, template::STATS, &mut ch)?;
        let skeleton =
            model.skeleton.as_deref().filter(|s| !is_editor_marker(s)).map(mesh_path).ok_or(ResolveError::NoSkeleton(model.id))?;
        let parts = model
            .models
            .iter()
            .map(|m| Part { role: "body".into(), form: model.id, model: creature_part_path(&skeleton, m), texture: None })
            .collect();
        Ok(CreatureAppearance {
            creature: crea.id,
            edid: crea.edid.clone(),
            kind: model.kind.clone(),
            skeleton,
            base_scale: stats.base_scale.unwrap_or(1.0),
            parts,
            anims: model.anims.iter().map(|a| a.replace('/', "\\").to_ascii_lowercase()).collect(),
            random: ch.random,
        })
    }

    /// A CREA base for `id`, resolving leveled creatures (LVLC) once per actor.
    fn creature_base(&self, id: FormId, ch: &mut Choices, depth: u32) -> Result<&'a Creature, ResolveError> {
        if depth > MAX_DEPTH {
            return Err(ResolveError::TooDeep(id));
        }
        if let Some(c) = self.actors.creatures.get(&id) {
            return Ok(c);
        }
        if let Some(l) = self.actors.leveled_creatures.get(&id) {
            let next = match ch.picks.get(&id) {
                Some(&p) => p,
                None => {
                    let (pool, _) = leveled_pool(&l.entries, l.all_levels, DISPLAY_LEVEL);
                    ch.random |= distinct(pool.iter().map(|e| e.1)) > 1;
                    let p = pick_leveled(&l.entries, l.all_levels, DISPLAY_LEVEL, &mut ch.rng).ok_or(ResolveError::LeveledEmpty(id))?;
                    ch.picks.insert(id, p);
                    p
                }
            };
            return self.creature_base(next, ch, depth + 1);
        }
        match self.actors.sigs.get(&id) {
            Some(b"NPC_") | Some(b"LVLN") => Err(ResolveError::NotACreature(id)),
            _ => Err(ResolveError::NotAnActor(id)),
        }
    }

    /// The creature whose data a template-flag group comes from (as
    /// [`Self::template_source`] for NPCs).
    fn creature_source(&self, c: &'a Creature, group: u16, ch: &mut Choices) -> Result<&'a Creature, ResolveError> {
        let mut cur = c;
        for _ in 0..MAX_DEPTH {
            match cur.template {
                Some(t) if cur.template_flags & group != 0 => cur = self.creature_base(t, ch, 0)?,
                _ => return Ok(cur),
            }
        }
        Err(ResolveError::TooDeep(c.id))
    }

    /// The carried items of `npc`, leveled lists resolved, in carried order.
    fn carried(&self, npc: FormId, ch: &mut Choices) -> Vec<FormId> {
        let Some(entries) = self.forms.entries(npc) else { return Vec::new() };
        let mut carried = Vec::new();
        for e in entries {
            ch.random |= e.count > 0 && leveled_item_random(self.forms, e.item, 0);
            // A leveled list that fails to resolve contributes nothing; the store
            // reports cycles when it loads.
            let _ = leveled::resolve(self.forms, e.item, e.count, DISPLAY_LEVEL, &mut ch.rng, &mut carried);
        }
        carried.iter().map(|e| e.item).collect()
    }

    /// Worn armour from the carried items (§2.3).
    fn choose_armor(&self, carried: &[FormId]) -> Vec<FormId> {
        let mut candidates: Vec<(usize, FormId)> = Vec::new();
        for (order, &item) in carried.iter().enumerate() {
            let is_armor = matches!(self.forms.get(item).map(|b| &b.data), Some(BaseData::Item(i)) if i.kind == ItemKind::Armor);
            if is_armor && self.actors.armors.contains_key(&item) && !candidates.iter().any(|c| c.1 == item) {
                candidates.push((order, item));
            }
        }
        let armors: BTreeMap<FormId, (f32, i32, u32)> = candidates
            .iter()
            .filter_map(|&(_, id)| self.actors.armors.get(&id).map(|a| (id, (a.dt, a.dr_raw, a.slots))))
            .collect();
        choose_by_protection(&candidates, &armors)
    }
}

/// Best protection first: DT descending, then DR descending, then carried order; an item
/// is worn unless an already chosen one shares a body slot. Items with no slot are not
/// worn.
pub fn choose_by_protection(candidates: &[(usize, FormId)], armors: &BTreeMap<FormId, (f32, i32, u32)>) -> Vec<FormId> {
    let mut sorted: Vec<_> = candidates.iter().filter_map(|&(order, id)| Some((order, id, *armors.get(&id)?))).collect();
    sorted.sort_by(|a, b| {
        b.2 .0.total_cmp(&a.2 .0).then(b.2 .1.cmp(&a.2 .1)).then(a.0.cmp(&b.0))
    });
    let mut used = 0u32;
    let mut worn = Vec::new();
    for (_, id, (_, _, slots)) in sorted {
        if slots == 0 || used & slots != 0 {
            continue;
        }
        used |= slots;
        worn.push(id);
    }
    worn
}

/// The entries a leveled pick chooses from (first value), and the eligible entries
/// (second): eligible entries have `level <= level`; the pool is all of them with
/// "Calculate from all levels", else those of the highest eligible level.
pub type Pool<T> = (Vec<(u16, T)>, Vec<(u16, T)>);

pub fn leveled_pool<T: Copy>(entries: &[(u16, T)], all_levels: bool, level: u16) -> Pool<T> {
    let eligible: Vec<_> = entries.iter().copied().filter(|e| e.0 <= level).collect();
    let pool = if all_levels {
        eligible.clone()
    } else {
        match eligible.iter().map(|e| e.0).max() {
            Some(top) => eligible.iter().copied().filter(|e| e.0 == top).collect(),
            None => Vec::new(),
        }
    };
    (pool, eligible)
}

fn distinct(ids: impl Iterator<Item = FormId>) -> usize {
    ids.collect::<std::collections::BTreeSet<_>>().len()
}

/// One entry of a leveled NPC list, uniformly from [`leveled_pool`], as for leveled items
/// (fnv-sim `leveled`). Chance none is not applied: a static display always shows an
/// actor (a display choice, not a claim about the game).
pub fn pick_leveled(entries: &[(u16, FormId)], all_levels: bool, level: u16, rng: &mut ChaCha8Rng) -> Option<FormId> {
    let (pool, _) = leveled_pool(entries, all_levels, level);
    if pool.is_empty() {
        return None;
    }
    Some(pool[leveled::below(rng, pool.len() as u32) as usize].1)
}

/// Whether resolving `item` at the display level can change which armour it yields: it
/// can yield armour at all, and involves a leveled item list with a chance none above 0
/// or an LVLG global, a pool of more than one distinct entry, or such a list among the
/// entries it can produce. Plain items are not random.
pub fn leveled_item_random(forms: &FormStore, item: FormId, depth: u32) -> bool {
    let Some(list) = forms.leveled(item) else { return false };
    if depth > MAX_DEPTH {
        return true;
    }
    if !yields_armor(forms, item, 0) {
        return false;
    }
    let entries: Vec<(u16, FormId)> = list.entries.iter().map(|e| (e.level, e.item)).collect();
    let (pool, eligible) = leveled_pool(&entries, list.all_levels, DISPLAY_LEVEL);
    if list.chance_none > 0 || list.global.is_some() {
        return true;
    }
    let reachable = if list.use_all { eligible } else { pool };
    if !list.use_all && distinct(reachable.iter().map(|e| e.1)) > 1 {
        return true;
    }
    reachable.iter().any(|e| leveled_item_random(forms, e.1, depth + 1))
}

/// Whether `item` is armour or a leveled list that can produce armour at the display
/// level.
fn yields_armor(forms: &FormStore, item: FormId, depth: u32) -> bool {
    if let Some(list) = forms.leveled(item) {
        let entries: Vec<(u16, FormId)> = list.entries.iter().map(|e| (e.level, e.item)).collect();
        let (pool, eligible) = leveled_pool(&entries, list.all_levels, DISPLAY_LEVEL);
        let reachable = if list.use_all { eligible } else { pool };
        return depth <= MAX_DEPTH && reachable.iter().any(|e| yields_armor(forms, e.1, depth + 1));
    }
    matches!(forms.get(item).map(|b| &b.data), Some(BaseData::Item(i)) if i.kind == ItemKind::Armor)
}

fn push_race_part(parts: &mut Vec<Part>, role: String, race: FormId, p: &RacePart, texture: Option<String>) {
    // Ears (index 1) have a texture but no model [observed: CaucasianOld].
    let Some(model) = &p.model else { return };
    let texture = texture.or_else(|| p.icon.as_deref().map(texture_path));
    parts.push(Part { role, form: race, model: mesh_path(model), texture });
}

/// [documented: xEdit wbHeadPartIndexEnum]
pub fn head_part_name(index: u32) -> &'static str {
    match index {
        0 => "head",
        1 => "ears",
        2 => "mouth",
        3 => "teeth_lower",
        4 => "teeth_upper",
        5 => "tongue",
        6 => "left_eye",
        7 => "right_eye",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn armors(list: &[(u32, f32, i32, u32)]) -> BTreeMap<FormId, (f32, i32, u32)> {
        list.iter().map(|&(id, dt, dr, slots)| (FormId(id), (dt, dr, slots))).collect()
    }

    #[test]
    fn best_dt_wins_a_shared_slot() {
        let a = armors(&[(1, 2.0, 0, slot::UPPER_BODY), (2, 12.0, 0, slot::UPPER_BODY)]);
        assert_eq!(choose_by_protection(&[(0, FormId(1)), (1, FormId(2))], &a), vec![FormId(2)]);
    }

    #[test]
    fn dr_breaks_dt_ties_then_carried_order() {
        let a = armors(&[(1, 5.0, 0, slot::UPPER_BODY), (2, 5.0, 300, slot::UPPER_BODY), (3, 5.0, 300, slot::UPPER_BODY)]);
        let c = [(0, FormId(1)), (1, FormId(3)), (2, FormId(2))];
        assert_eq!(choose_by_protection(&c, &a), vec![FormId(3)]);
    }

    #[test]
    fn disjoint_slots_are_all_worn() {
        let a = armors(&[(1, 6.0, 0, slot::UPPER_BODY), (2, 1.0, 0, slot::HAT), (3, 0.0, 0, 0)]);
        let c = [(0, FormId(2)), (1, FormId(1)), (2, FormId(3))];
        assert_eq!(choose_by_protection(&c, &a), vec![FormId(1), FormId(2)]);
    }

    #[test]
    fn partial_overlap_blocks_the_weaker_item() {
        let a = armors(&[(1, 3.0, 0, slot::HEAD | slot::HAIR), (2, 1.0, 0, slot::HAIR | slot::HAT)]);
        assert_eq!(choose_by_protection(&[(0, FormId(2)), (1, FormId(1))], &a), vec![FormId(1)]);
    }

    #[test]
    fn leveled_pick_prefers_highest_eligible_level() {
        let mut rng = rng_for(FormId(7));
        let e = [(1, FormId(10)), (5, FormId(11)), (1, FormId(12))];
        for _ in 0..20 {
            let p = pick_leveled(&e, false, 1, &mut rng).unwrap();
            assert!(p == FormId(10) || p == FormId(12));
        }
        assert_eq!(pick_leveled(&e, false, 9, &mut rng), Some(FormId(11)));
        assert_eq!(pick_leveled(&[(3, FormId(1))], false, 1, &mut rng), None);
    }

    #[test]
    fn editor_markers() {
        assert!(is_editor_marker("marker_creature.nif"));
        assert!(is_editor_marker(r"Meshes\Marker_X.NIF"));
        assert!(!is_editor_marker(r"Characters\_Male\Skeleton.NIF"));
    }

    #[test]
    fn creature_parts_sit_beside_the_skeleton() {
        let sk = mesh_path(r"creatures\NVBigHorner\skeleton.nif");
        assert_eq!(creature_part_path(&sk, "NVBigHorner.nif"), r"meshes\creatures\nvbighorner\nvbighorner.nif");
        assert_eq!(creature_part_path(&sk, " Sub/Part.NIF"), r"meshes\creatures\nvbighorner\sub\part.nif");
    }

    #[test]
    fn paths() {
        assert_eq!(mesh_path("Characters\\_Male\\Skeleton.NIF"), "meshes\\characters\\_male\\skeleton.nif");
        assert_eq!(mesh_path("meshes/a.nif"), "meshes\\a.nif");
        assert_eq!(texture_path("\\Characters\\Male\\HeadHumanOld.dds"), "textures\\characters\\male\\headhumanold.dds");
    }
}
