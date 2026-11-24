//! Item animations of the B42 Inject and B42 Interact mods, played natively (the co-op
//! fork's own [design: owner 2026-11-04]; the folders' conventions are
//! `fnv_actor::b42`, docs/spec-notes/b42-anims.md). The mods leave the playing to script
//! extender plugins this engine does not load; here the engine plays them.
//!
//! - **Consume.** An aid item the player uses (the inventory, the Pip-Boy, the hotkeys)
//!   with clips in its folder plays one of them, picked at random, in the view the player
//!   is in (`fnv_actor::b42::Index::inject`): first person on the arms rig, third person
//!   on the body (both through `PlayerIdles`, as an idle a script asked for). Used from
//!   the Pip-Boy or any menu, it plays once the menu is closed. In third person with no
//!   clip of that view, the body plays the game's own idle for the kind of item
//!   (`fnv_actor::b42::Style`: drinking, eating, the stimpak, smoking), so the body shows
//!   what the others see. Not while swimming, ragdolled, reloading, attacking or already
//!   playing one (`fnv_actor::b42::decide`, the mod's rule); in power armour only with the
//!   option on.
//! - **The item's sound** waits for the animation (`nvf_play::ConsumeAnims` holds it back
//!   in `ui_sounds`): it plays at the clip's key for it, or at once when no animation
//!   plays. A clip without such a key plays its own sounds (`Sound:` keys, heard by
//!   `anim_sounds.rs`, and the files of its `SoundPath:` keys here) [observed: the
//!   mod's script plays the item's sound only from its key].
//! - **The model in the hand.** The clips' keys attach a model at `B42AttachPointL`
//!   under `Bip01 L Hand` (the model they name, else the item's own) and take it away
//!   (`fnv_actor::b42::Cue`); the attach point follows the clip's track of that name, and
//!   the model's nodes the tracks named after them with `_B42` [observed: the clips'
//!   tracks]. In first person it is drawn with the arms' depth, as the weapon in the hand
//!   (the hands drew through it before, owner's report 2026-11-06).
//! - **Cut short** (not Hardcore): attacking or aiming stops it, as do swimming, a
//!   change of view and, in third person, walking off (the body's legs are the clip's).
//! - **Hardcore** (`Config\B42Inject.ini` `Hardcore=1`, or the settings page): while
//!   busy (one plays, a reload, an attack, a ragdoll) an aid item is refused and given
//!   back (`nvf_play::script`), and the animation cannot be cut short.
//! - **Interact** (B42 Interact): an item with an interact folder coming into the
//!   player's inventory plays its first-person clip after the menu closes, when its
//!   `cnd.ini` holds (not in combat; a creature part from a dead creature's body near
//!   by, each body once, the body's model folder matching the clip's folder): skinning
//!   meat and parts off a corpse, the look a weapon gets the first time it is picked up
//!   (`once`, for this run), an instrument. A recipe made at a workbench or a campfire
//!   with a crafting folder (`awr\edid`) plays its clip once the recipe menu closes.
//! - **Co-op.** Each animation this player plays goes to the others
//!   (`nvf_coop::net::AnimCue`, through `coop.rs`), and every aid item used too: their
//!   games play it on this player's puppet, on the upper body (it walks on), with the
//!   item's third-person clip when one is there (of the same name as the one played),
//!   else the game's idle for its kind; its sounds where the puppet is.
//!
//! - **B42 Loot** (`fnv_actor::b42::LootSet`): an item picked up from the world (E on
//!   it, `nvf_play::activate::PickedUp`) that no consume or interact clip is for plays
//!   the mod's loot clip, holding the item's own model in the left hand where the mod's
//!   Coord files put it (by its model, else its type). The others see the third-person
//!   one. Not done: the item's pull to the hand from afar and the clavicle's nudge.
//! - **B42 Melee Bash** (`fnv_actor::b42::BashClips`): nvf-play's weapon bash
//!   (`nvf_play::bash`) asks for its clip ([`nvf_play::bash::BashCue`]); one of the mod's
//!   for the view and the gun's type plays at once (none while another plays), its
//!   `B42BashHitFrameDetect` key strikes ([`nvf_play::bash::BashStrike`]) and its end, or
//!   its not playing, ends the bash ([`nvf_play::bash::BashOver`]). The others see the
//!   third-person one.
//!
//! - **The weapon drawn** is not seen while an item clip plays (not a bash or an
//!   activation's clip, made with it in hand), on the player and on puppets
//!   (`hide_weapons`).
//!
//! Log: `consume anim: …` per use and start; at exit `consume anims summary: …`.
//!
//! Not done: kNVSE's AnimGroupOverride folders (Idle Variety), a remembered "once" across
//! runs, the mods' `*_Sit` crouch, Hardcore closing the Pip-Boy during an animation.

use crate::idles::{ActorIdle, PlayerIdles};
use crate::limbs::ActorRig;
use crate::player_body::PlayerBody;
use crate::world::WorldData;
use bevy::camera::visibility::RenderLayers;
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_actor::b42::{self, Cue, Decision, Index, Options, Situation, Style, View};
use fnv_actor::idle_play::{IdleKeys, IdlePlay};
use fnv_game::idles::IdleSource;
use fnv_nif::anim::TransformTrack;
use fnv_sim::FormId;
use nvf_assets::actor::{bind_clip, ActorCtx, ActorParts};
use nvf_assets::anim_play::Clip;
use nvf_assets::model::ModelCtx;
use nvf_coop::net::AnimCue;
use nvf_play::sfx::{PlaySound, Sfx};
use nvf_play::PlayState;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const P: FormId = FormId::PLAYER_REF;

/// The folder of people's clips (the fallback idles are below it).
const PEOPLE: &str = "meshes\\characters\\_male\\";

/// A puppet's animation no longer than this [design: the smoking idle loops for long].
const PUPPET_MAX_SECONDS: f32 = 8.0;

/// Synthetic SOUN FormIDs for the clips' sound files (no load order uses them)
/// [engineering choice].
const FILE_SOUNDS: u32 = 0xFFB4_2000;

/// The same cue within this many seconds is one (the packs key a sound two ways at once)
/// [design].
const SAME_CUE: f32 = 0.2;

/// This player's animation, for the others (`coop.rs` sends it).
#[derive(Message, Clone, Debug)]
pub struct SendAnim(pub AnimCue);

/// Another player's animation, for its puppet (`coop.rs` reads the net's).
#[derive(Message, Clone, Debug)]
pub struct PeerAnim {
    /// The puppet's reference (`ActorRig::reference`).
    pub reference: u32,
    pub cue: AnimCue,
}

/// The settings (the settings page; their start from `Config\B42Inject.ini`).
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct ConsumeOptions(pub Options);

/// What to play once nothing is in the way.
#[derive(Debug, Clone)]
struct Pending {
    kind: u8,
    item: FormId,
    edid: String,
    /// Interact and crafting: the clips (first person only, unless `view` says).
    clips: Vec<String>,
    style: Style,
    /// The view `clips` are for (an activation's type clip); `None`: first person.
    view: Option<View>,
    /// The reference whose activation waits for the clip (`HeldActivation`).
    activate: Option<u32>,
    /// Where the held model sits under the hand (B42 Loot), instead of the clip's attach point.
    grip: Option<Transform>,
}

/// A model held in the hand while a clip plays.
struct Held {
    entities: Vec<Entity>,
    /// The attach point, moved by the clip's track for it.
    point: Option<(Entity, Arc<TransformTrack>)>,
    /// Nodes of the model moved by tracks: entity, transform above it, its rest, track.
    moved: Vec<(Entity, Affine3A, Affine3A, Arc<TransformTrack>)>,
}

/// An animation playing, on this player or a puppet.
struct Playing {
    item: FormId,
    view: View,
    serial: u32,
    clip: Arc<Clip>,
    /// The clip time read up to.
    at: Option<f32>,
    /// The model the next attach draws.
    model: Option<String>,
    held: Option<Held>,
    /// Cues heard lately (cue, seconds), for [`SAME_CUE`].
    recent: Vec<(Cue, f32)>,
    /// The item's sound is the clip's to play.
    keyed_sound: bool,
    age: f32,
    /// Where the player stood when it began (third person: walking off ends it).
    feet: [f32; 3],
    /// The reference whose activation waits for this clip, and the clip asked for it.
    activate: Option<u32>,
    fire: bool,
    /// It gave its activation back (its menu may be up now).
    gave: bool,
    grip: Option<Transform>,
    /// A weapon bash's clip, and its blow's key passed this step.
    bash: bool,
    strike: bool,
}

#[derive(Default, Debug, Clone, Copy)]
struct Stats {
    used: u32,
    played: u32,
    fallback: u32,
    skipped: u32,
    refused: u32,
    interact: u32,
    crafted: u32,
    sent: u32,
    puppets: u32,
    attached: u32,
    /// Activations played before (B42 Interact's type clips), and let go without one.
    typed: u32,
    untyped: u32,
}

#[derive(Resource, Default)]
pub struct ConsumeAnims {
    index: Option<Arc<Index>>,
    pending: Vec<Pending>,
    own: Option<Playing>,
    puppets: HashMap<u32, Playing>,
    file_sounds: HashMap<String, FormId>,
    rng: u64,
    /// Interact: items' counts as last seen, the items had (for `once`), the bodies used.
    counts: HashMap<FormId, i32>,
    interact_items: Vec<(FormId, String)>,
    had: HashSet<String>,
    bodies: HashSet<u32>,
    /// B42 Interact: the form types animated (Settings.ini), the EditorIDs left out
    /// (Blacklist.ini), the KEYWORDS lists' keywords by EditorID.
    types_on: Vec<u32>,
    blacklist: HashSet<String>,
    keywords: HashMap<String, Vec<String>>,
    /// B42 Loot as installed.
    loot: Arc<b42::LootSet>,
    /// B42 Melee Bash's clips as installed.
    bash: b42::BashClips,
    /// The drawn weapons hidden while an item clip plays, with their visibility before.
    hidden: HashMap<Entity, Visibility>,
    stats: Stats,
}

impl ConsumeAnims {
    fn roll(&mut self) -> f32 {
        // SplitMix64 [engineering choice].
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    }

    fn file_sound(&mut self, sfx: &mut Sfx, path: &str) -> FormId {
        let key = fnv_vfs::normalize(path);
        if let Some(&f) = self.file_sounds.get(&key) {
            return f;
        }
        let f = FormId(FILE_SOUNDS + self.file_sounds.len() as u32);
        let sound = fnv_combat::fx::Sound { edid: Some(format!("B42 {key}")), file: Some(key.clone()), min_distance: 100.0, max_distance: 2000.0, ..Default::default() };
        sfx.forms.sounds.insert(f, sound);
        self.file_sounds.insert(key, f);
        f
    }
}

pub struct ConsumeAnimsPlugin;

impl Plugin for ConsumeAnimsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConsumeAnims>()
            .init_resource::<ConsumeOptions>()
            .add_message::<SendAnim>()
            .add_message::<PeerAnim>()
            .add_systems(Update, (build, held_activation, own, crafted, interact, loot, bash, puppets, hide_weapons).chain().after(nvf_play::PlaySet::Scripts).before(crate::idles::drive))
            .add_systems(Last, report);
        register_settings(app);
    }
}

fn register_settings(app: &mut App) {
    use nvf_play::settings::{add_setting, Setting};
    add_setting(
        app,
        Setting {
            key: "consume-anims",
            group: nvf_play::pause::GROUP_MENUS,
            label: "Item animations",
            note: "On: eating, drinking, chems, stimpaks and smoking play the B42 Inject animations installed, and B42 Interact's skinning, weapon and crafting ones; other co-op players see them. Off: items are used at once, as in the original.",
            switches: "",
            get: |w| w.resource::<ConsumeOptions>().0.on,
            set: |w, on| w.resource_mut::<ConsumeOptions>().0.on = on,
            slider: None,
        },
    );
    add_setting(
        app,
        Setting {
            key: "consume-anims-hardcore",
            group: nvf_play::pause::GROUP_MENUS,
            label: "Item animations: Hardcore",
            note: "On: an aid item cannot be used while you reload, attack, lie ragdolled or already use one, and an animation cannot be cut short. (B42Inject.ini Hardcore)",
            switches: "",
            get: |w| w.resource::<ConsumeOptions>().0.hardcore,
            set: |w, on| w.resource_mut::<ConsumeOptions>().0.hardcore = on,
            slider: None,
        },
    );
    add_setting(
        app,
        Setting {
            key: "consume-anims-power-armor",
            group: nvf_play::pause::GROUP_MENUS,
            label: "Item animations in power armour",
            note: "On: the animations play in power armour too. (B42Inject.ini InjectInPA)",
            switches: "",
            get: |w| w.resource::<ConsumeOptions>().0.in_power_armor,
            set: |w, on| w.resource_mut::<ConsumeOptions>().0.in_power_armor = on,
            slider: None,
        },
    );
}

/// Read the folders once (and the mod's ini for the options' start).
fn build(mut st: ResMut<ConsumeAnims>, mut opts: ResMut<ConsumeOptions>, models: ModelCtx, script: Option<Res<nvf_play::script::ScriptState>>, mut held: ResMut<nvf_play::ConsumeAnims>, mut weapon_bash: ResMut<nvf_play::bash::WeaponBash>) {
    if st.index.is_some() {
        return;
    }
    let Some(rt) = script.as_deref().and_then(|s| s.rt.as_ref()) else { return };
    let vfs = &models.data.vfs;
    let t = std::time::Instant::now();
    let paths = vfs.paths(|p| p.starts_with(b42::INJECT_ROOT) || p.starts_with(b42::INTERACT_ROOT));
    let ix = Index::build(paths.iter().map(String::as_str), |p| vfs.read(p).and_then(Result::ok).map(|b| String::from_utf8_lossy(&b).into_owned()));
    if let Some(text) = vfs.read("config\\b42inject.ini").and_then(Result::ok) {
        b42::read_ini(&String::from_utf8_lossy(&text), &mut opts.0);
        info!("consume anim: Config\\B42Inject.ini: hardcore {}, in power armour {}", opts.0.hardcore, opts.0.in_power_armor);
    }
    let (codes, typed) = ix.type_counts();
    let off: Vec<u32> = vfs.read("config\\b42interact\\settings.ini").and_then(Result::ok).map(|t| b42::disabled_types(&String::from_utf8_lossy(&t))).unwrap_or_default();
    st.types_on = codes.into_iter().filter(|c| b42::TYPES.contains(c) && !off.contains(c)).collect();
    st.blacklist = vfs.read("config\\b42interact\\blacklist.ini").and_then(Result::ok).map(|t| b42::blacklist(&String::from_utf8_lossy(&t)).into_iter().collect()).unwrap_or_default();
    for path in vfs.paths(|p| p.starts_with("keywords\\") && p.ends_with(".ini")) {
        if let Some(t) = vfs.read(&path).and_then(Result::ok) {
            for (edid, kw) in b42::keywords(&String::from_utf8_lossy(&t)) {
                st.keywords.entry(edid).or_default().push(kw);
            }
        }
    }
    if typed > 0 {
        info!("consume anim: {typed} activation clips for form types {:?} (off: {off:?}), {} left out, keywords for {} EditorIDs", st.types_on, st.blacklist.len(), st.keywords.len());
    }
    let data = rt.game.data;
    let items: HashSet<u32> = ix.inject_items().filter_map(|e| data.by_edid(e)).map(|f| f.0).collect();
    held.items = items.clone();
    st.interact_items = ix.interact_items().filter_map(|e| data.by_edid(e).map(|f| (f, e.to_string()))).collect();
    let (inject, interact, crafting) = ix.counts();
    info!(
        "consume anim: {inject} consume clips for {} items, {interact} interact clips for {} items, {crafting} crafting clips ({} files looked at, {:.0} ms)",
        items.len(),
        st.interact_items.len(),
        paths.len(),
        t.elapsed().as_secs_f64() * 1000.0
    );
    let loot_paths = vfs.paths(|p| p.starts_with(b42::LOOT_ROOT));
    let read = |p: &str| vfs.read(p).and_then(Result::ok).map(|b| String::from_utf8_lossy(&b).into_owned());
    let obj: Vec<String> = vfs.paths(|p| p.starts_with("config\\b42loot\\coord\\obj\\") && p.ends_with(".ini")).iter().filter_map(|p| read(p)).collect();
    let loot = b42::LootSet::build(loot_paths.iter().map(String::as_str), &obj, read("config\\b42loot\\coord\\types.ini").as_deref(), read("config\\b42loot\\settings.ini").as_deref());
    if !loot.is_empty() {
        let (clips, models, types) = loot.counts();
        info!("consume anim: B42 Loot: {clips} clips, grips for {models} models and {types} types");
    }
    st.loot = Arc::new(loot);
    let bash = b42::BashClips::build(vfs.paths(|p| p.starts_with(b42::BASH_ROOT)).iter().map(String::as_str));
    if !bash.is_empty() {
        let (first, third) = bash.counts();
        info!("consume anim: B42 Melee Bash: {first} first-person and {third} third-person bash clips{}", if bash.fail.is_some() { ", the fail clip" } else { "" });
    }
    weapon_bash.clips = !bash.is_empty();
    st.bash = bash;
    st.rng = 0x0B42_1A2B_3C4D;
    st.index = Some(Arc::new(ix));
}

/// What stops an animation now (the mod's filter).
#[derive(bevy::ecs::system::SystemParam)]
struct Now<'w> {
    state: Res<'w, State<PlayState>>,
    pip: Option<Res<'w, nvf_play::pipboy::PipBoy>>,
    recipes: Option<Res<'w, nvf_play::crafting::RecipeMenu>>,
    phys: Option<Res<'w, nvf_play::phys::PhysState>>,
    knock: Option<Res<'w, crate::player_knockdown::PlayerKnockdown>>,
    control: Option<Res<'w, nvf_play::control::PlayerControl>>,
    player: Option<Res<'w, nvf_play::player::Player>>,
}

impl Now<'_> {
    fn in_menu(&self) -> bool {
        *self.state.get() != PlayState::Playing || self.pip.as_deref().is_some_and(|p| p.open() || p.on_arm()) || self.recipes.as_deref().is_some_and(|r| r.is_open())
    }

    fn swimming(&self) -> bool {
        self.phys.as_deref().and_then(|p| p.player.as_ref()).is_some_and(|c| c.swimming)
    }

    fn third(&self) -> bool {
        self.control.as_deref().is_some_and(|c| c.third_person_view())
    }

    fn feet(&self) -> [f32; 3] {
        self.player.as_deref().map_or([0.0; 3], |p| p.feet)
    }
}

fn situation(now: &Now, game: &fnv_game::game::Game, sim: &nvf_play::sim::SimState, body: &PlayerBody, pi: &PlayerIdles, st: &ConsumeAnims) -> Situation {
    use fnv_game::anim_state::AnimAction as A;
    let action = game.anim_state(P).and_then(|a| a.action);
    let control = now.control.as_deref();
    Situation {
        swimming: now.swimming(),
        ragdolled: now.knock.as_deref().is_some_and(|k| k.ragdolled()),
        reloading: control.is_some_and(|c| c.reload) || matches!(action, Some(A::Reload | A::ReloadOpen)),
        attacking: control.is_some_and(|c| c.trigger) || matches!(action, Some(A::Attack | A::FollowThrough | A::AttackReturn | A::ThrowWindUp | A::Thrown)),
        busy: st.own.is_some() || pi.first.playing().is_some() || pi.third.playing().is_some(),
        in_menu: now.in_menu(),
        queued: !st.pending.is_empty(),
        power_armor: body.worn().iter().any(|&f| sim.world.forms.item(f).is_some_and(|i| i.power_armor)),
    }
}

/// Activations held for B42 Interact's type clips (`nvf_play::activate::HeldActivation`):
/// the records held for, and the clip of the one just held (or it goes ahead at once).
#[allow(clippy::too_many_arguments)]
fn held_activation(
    mut st: ResMut<ConsumeAnims>,
    opts: Res<ConsumeOptions>,
    (mut hold, mut next): (ResMut<nvf_play::activate::HeldActivation>, ResMut<nvf_play::activate::ActivateNext>),
    mut script: Option<ResMut<nvf_play::script::ScriptState>>,
    sim: Res<nvf_play::sim::SimState>,
    now: Now,
    body: Res<PlayerBody>,
) {
    let st = &mut *st;
    let Some(ix) = st.index.clone() else { return };
    // The records: a type's, while the animations are on.
    let want: HashSet<[u8; 4]> = if opts.0.on {
        st.types_on
            .iter()
            .flat_map(|&c| match c {
                b42::ACTIVATOR => vec![*b"ACTI"],
                b42::TERMINAL => vec![*b"TERM"],
                b42::CONTAINER => vec![*b"CONT"],
                b42::DOOR => vec![*b"DOOR"],
                b42::PERSON => vec![*b"NPC_", *b"LVLN"],
                b42::CREATURE => vec![*b"CREA", *b"LVLC"],
                _ => vec![],
            })
            .collect()
    } else {
        HashSet::new()
    };
    if hold.sigs != want {
        hold.sigs = want;
    }
    if hold.sigs.is_empty() {
        hold.release(&mut next);
        return;
    }
    let Some(h) = hold.held.as_mut().filter(|h| h.fresh) else {
        // Held, but no clip waits on it (it was dropped): let it go.
        let waiting = st.own.as_ref().is_some_and(|p| p.activate.is_some()) || st.pending.iter().any(|p| p.activate.is_some());
        if hold.held.is_some() && !waiting {
            hold.release(&mut next);
        }
        return;
    };
    h.fresh = false;
    let (reference, sig, lock) = (h.reference, h.sig, h.lock);
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else {
        hold.release(&mut next);
        return;
    };
    let game = &mut rt.game;
    let data = game.data;
    let base = sim.base_of(reference).unwrap_or(FormId(reference));
    let edid = data.edid_of.get(&base).cloned().unwrap_or_default();
    let name = sim.forms.get(base).and_then(|b| b.name.clone()).unwrap_or_default();
    let code = match &sig {
        b"ACTI" => b42::ACTIVATOR,
        b"TERM" => b42::TERMINAL,
        b"CONT" => b42::CONTAINER,
        b"DOOR" => b42::DOOR,
        b"NPC_" | b"LVLN" => b42::PERSON,
        _ => b42::CREATURE,
    };
    let actor = matches!(code, b42::PERSON | b42::CREATURE);
    // Another animation plays (or waits): no clip [observed: the script's checks].
    let busy = st.own.is_some() || !st.pending.is_empty() || now.in_menu();
    let category = if actor {
        // The dead are looted; a person is also when the player sneaks (pickpocketing).
        let sneaking = now.control.as_deref().is_some_and(|c| c.sneak);
        if game.is_dead(FormId(reference)) || (code == b42::PERSON && sneaking) {
            "lootable".to_string()
        } else {
            "default".to_string()
        }
    } else {
        let kw = st.keywords.get(&edid.to_ascii_lowercase()).and_then(|ks| ks.iter().find(|k| ix.has_category(code, k)).cloned());
        kw.unwrap_or_else(|| {
            let c = if code == b42::ACTIVATOR { b42::activator_category(&edid, &name) } else { "default" };
            if ix.has_category(code, c) { c.to_string() } else { "default".to_string() }
        })
    };
    let third = now.third() || !body.first_person_drawn();
    let view = if third { View::Third } else { View::First };
    let drawn = now.control.as_deref().is_some_and(|c| c.weapon_out);
    let anim_type = drawn.then(|| game.weapon_equipped(P).and_then(|w| data.weapon_anim_types.get(&w).copied()).unwrap_or(0));
    let stance = b42::stance(anim_type);
    let left_out = st.blacklist.contains(&edid.to_ascii_lowercase()) || name.is_empty();
    let clips = if busy || left_out { &[][..] } else { ix.type_clips(code, &category, lock, view, stance) };
    if clips.is_empty() {
        st.stats.untyped += 1;
        debug!("consume anim: {edid} ({code}\\{category}, {lock:?}, {view:?}, {stance}): no clip{}", if busy { ", another plays" } else if left_out { ", left out" } else { "" });
        hold.release(&mut next);
        return;
    }
    st.stats.typed += 1;
    info!("consume anim: {edid} activated: type {code}\\{category}, lock {lock:?}, {view:?} person, stance {stance}: {} clips", clips.len());
    st.pending.insert(0, Pending { kind: AnimCue::INTERACT, item: base, edid, clips: clips.to_vec(), style: Style::Handle, view: Some(view), activate: Some(reference), grip: None });
}

/// This player's uses and the animation playing.
#[allow(clippy::too_many_arguments)]
fn own(
    mut commands: Commands,
    time: Res<Time>,
    mut st: ResMut<ConsumeAnims>,
    opts: Res<ConsumeOptions>,
    (mut held, mut used, mut contest, mut contest_stop): (ResMut<nvf_play::ConsumeAnims>, MessageReader<nvf_play::ItemUsed>, MessageReader<nvf_play::drink_world::ContestDrink>, MessageReader<nvf_play::drink_world::ContestDrinkStop>),
    (script, sim, body, mut pi): (Option<Res<nvf_play::script::ScriptState>>, Res<nvf_play::sim::SimState>, Res<PlayerBody>, ResMut<PlayerIdles>),
    now: Now,
    (mut parts, mut models, world, objects): (ResMut<ActorParts>, ModelCtx, Res<WorldData>, Res<crate::anim_objects::AnimObjects>),
    (mut sfx, mut sounds, mut send): (Option<ResMut<Sfx>>, MessageWriter<PlaySound>, MessageWriter<SendAnim>),
    mut transforms: Query<&mut Transform>,
    (mut hold, mut next): (ResMut<nvf_play::activate::HeldActivation>, ResMut<nvf_play::activate::ActivateNext>),
    (mut bash_strike, mut bash_over): (MessageWriter<nvf_play::bash::BashStrike>, MessageWriter<nvf_play::bash::BashOver>),
) {
    let st = &mut *st;
    let o = opts.0;
    // E pressed again on the held object: it was activated; its clip ends.
    if std::mem::take(&mut hold.skipped) {
        st.pending.retain(|p| p.activate.is_none());
        if let Some(pl) = st.own.as_mut().filter(|pl| pl.activate.is_some()) {
            pl.activate = None;
            let slot = if pl.view == View::First { &mut pi.first } else { &mut pi.third };
            if let Some(c) = slot.cur.as_mut().filter(|c| c.serial == pl.serial) {
                c.play.stop();
            }
        }
    }
    // A drinking contest's winner: its drink ends now, for its dance.
    let contest_over = contest_stop.read().count() > 0;
    let Some(ix) = st.index.clone() else {
        used.clear();
        contest.clear();
        return;
    };
    let Some(rt) = script.as_deref().and_then(|s| s.rt.as_ref()) else {
        used.clear();
        contest.clear();
        return;
    };
    let game = &rt.game;
    let data = game.data;
    let dt = time.delta_secs();
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    let s = situation(&now, game, &sim, &body, &pi, st);
    let consume_sound = |item: FormId| data.ingestibles.get(&item).and_then(|d| d.consume_sound);

    // Uses since the last frame (the refused were given back, `nvf_play::script`).
    let refused = std::mem::take(&mut held.refused);
    for u in used.read() {
        let item = FormId(u.item);
        let Some(def) = data.ingestibles.get(&item) else { continue };
        if refused.contains(&u.item) {
            st.stats.refused += 1;
            continue;
        }
        st.stats.used += 1;
        let edid = data.edid_of.get(&item).cloned().unwrap_or_default();
        let style = b42::style_of(&edid, def.equip_type);
        let has_clips = held.items.contains(&u.item);
        match b42::decide(&s, &o) {
            Decision::Play | Decision::Wait => {
                info!("consume anim: {edid} used{}", if s.in_menu { "; plays once the menu is closed" } else { "" });
                st.pending.push(Pending { kind: AnimCue::CONSUME, item, edid, clips: Vec::new(), style, view: None, activate: None, grip: None });
            }
            Decision::Skip(_) | Decision::Refuse if !has_clips => {}
            Decision::Skip(why) => {
                st.stats.skipped += 1;
                info!("consume anim: {edid}: no animation ({why})");
                if let Some(sound) = consume_sound(item) {
                    sounds.write(PlaySound { sound, at: None });
                }
            }
            Decision::Refuse => {}
        }
    }

    // A drinking contest's drink (`nvf_play::drink_world`): the item's drink plays as if
    // it were used, nothing used up.
    for d in contest.read() {
        let item = FormId(d.item);
        let Some(def) = data.ingestibles.get(&item) else { continue };
        let edid = data.edid_of.get(&item).cloned().unwrap_or_default();
        let style = b42::style_of(&edid, def.equip_type);
        info!("consume anim: {edid}: a drinking contest's drink");
        st.pending.push(Pending { kind: AnimCue::CONSUME, item, edid, clips: Vec::new(), style, view: None, activate: None, grip: None });
    }

    // Start what waits, once nothing is in the way.
    if st.own.is_none() && !now.in_menu() && !st.pending.is_empty() {
        let p = st.pending.remove(0);
        let s = situation(&now, game, &sim, &body, &pi, st);
        let blocked = s.swimming || s.ragdolled || s.reloading || s.attacking || (s.power_armor && !o.in_power_armor) || !o.on;
        let held_sound = held.items.contains(&p.item.0);
        let third = now.third() || !body.first_person_drawn();
        let view = if third { View::Third } else { View::First };
        let (clip_path, fallback) = if blocked {
            (None, false)
        } else if p.kind == AnimCue::CONSUME {
            let own = ix.inject(&p.edid, s.power_armor, view);
            let own = if own.is_empty() && s.power_armor { ix.inject(&p.edid, false, view) } else { own };
            let roll = st.roll();
            match b42::pick(own, roll) {
                Some(c) => (Some(c.clone()), false),
                None if third => (p.style.clips().iter().map(|c| format!("{PEOPLE}{c}")).find(|c| ctx.clip(c).is_some()), true),
                None => (None, false),
            }
        } else if p.view.map_or(view == View::First, |v| v == view) {
            let roll = st.roll();
            (b42::pick(&p.clips, roll).cloned(), false)
        } else {
            (None, false)
        };
        let clip = clip_path.as_deref().and_then(|c| ctx.clip(c));
        let mut keyed = false;
        match (&clip_path, clip) {
            (Some(path), Some(clip)) => {
                keyed = !fallback && clip.text_keys.iter().any(|(_, k)| b42::cues(k).contains(&Cue::ItemSound));
                let play = IdlePlay::new(IdleKeys::read(clip.start, clip.stop, &clip.text_keys), Some(1));
                let slot = if view == View::First { &mut pi.first } else { &mut pi.third };
                let serial = slot.start_clip(IdleSource::Script, clip.clone(), play);
                if fallback {
                    st.stats.fallback += 1;
                }
                st.stats.played += 1;
                match p.kind {
                    AnimCue::INTERACT => st.stats.interact += 1,
                    AnimCue::CRAFT => st.stats.crafted += 1,
                    _ => {}
                }
                info!("consume anim: {} plays {path} ({:?} person, {:.1} s)", p.edid, view, clip.length());
                let is_bash = path.starts_with(b42::BASH_ROOT);
                // A game idle in its place: the object the game holds with it.
                let held = if fallback && view == View::Third {
                    let equip_type = data.ingestibles.get(&p.item).map_or(0, |d| d.equip_type);
                    let rig = body.animators().first().map(|(_, a)| (a.skel.clone(), a.nodes.clone()));
                    rig.and_then(|(skel, nodes)| game_object(&mut commands, &mut ctx, &objects, &p.edid, equip_type, &skel, &nodes, Some(RenderLayers::layer(crate::sky::BODY_LAYER))))
                } else {
                    None
                };
                if held.is_some() {
                    st.stats.attached += 1;
                }
                st.own = Some(Playing { item: p.item, view, serial, clip, at: None, model: None, held, recent: Vec::new(), keyed_sound: keyed, age: 0.0, feet: now.feet(), activate: p.activate, fire: false, gave: false, grip: p.grip, bash: is_bash, strike: false });
            }
            (Some(path), None) => warn!("consume anim: {}: {path} does not load", p.edid),
            (None, _) => {
                if !blocked && p.kind == AnimCue::CONSUME {
                    debug!("consume anim: {}: no clip for the {view:?} person view", p.edid);
                }
            }
        }
        // An activation whose clip cannot play goes ahead now.
        if p.activate.is_some() && st.own.is_none() {
            hold.release(&mut next);
        }
        // A bash whose clip cannot play is over.
        if st.own.is_none() && p.clips.first().is_some_and(|c| c.starts_with(b42::BASH_ROOT)) {
            info!("consume anim: weapon bash: its clip does not play now{}", if blocked { " (busy)" } else { "" });
            bash_over.write(nvf_play::bash::BashOver);
        }
        // The item's sound now unless the clip plays it.
        if p.kind == AnimCue::CONSUME && held_sound && !keyed && (st.own.is_none() || fallback) {
            if let Some(sound) = consume_sound(p.item) {
                sounds.write(PlaySound { sound, at: None });
            }
        }
        // The others see it (every aid item, when it is not blocked).
        if !blocked {
            let style = if p.kind == AnimCue::CONSUME { p.style } else if p.kind == AnimCue::CRAFT { Style::Craft } else { p.style };
            send.write(SendAnim(AnimCue { kind: p.kind, style: style.code(), power_armor: s.power_armor, item: p.item.0, edid: p.edid.clone(), clip: clip_path.clone().filter(|_| !fallback).unwrap_or_default() }));
            st.stats.sent += 1;
        }
    }

    // The one playing: its keys, its model, its end.
    let Some(mut pl) = st.own.take() else {
        held.refuse = (o.hardcore && o.on && (s.reloading || s.attacking || s.ragdolled)).then_some("busy");
        return;
    };
    pl.age += dt;
    let slot = if pl.view == View::First { &mut pi.first } else { &mut pi.third };
    let entry = slot.cur.as_mut().filter(|c| c.serial == pl.serial && !c.play.ended());
    let third_now = now.third() || !body.first_person_drawn();
    let walked = pl.view == View::Third && {
        let f = now.feet();
        (f[0] - pl.feet[0]).hypot(f[1] - pl.feet[1]) > 24.0
    };
    let cut = !o.hardcore && now.control.as_deref().is_some_and(|c| c.trigger || c.aiming);
    let stop = now.swimming() || (third_now != (pl.view == View::Third)) || walked || cut || contest_over;
    let alive = match entry {
        Some(c) if stop => {
            c.play.stop();
            info!("consume anim: cut short ({})", if contest_over { "the drinking contest is won" } else if cut { "attack or aim" } else if walked { "walked off" } else { "swimming or the view changed" });
            false
        }
        Some(c) => {
            let t = pl.clip.start + c.play.time();
            let rig = body.animators().get(if pl.view == View::First { 1 } else { 0 }).map(|(_, a)| (a.skel.clone(), a.nodes.clone()));
            let layer = RenderLayers::layer(if pl.view == View::First { crate::sky::ARMS_LAYER } else { crate::sky::BODY_LAYER });
            let model_of = |item: FormId| fnv_world::placed::base_object(world.lo, world.schema, item.0).and_then(|b| held_model(&b));
            let item_sound = consume_sound(pl.item);
            let fx = Fx { sfx: sfx.as_deref_mut(), sounds: &mut sounds, at: None, item_sound: item_sound.filter(|_| pl.keyed_sound) };
            let model = model_of(pl.item);
            step(&mut commands, st, &mut pl, t, rig.as_ref().map(|(s, n)| (&**s, n.as_slice())), Some(layer), &mut ctx, model, fx, &mut transforms);
            true
        }
        None => false,
    };
    // The clip says activate now (`*_ACT`), or it ended first.
    if (pl.fire || !alive) && pl.activate.take().is_some() {
        info!("consume anim: {} activated{}", pl.item, if alive { " (its clip's key)" } else { " (its clip ended)" });
        hold.release(&mut next);
        pl.gave = true;
    }
    pl.fire = false;
    if std::mem::take(&mut pl.strike) && pl.bash {
        bash_strike.write(nvf_play::bash::BashStrike);
    }
    if alive {
        st.own = Some(pl);
    } else {
        if pl.bash {
            bash_over.write(nvf_play::bash::BashOver);
        }
        detach(&mut commands, &mut pl);
    }
    let busy = st.own.is_some() || s.reloading || s.attacking || s.ragdolled;
    held.refuse = (o.hardcore && o.on && busy).then_some(if st.own.is_some() { "an animation plays" } else { "busy" });
}

/// Where the cues' sounds go.
struct Fx<'a, 'b> {
    sfx: Option<&'a mut Sfx>,
    sounds: &'a mut MessageWriter<'b, PlaySound>,
    at: Option<[f32; 3]>,
    item_sound: Option<FormId>,
}

/// Read the keys `pl`'s clip passed up to `t`, act on them, and move the held model.
#[allow(clippy::too_many_arguments)]
fn step<F: bevy::ecs::query::QueryFilter>(
    commands: &mut Commands,
    st: &mut ConsumeAnims,
    pl: &mut Playing,
    t: f32,
    rig: Option<(&nvf_assets::actor::PreparedSkeleton, &[Entity])>,
    layer: Option<RenderLayers>,
    ctx: &mut ActorCtx,
    item_model: Option<String>,
    mut fx: Fx,
    transforms: &mut Query<&mut Transform, F>,
) {
    let keys: Vec<String> = fnv_actor::anim_sounds::passed(&pl.clip.text_keys, pl.clip.start, pl.at, t).map(str::to_string).collect();
    pl.at = Some(t);
    pl.recent.retain(|(_, at)| pl.age - at < SAME_CUE);
    for cue in keys.iter().flat_map(|k| b42::cues(k)) {
        if pl.recent.iter().any(|(c, _)| *c == cue) {
            continue;
        }
        pl.recent.push((cue.clone(), pl.age));
        match cue {
            Cue::Model(m) => pl.model = Some(format!("meshes\\{}", m.trim_start_matches(['\\', '/']))),
            Cue::Attach => {
                detach(commands, pl);
                let model = pl.model.clone().or_else(|| item_model.clone());
                if let (Some(model), Some((skel, nodes))) = (model, rig) {
                    if let Some(h) = attach(commands, ctx, &pl.clip, t, skel, nodes, &model, layer.clone(), pl.grip) {
                        st.stats.attached += 1;
                        info!("consume anim: holds {model}");
                        pl.held = Some(h);
                    }
                }
            }
            Cue::Detach => detach(commands, pl),
            Cue::SoundFile(path) => {
                if let Some(sfx) = fx.sfx.as_deref_mut() {
                    let sound = st.file_sound(sfx, &path);
                    fx.sounds.write(PlaySound { sound, at: fx.at });
                }
            }
            Cue::ItemSound => {
                if let Some(sound) = fx.item_sound.take() {
                    fx.sounds.write(PlaySound { sound, at: fx.at });
                }
            }
            Cue::Activate => pl.fire = true,
            Cue::Strike => pl.strike = true,
            Cue::Controls(_) => {}
        }
    }
    if let Some(h) = &pl.held {
        if let Some((e, track)) = &h.point {
            if let Ok(mut tr) = transforms.get_mut(*e) {
                *tr = sampled(track, t, Affine3A::IDENTITY, Affine3A::IDENTITY);
            }
        }
        for (e, above, rest, track) in &h.moved {
            if let Ok(mut tr) = transforms.get_mut(*e) {
                *tr = sampled(track, t, *above, *rest);
            }
        }
    }
}

/// A node's transform with `track` sampled at `t` (its rest for channels it lacks),
/// under `above`.
fn sampled(track: &TransformTrack, t: f32, above: Affine3A, rest: Affine3A) -> Transform {
    let (s0, r0, t0) = rest.to_scale_rotation_translation();
    let tr = track.translation.sample(t).map_or(t0, Vec3::from_array);
    let r = track.rotation_at(t).map_or(r0, |[w, x, y, z]| Quat::from_xyzw(x, y, z, w).normalize());
    let s = track.scale.sample(t).map_or(s0.x, |[s]| s);
    Transform::from_matrix(Mat4::from(above * Affine3A::from_scale_rotation_translation(Vec3::splat(s), r, tr)))
}

fn track<'a>(clip: &'a Clip, name: &str) -> Option<&'a Arc<TransformTrack>> {
    clip.tracks.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, t)| t)
}

/// Draw `model` at the attach point under the rig's left hand (module notes).
#[allow(clippy::too_many_arguments)]
fn attach(commands: &mut Commands, ctx: &mut ActorCtx, clip: &Clip, t: f32, skel: &nvf_assets::actor::PreparedSkeleton, nodes: &[Entity], model: &str, layer: Option<RenderLayers>, grip: Option<Transform>) -> Option<Held> {
    let part = ctx.part(model).or_else(|| {
        warn!("consume anim: {model} does not load");
        None
    })?;
    let hand = *skel.by_name.get(b42::HAND_NODE)?;
    let hand = *nodes.get(hand)?;
    // B42 Loot's grip is a node of its own under the hand, not moved by the clip.
    let point_track = if grip.is_some() { None } else { track(clip, b42::ATTACH_NODE).cloned() };
    let at = grip.unwrap_or_else(|| point_track.as_ref().map_or(Transform::IDENTITY, |tr| sampled(tr, t, Affine3A::IDENTITY, Affine3A::IDENTITY)));
    let point = commands.spawn((at, Visibility::default())).id();
    commands.entity(hand).add_child(point);
    let mut entities = vec![point];
    let mut moved = Vec::new();
    let mut by_node: HashMap<String, Entity> = HashMap::new();
    let with_layer = |commands: &mut Commands, e: Entity| {
        if let Some(l) = &layer {
            commands.entity(e).insert(l.clone());
        }
    };
    // In the first-person view the held model takes the arms' material, as the weapon in
    // the hand does (`player_body::arms_material`): the arms' depth is squeezed in front
    // of the world, and with the model's own the hands drew through it.
    let first_person = layer.as_ref().is_some_and(|l| *l == RenderLayers::layer(crate::sky::ARMS_LAYER));
    // A skinned shape (a worn model, B42 Loot's `prosGO.nif` too) is drawn rigid: with no
    // skeleton under it, its joint attributes made every draw a GPU validation error
    // (`nvf_assets::actor::rigid_mesh`).
    let shapes: Vec<_> = part.shapes.iter().filter(|s| !s.cap).map(|s| (s, nvf_assets::actor::rigid_mesh(&s.cpu).map_or_else(|| s.mesh.clone(), |m| ctx.models.meshes.add(m)))).collect();
    let mut material = |m: &Handle<nvf_assets::nif_material::NifMaterial>| if first_person { crate::player_body::arms_material(&mut ctx.models.materials, m) } else { m.clone() };
    for (s, mesh) in shapes {
        // The outermost ancestor a track moves (`<name>_B42`).
        let found = s.ancestors.iter().enumerate().rev().find_map(|(k, (name, own))| {
            let name = name.as_deref()?;
            let tr = track(clip, &format!("{name}{}", b42::NODE_SUFFIX))?;
            Some((k, name.to_string(), *own, tr.clone()))
        });
        let Some((k, name, rest, tr)) = found else {
            let x = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material(&s.material)), Transform::from_matrix(Mat4::from(part.root * s.transform)))).id();
            with_layer(commands, x);
            commands.entity(point).add_child(x);
            entities.push(x);
            continue;
        };
        let node = match by_node.get(&name) {
            Some(&x) => x,
            None => {
                let above = s.ancestors[k + 1..].iter().rev().fold(part.root, |acc, (_, a)| acc * *a);
                let x = commands.spawn((sampled(&tr, t, above, rest), Visibility::default())).id();
                commands.entity(point).add_child(x);
                entities.push(x);
                moved.push((x, above, rest, tr));
                by_node.insert(name, x);
                x
            }
        };
        let below = s.ancestors[..k].iter().rev().fold(Affine3A::IDENTITY, |acc, (_, a)| acc * *a) * s.own;
        let x = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material(&s.material)), Transform::from_matrix(Mat4::from(below)))).id();
        with_layer(commands, x);
        commands.entity(node).add_child(x);
        entities.push(x);
    }
    Some(Held { entities, point: point_track.map(|tr| (point, tr)), moved })
}

/// The game's animated object for an aid item played with a game idle in place of a
/// mod's clip (`b42::anim_object`): its model under the bone its `Prn` names, rigid,
/// as `anim_objects.rs` draws the objects of an actor's idle. Log: `consume anim: holds
/// <model> on <bone> (<ANIO>)`.
#[allow(clippy::too_many_arguments)]
fn game_object(commands: &mut Commands, ctx: &mut ActorCtx, objects: &crate::anim_objects::AnimObjects, edid: &str, equip_type: i64, skel: &nvf_assets::actor::PreparedSkeleton, nodes: &[Entity], layer: Option<RenderLayers>) -> Option<Held> {
    let name = b42::anim_object(edid, equip_type)?;
    let Some(model) = objects.model_by_edid(name).map(str::to_string) else {
        warn!("consume anim: no animated object {name} for {edid}");
        return None;
    };
    let part = ctx.part(&model).or_else(|| {
        warn!("consume anim: {model} does not load");
        None
    })?;
    let bone = part.attach_bone().to_string();
    let Some(b) = skel.by_name.get(&bone).copied() else {
        warn!("consume anim: no {bone} for {model}");
        return None;
    };
    let at = *nodes.get(b)?;
    let point = commands.spawn((Transform::IDENTITY, Visibility::default())).id();
    commands.entity(at).add_child(point);
    for s in part.shapes.iter().filter(|s| !s.cap) {
        let local = nvf_assets::actor::rigid_local(skel, b, &part.root, &s.transform);
        let x = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(local)))).id();
        if let Some(l) = &layer {
            commands.entity(x).insert(l.clone());
        }
        commands.entity(point).add_child(x);
    }
    info!("consume anim: holds {model} on {bone} ({name})");
    Some(Held { entities: vec![point], point: None, moved: Vec::new() })
}

fn detach(commands: &mut Commands, pl: &mut Playing) {
    if let Some(h) = pl.held.take() {
        // The first is the attach point: its children go with it.
        if let Some(&root) = h.entities.first() {
            if let Ok(mut e) = commands.get_entity(root) {
                e.despawn();
            }
        }
    }
}

/// Recipes made in the recipe menu: their crafting clips play once it is closed.
fn crafted(mut st: ResMut<ConsumeAnims>, opts: Res<ConsumeOptions>, menu: Option<ResMut<nvf_play::crafting::RecipeMenu>>, script: Option<Res<nvf_play::script::ScriptState>>) {
    let (Some(mut menu), Some(rt)) = (menu, script.as_deref().and_then(|s| s.rt.as_ref())) else { return };
    let Some(open) = menu.0.as_mut() else { return };
    let made = std::mem::take(&mut open.made);
    let Some(ix) = st.index.clone().filter(|_| opts.0.on) else { return };
    for r in made {
        let Some(edid) = rt.game.data.recipes.get(&r).and_then(|d| d.edid.clone()) else { continue };
        let clips = ix.recipe(&edid);
        // One a menu visit.
        if clips.is_empty() || st.pending.iter().any(|p| p.kind == AnimCue::CRAFT) {
            continue;
        }
        info!("consume anim: recipe {edid} made; its animation plays once the menu is closed");
        st.pending.push(Pending { kind: AnimCue::CRAFT, item: r, edid, clips: clips.to_vec(), style: Style::Craft, view: None, activate: None, grip: None });
    }
}

/// Items with interact folders coming into the inventory (module notes).
#[allow(clippy::too_many_arguments)]
fn interact(
    mut st: ResMut<ConsumeAnims>,
    opts: Res<ConsumeOptions>,
    script: Option<Res<nvf_play::script::ScriptState>>,
    mut sim: ResMut<nvf_play::sim::SimState>,
    mut taken: MessageReader<nvf_play::crime::ItemsTaken>,
    rigs: Query<(&ActorRig, &GlobalTransform)>,
    player: Option<Res<nvf_play::player::Player>>,
    now: Now,
) {
    let st = &mut *st;
    let Some(ix) = st.index.clone() else { return };
    let Some(rt) = script.as_deref().and_then(|s| s.rt.as_ref()) else { return };
    let from: Vec<FormId> = taken.read().map(|t| t.container).collect();
    let counts: Vec<(FormId, String, i32)> = st.interact_items.iter().map(|(f, e)| (*f, e.clone(), sim.world.item_count(P, *f).unwrap_or(0))).collect();
    let first_look = st.counts.is_empty();
    let gained: Vec<(FormId, String)> = counts.iter().filter(|(f, _, n)| *n > st.counts.get(f).copied().unwrap_or(*n)).map(|(f, e, _)| (*f, e.clone())).collect();
    for (f, e, n) in &counts {
        st.counts.insert(*f, *n);
        if *n > 0 && first_look {
            st.had.insert(e.clone());
        }
    }
    // Many at once is a load or a script's gift, not a pick-up [design].
    if first_look || gained.len() > 3 || !opts.0.on {
        for (_, e) in gained {
            st.had.insert(e);
        }
        return;
    }
    let game = &rt.game;
    let feet = player.as_deref().map_or([0.0; 3], |p| p.feet);
    let in_combat = game.awareness.hunted(P);
    for (item, edid) in gained {
        let first = st.had.insert(edid.clone());
        // The body the item came from, when it is a dead creature drawn near by.
        let body = from.iter().find_map(|&c| {
            let (rig, gt) = rigs.iter().find(|(r, _)| r.reference == c.0)?;
            // A creature a script placed (PlaceAtMe, a spawn) has its base in the live
            // world only, not in the scripts' view of it.
            let kind = game.creature_type(c).or_else(|| game.data.template_source(sim.world.peek(c)?.base, c, |d| d.template_stats)?.1.creature_type)?;
            let folder = rig.skeleton.rsplit('\\').nth(1)?.to_string();
            let at = crate::world::bevy_to_game(gt.translation());
            let d = (at.x - feet[0]).hypot(at.y - feet[1]).hypot(at.z - feet[2]);
            game.is_dead(c).then_some((c, folder, kind as i32, d))
        });
        let source = b42::Source { creature: body.as_ref().map(|(_, f, k, d)| (f.clone(), *k, *d)), used: body.as_ref().is_some_and(|(c, ..)| st.bodies.contains(&c.0)) };
        let Some(entry) = ix.interact(&edid, &source, in_combat, first) else {
            let why = from.iter().map(|&c| {
                let rig = rigs.iter().find(|(r, _)| r.reference == c.0).map(|(r, _)| r.skeleton.clone());
                format!("{c}: rig {rig:?}, dead {}", game.is_dead(c))
            });
            info!("consume anim: {edid} picked up, no interact clip (combat {in_combat}, first {first}; from {})", why.collect::<Vec<_>>().join(", "));
            continue;
        };
        if let Some((c, ..)) = &body {
            if entry.cond.corpse.is_some() {
                st.bodies.insert(c.0);
            }
        }
        // A clip that already gave its activation back (a body's loot clip, its menu up
        // now) doesn't keep the pick-up's clip out: it plays after.
        if st.pending.iter().any(|p| p.kind == AnimCue::INTERACT && p.activate.is_none()) || st.own.as_ref().is_some_and(|o| !o.gave) {
            info!("consume anim: {edid} picked up while another interact clip plays or waits; none for it");
            continue;
        }
        let style = if entry.cond.corpse.is_some() { Style::Skin } else { Style::Handle };
        info!("consume anim: {edid} picked up{}; its animation plays{}", body.as_ref().map_or(String::new(), |(_, f, ..)| format!(" from a {f}")), if now.in_menu() { " once the menu is closed" } else { "" });
        st.pending.push(Pending { kind: AnimCue::INTERACT, item, edid, clips: entry.clips.clone(), style, view: None, activate: None, grip: None });
    }
}

/// The model an item is held by in the hand, and its key in B42 Loot's grips: for
/// armour its world (ground) model, MOD2, else MODL; for anything else MODL. The mod's
/// script takes `GetBipedModelPath 2` for armour (form type 24) and `GetModelPath`
/// otherwise [observed: `ln_B42Loot.txt`, "SNATCH MODEL PATH"; index 2 is the male world
/// model, `fnv_game::nvse`]. MODL is armour's worn model: picking up the Exposed
/// Prostitute Outfit put the whole outfit and body (`prosmale03.nif`) in the hand instead
/// of the folded clothes (`prosGO.nif`), and the grip lookup, keyed by world models, found
/// nothing for it.
fn held_model(base: &fnv_world::placed::BaseObject) -> Option<String> {
    if &base.sig == b"ARMO" {
        base.world_model_path()
    } else {
        base.model_path()
    }
}

/// Where B42 Loot puts `item` in the hand (module notes).
fn loot_grip(loot: &b42::LootSet, world: &WorldData, script: Option<&nvf_play::script::ScriptState>, item: FormId) -> Option<Transform> {
    let base = fnv_world::placed::base_object(world.lo, world.schema, item.0)?;
    let anim = (&base.sig == b"WEAP").then(|| script.and_then(|s| s.rt.as_ref()).and_then(|rt| rt.game.data.weapon_anim_types.get(&item).copied())).flatten();
    let code = b42::loot_type(&base.sig, anim, None, false);
    let g = loot.grip(held_model(&base).as_deref(), code.as_deref())?;
    let [rx, ry, rz] = g.turn.map(f32::to_radians);
    Some(Transform { translation: Vec3::from(g.at), rotation: Quat::from_euler(EulerRot::XYZ, rx, ry, rz), scale: Vec3::splat(g.scale) })
}

/// B42 Loot: an item picked up from the world plays the loot clip (module notes).
#[allow(clippy::too_many_arguments)]
fn loot(
    mut st: ResMut<ConsumeAnims>,
    opts: Res<ConsumeOptions>,
    mut picked: MessageReader<nvf_play::activate::PickedUp>,
    mut script: Option<ResMut<nvf_play::script::ScriptState>>,
    world: Res<WorldData>,
    body: Res<PlayerBody>,
    now: Now,
) {
    let st = &mut *st;
    for p in picked.read() {
        if !opts.0.on || st.loot.is_empty() {
            continue;
        }
        // Another clip has it (its own interact clip, one playing or waiting).
        if st.own.is_some() || !st.pending.is_empty() {
            debug!("consume anim: {} picked up while another clip plays or waits: no loot clip", p.item);
            continue;
        }
        let third = now.third() || !body.first_person_drawn();
        let view = if third { View::Third } else { View::First };
        let drawn = now.control.as_deref().is_some_and(|c| c.weapon_out);
        let game = script.as_deref_mut().and_then(|s| s.rt.as_mut()).map(|rt| &mut rt.game);
        let anim = drawn.then(|| game.and_then(|g| g.weapon_equipped(P).and_then(|w| g.data.weapon_anim_types.get(&w).copied())).unwrap_or(0));
        let Some(clip) = st.loot.clip(view, anim).map(str::to_string) else { continue };
        let grip = loot_grip(&st.loot, &world, script.as_deref(), p.item);
        let edid = fnv_world::placed::base_object(world.lo, world.schema, p.item.0).and_then(|b| b.edid).unwrap_or_else(|| p.item.to_string());
        info!("consume anim: {edid} picked up: B42 Loot {clip}{}", if grip.is_some() { "" } else { " (no grip for it: held as the clip has it)" });
        st.pending.push(Pending { kind: AnimCue::INTERACT, item: p.item, edid, clips: vec![clip], style: Style::Handle, view: Some(view), activate: None, grip });
    }
}

/// A weapon bash's clip (module notes): first in line, or the bash is over.
fn bash(mut st: ResMut<ConsumeAnims>, mut cues: MessageReader<nvf_play::bash::BashCue>, mut over: MessageWriter<nvf_play::bash::BashOver>, body: Res<PlayerBody>, mut pi: ResMut<PlayerIdles>, now: Now) {
    let st = &mut *st;
    for c in cues.read() {
        let third = now.third() || !body.first_person_drawn();
        let view = if third { View::Third } else { View::First };
        let clips: Vec<String> = if c.fail {
            st.bash.fail.iter().filter(|_| view == View::First).cloned().collect()
        } else {
            st.bash.clips(view, c.clip_type).to_vec()
        };
        // A clip that holds back no activation (an inspect, a pick-up, a use) gives way.
        let busy = st.own.as_ref().is_some_and(|o| o.bash || o.activate.is_some());
        if clips.is_empty() || busy {
            info!("consume anim: weapon bash: {}", if clips.is_empty() { format!("no clip for type {} in the {view:?} person view", c.clip_type) } else { "another clip plays".to_string() });
            over.write(nvf_play::bash::BashOver);
            continue;
        }
        if let Some(o) = st.own.as_ref() {
            let slot = if o.view == View::First { &mut pi.first } else { &mut pi.third };
            if let Some(cur) = slot.cur.as_mut().filter(|cur| cur.serial == o.serial) {
                cur.play.stop();
                info!("consume anim: {}'s clip gives way to a weapon bash", o.item);
            }
        }
        st.pending.insert(0, Pending { kind: AnimCue::INTERACT, item: FormId(0), edid: if c.fail { "weapon bash (no AP)".to_string() } else { "weapon bash".to_string() }, clips, style: Style::Handle, view: Some(view), activate: None, grip: None });
    }
}

/// Other players' animations on their puppets.
#[allow(clippy::too_many_arguments)]
fn puppets(
    mut commands: Commands,
    time: Res<Time>,
    mut st: ResMut<ConsumeAnims>,
    mut heard: MessageReader<PeerAnim>,
    mut rigs: Query<(Entity, &ActorRig, Option<&mut ActorIdle>, &GlobalTransform)>,
    (mut parts, mut models): (ResMut<ActorParts>, ModelCtx),
    (mut sfx, mut sounds): (Option<ResMut<Sfx>>, MessageWriter<PlaySound>),
    world: Res<WorldData>,
    script: Option<Res<nvf_play::script::ScriptState>>,
    mut transforms: Query<&mut Transform, Without<ActorRig>>,
    objects: Res<crate::anim_objects::AnimObjects>,
) {
    let st = &mut *st;
    let dt = time.delta_secs();
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    let data = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| rt.game.data);
    for PeerAnim { reference, cue } in heard.read() {
        let Some((e, rig, idle, _)) = rigs.iter_mut().find(|(_, r, ..)| r.reference == *reference) else {
            debug!("consume anim: no puppet {reference:08X} drawn for {}", cue.edid);
            continue;
        };
        let roll = st.roll();
        let own = st.index.as_ref().and_then(|ix| {
            if cue.kind == AnimCue::CONSUME {
                ix.third_for(&cue.edid, cue.power_armor, &cue.clip, roll).map(str::to_string)
            } else if cue.kind == AnimCue::GAME {
                // A game's own clip (a darts throw, `darts_world.rs`), played as named.
                Some(cue.clip.clone())
            } else if cue.clip.starts_with(b42::LOOT_ROOT) {
                st.loot.clip(View::Third, None).map(str::to_string)
            } else if cue.clip.starts_with(b42::BASH_ROOT) {
                b42::BashClips::type_of(&cue.clip).and_then(|t| b42::pick(st.bash.clips(View::Third, t), roll).cloned())
            } else {
                // An activation's type clip: its folder's third-person one.
                b42::pick(ix.type_third(&cue.clip), roll).cloned()
            }
        });
        let grip = cue.clip.starts_with(b42::LOOT_ROOT).then(|| loot_grip(&st.loot, &world, script.as_deref(), FormId(cue.item))).flatten();
        let style = Style::from_code(cue.style);
        let path = own.clone().filter(|p| ctx.clip(p).is_some()).or_else(|| style.clips().iter().map(|c| format!("{PEOPLE}{c}")).find(|c| ctx.clip(c).is_some()));
        let Some((path, clip)) = path.and_then(|p| Some((p.clone(), ctx.clip(&p)?))) else {
            warn!("consume anim: no clip for {}'s puppet ({:?})", cue.edid, style);
            continue;
        };
        let bound = crate::npc_stimpaks::upper_body(bind_clip(&rig.skel, clip.clone()), &rig.skel.names);
        let play = IdlePlay::new(IdleKeys::read(clip.start, clip.stop, &clip.text_keys), Some(1));
        let serial = match idle {
            Some(mut i) => i.0.start_clip(IdleSource::Stimpak, Arc::new(bound), play),
            None => {
                let mut i = ActorIdle::default();
                let serial = i.0.start_clip(IdleSource::Stimpak, Arc::new(bound), play);
                commands.entity(e).insert(i);
                serial
            }
        };
        if let Some(mut old) = st.puppets.remove(reference) {
            detach(&mut commands, &mut old);
        }
        // A game idle in place of the mod's clip: the object the game holds with it.
        let held = if own.as_deref() != Some(path.as_str()) && cue.kind == AnimCue::CONSUME {
            let equip_type = data.and_then(|d| d.ingestibles.get(&FormId(cue.item))).map_or(0, |d| d.equip_type);
            game_object(&mut commands, &mut ctx, &objects, &cue.edid, equip_type, &rig.skel, &rig.nodes, None)
        } else {
            None
        };
        let keyed = own.is_some() && clip.text_keys.iter().any(|(_, k)| b42::cues(k).contains(&Cue::ItemSound));
        // The item's sound where the puppet is, unless the clip keys it.
        if cue.kind == AnimCue::CONSUME && !keyed {
            if let Some(sound) = data.and_then(|d| d.ingestibles.get(&FormId(cue.item))).and_then(|d| d.consume_sound) {
                let at = rigs.get(e).ok().map(|(.., gt)| crate::world::bevy_to_game(gt.translation()).to_array());
                sounds.write(PlaySound { sound, at });
            }
        }
        st.stats.puppets += 1;
        info!("consume anim: puppet {reference:08X} plays {path} for {}", cue.edid);
        st.puppets.insert(*reference, Playing { item: FormId(cue.item), view: View::Third, serial, clip, at: None, model: None, held, recent: Vec::new(), keyed_sound: keyed, age: 0.0, feet: [0.0; 3], activate: None, fire: false, gave: false, grip, bash: false, strike: false });
    }
    let mut gone = Vec::new();
    for (reference, pl) in st.puppets.iter_mut() {
        pl.age += dt;
        let Some((_, rig, idle, gt)) = rigs.iter_mut().find(|(_, r, ..)| r.reference == *reference) else {
            gone.push(*reference);
            continue;
        };
        let Some(mut idle) = idle else {
            gone.push(*reference);
            continue;
        };
        let Some(c) = idle.0.cur.as_mut().filter(|c| c.serial == pl.serial && !c.play.ended()) else {
            gone.push(*reference);
            continue;
        };
        if pl.age > PUPPET_MAX_SECONDS {
            c.play.stop();
        }
        let t = pl.clip.start + c.play.time();
        let at = crate::world::bevy_to_game(gt.translation()).to_array();
        let item_sound = data.and_then(|d| d.ingestibles.get(&pl.item)).and_then(|d| d.consume_sound).filter(|_| pl.keyed_sound);
        let model = fnv_world::placed::base_object(world.lo, world.schema, pl.item.0).and_then(|b| held_model(&b));
        let (skel, nodes) = (rig.skel.clone(), rig.nodes.clone());
        let fx = Fx { sfx: sfx.as_deref_mut(), sounds: &mut sounds, at: Some(at), item_sound };
        // `step` takes the whole state for the file sounds: lend it the parts it uses.
        let mut lent = ConsumeAnims { file_sounds: std::mem::take(&mut st.file_sounds), ..Default::default() };
        step(&mut commands, &mut lent, pl, t, Some((&skel, &nodes)), None, &mut ctx, model, fx, &mut transforms);
        st.file_sounds = lent.file_sounds;
        st.stats.attached += lent.stats.attached;
    }
    for r in gone {
        if let Some(mut pl) = st.puppets.remove(&r) {
            detach(&mut commands, &mut pl);
        }
    }
}

/// While an item clip plays (not a weapon bash nor an activation's clip, which are made
/// with the weapon in hand), the weapon drawn is not seen: B42 Inject and Interact put it
/// away for the clip [observed: the mods' clips leave the right hand empty or holding
/// the item; owner's report 2026-11-06, the machete and a rifle drawn through a stimpak
/// and an inspect clip]. The player's (both rigs) and the puppets' with a clip; back
/// as they were once it ends.
fn hide_weapons(
    mut st: ResMut<ConsumeAnims>,
    body: Res<PlayerBody>,
    control: Option<Res<nvf_play::control::PlayerControl>>,
    rigs: Query<(&ActorRig, Option<&crate::weapons::WeaponHold>)>,
    held: Query<(Entity, &crate::weapons::HeldWeapon)>,
    mut visibility: Query<&mut Visibility>,
) {
    let st = &mut *st;
    let item_clip = |pl: &Playing| pl.activate.is_none() && !pl.bash;
    let mut want: Vec<Entity> = Vec::new();
    if st.own.as_ref().is_some_and(item_clip) && control.as_deref().is_some_and(|c| c.weapon_out) {
        want.extend(body.weapon_holders());
    }
    for (reference, _) in st.puppets.iter().filter(|(_, pl)| item_clip(pl)) {
        let drawn = rigs.iter().any(|(r, hold)| r.reference == *reference && hold.is_some_and(|h| h.in_hand()));
        if drawn {
            want.extend(held.iter().filter(|(_, h)| h.reference == *reference).map(|(e, _)| e));
        }
    }
    // Back as they were: those no longer to hide.
    let back: Vec<Entity> = st.hidden.keys().filter(|e| !want.contains(e)).copied().collect();
    for e in back {
        if let (Some(v), Ok(mut now)) = (st.hidden.remove(&e), visibility.get_mut(e)) {
            *now = v;
        }
    }
    for e in want {
        let Ok(mut v) = visibility.get_mut(e) else { continue };
        if *v != Visibility::Hidden {
            if let std::collections::hash_map::Entry::Vacant(slot) = st.hidden.entry(e) {
                slot.insert(*v);
                debug!("consume anim: the weapon drawn ({e:?}) hidden for the clip");
            }
            *v = Visibility::Hidden;
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, st: Res<ConsumeAnims>) {
    if exit.read().last().is_none() {
        return;
    }
    let s = st.stats;
    info!(
        "consume anims summary: {} aid items used, {} animations played ({} the game's idles, {} interact, {} crafting, {} before an activation), {} activations without one, {} not animated, {} refused (Hardcore), {} models held, {} sent to co-op, {} played on puppets",
        s.used, s.played, s.fallback, s.interact, s.crafted, s.typed, s.untyped, s.skipped, s.refused, s.attached, s.sent, s.puppets
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_world::placed::BaseObject;

    fn base(sig: &[u8; 4], model: Option<&str>, world_model: Option<&str>) -> BaseObject {
        BaseObject { form_id: 0x0012_0855, sig: *sig, edid: None, model: model.map(Into::into), world_model: world_model.map(Into::into), world_alt_textures: Vec::new(), worn_only: false, flags: 0, bounds: None }
    }

    #[test]
    fn armour_is_held_by_its_world_model_not_the_worn_body() {
        // The Exposed Prostitute Outfit (FalloutNV.esm 0x00120855) as stored: MODL the
        // worn male body, MOD2 the folded clothes.
        let outfit = base(b"ARMO", Some(r"Armor\Prostitutes\prosmale03.NIF"), Some(r"Armor\Prostitutes\prosGO.NIF"));
        assert_eq!(held_model(&outfit).as_deref(), Some(r"meshes\armor\prostitutes\prosgo.nif"));
    }

    #[test]
    fn armour_with_no_world_model_is_held_by_modl() {
        let a = base(b"ARMO", Some(r"Armor\Hat\hat.NIF"), None);
        assert_eq!(held_model(&a).as_deref(), Some(r"meshes\armor\hat\hat.nif"));
    }

    #[test]
    fn a_weapon_is_held_by_its_modl_not_its_world_model() {
        // The mod takes GetModelPath for anything not armour: a weapon's MODL is its
        // model in the hand, MOD4 the one lying on the ground.
        let w = base(b"WEAP", Some(r"Weapons\1HandPistol\9mmPistol.NIF"), Some(r"Weapons\1HandPistol\9mmPistolGO.NIF"));
        assert_eq!(held_model(&w).as_deref(), Some(r"meshes\weapons\1handpistol\9mmpistol.nif"));
        let food = base(b"ALCH", Some(r"Clutter\Food\Apple.NIF"), None);
        assert_eq!(held_model(&food).as_deref(), Some(r"meshes\clutter\food\apple.nif"));
    }
}
