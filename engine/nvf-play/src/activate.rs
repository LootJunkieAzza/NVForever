//! Part A (docs/design/layer4-play.md §4): crosshair, prompt, activation and keeping
//! render entities in line with the simulation.
//!
//! - `PlaySet::Activation` (while playing): find the target under the crosshair
//!   ([`pick`]: the collision ray, else references' render bounds), then E activates it
//!   through `fnv_sim::World::activate` and its outcome becomes messages
//!   ([`outcome`]): `RefChanged`, `Travel`, `OpenContainer` or an on-screen line.
//! - `PlaySet::Ui`: the crosshair, the prompt and the message line (hidden in menus).
//! - `PlaySet::Sync`: `RefVisual` entities are hidden while the simulation says their
//!   reference is not in the world, on `RefChanged` and when cells stream in.
//!
//! Test aid: `NVF_PLAY_TEST_ACTIVATE=<seconds>[,<seconds>...]` presses E at those times
//! after start (unattended in-game checks).

pub mod outcome;
pub mod pick;
#[cfg(test)]
mod tests;

use crate::phys::PhysState;
use crate::player::Player;
use crate::sim::SimState;
use crate::{space, OpenContainer, PlaySet, PlayState, PlayWorld, RefChanged, RefVisual, Travel};
use bevy::camera::primitives::Aabb;
use bevy::prelude::*;
use fnv_phys::{layers, Filter, Owner};
use fnv_sim::forms::BaseData;
use fnv_sim::world::Outcome;
use fnv_sim::FormId;
use outcome::{Action, Effect};
use pick::{Pick, RayHit, Source, REACH};
use std::collections::HashMap;

/// Seconds a message stays fully visible, then seconds it takes to fade.
const MESSAGE_HOLD: f32 = 2.0;
const MESSAGE_FADE: f32 = 0.5;

pub struct ActivatePlugin;

impl Plugin for ActivatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Target>()
            .init_resource::<HudMessage>()
            .insert_resource(AutoActivate::from_env())
            .add_systems(Startup, spawn_hud)
            .add_systems(Update, (find_target, activate).chain().in_set(PlaySet::Activation).run_if(in_state(PlayState::Playing)))
            .add_systems(Update, update_hud.in_set(PlaySet::Ui))
            .add_systems(Update, sync_visuals.in_set(PlaySet::Sync));
    }
}

/// The reference under the crosshair, if it can be activated.
#[derive(Resource, Default, Debug)]
pub struct Target(pub Option<Targeted>);

#[derive(Debug, Clone, PartialEq)]
pub struct Targeted {
    pub form_id: u32,
    pub name: String,
    pub action: Action,
    pub distance: f32,
    pub source: Source,
}

/// The last on-screen message and how long it has been shown.
#[derive(Resource, Default)]
struct HudMessage {
    text: String,
    age: f32,
}

impl HudMessage {
    fn show(&mut self, text: String) {
        self.text = text;
        self.age = 0.0;
    }
}

#[derive(Component)]
struct Hud;

#[derive(Component)]
struct PromptText;

#[derive(Component)]
struct MessageText;

// ---- targeting -------------------------------------------------------------------

/// Name and action of a reference the player can activate now: present in the
/// simulation and of an activatable type. Activators and furniture need a display name
/// (FULL) [hypothesis: unnamed activators are trigger volumes and markers the player is
/// not meant to use; not captured]; placed leveled items and explosives with no weapon
/// to give are not offered.
pub fn describe(sim: &SimState, index: &fnv_world::WorldIndex, form_id: u32) -> Option<(String, Action)> {
    if !sim.present(form_id) {
        return None;
    }
    let id = FormId(form_id);
    let base = sim.forms.get(sim.base_of(form_id)?)?;
    let name = sim.name(base.id)?.to_string();
    let state = sim.world.peek(id);
    let placed = sim.refs.get(id);
    // A reference with lock data starts locked [hypothesis, as fnv_sim::World does].
    let lock = match state {
        Some(s) => s.lock.filter(|l| l.locked).map(|l| (l.level, l.key)),
        None => placed.and_then(|p| p.lock).map(|l| (l.level, l.key)),
    };
    let carried = |k: FormId| {
        sim.world.peek(FormId::PLAYER_REF).and_then(|p| p.inventory.as_ref()).is_some_and(|inv| inv.count(k) > 0)
    };
    let locked = |unlocked: Action| match lock {
        Some((_, Some(k))) if carried(k) => Action::Unlock,
        Some((level, _)) => Action::Locked { level },
        None => unlocked,
    };
    let action = match &base.data {
        BaseData::Item(_) => Action::Take { count: state.map(|s| s.count).or(placed.and_then(|p| p.count)).unwrap_or(1) },
        BaseData::Container(_) => locked(Action::Open),
        BaseData::Door(_) => locked(match placed.and_then(|p| p.teleport) {
            Some(tp) => Action::Travel { to: destination_name(sim, index, tp.door, tp.pos) },
            None if state.is_some_and(|s| s.open) => Action::Close,
            None => Action::Open,
        }),
        BaseData::Actor(_) => Action::Talk,
        BaseData::Activator | BaseData::Furniture => {
            base.name.as_ref()?;
            Action::Activate
        }
        BaseData::LeveledItem(_) | BaseData::Projectile { item: None } => return None,
        BaseData::Projectile { item: Some(_) } => Action::Disarm,
    };
    Some((name, action))
}

/// Name of the cell a door leads into: the destination door's cell, or for a worldspace
/// the exterior cell holding the destination, by display name, else its worldspace's,
/// else its Editor ID.
fn destination_name(sim: &SimState, index: &fnv_world::WorldIndex, door: FormId, pos: [f32; 3]) -> Option<String> {
    let cell = sim.refs.get(door)?.cell?;
    let cell = sim.refs.exterior_cell_at(cell, pos).unwrap_or(cell);
    let c = index.cells.get(&cell.0)?;
    c.name
        .clone()
        .or_else(|| c.world.and_then(|w| index.worldspaces.get(&w)).and_then(|w| w.name.clone()))
        .or_else(|| c.edid.clone())
}

/// The collision ray from the eye along the view (the player's capsule and trigger
/// volumes ignored), then the render-bounds fallback (`pick`).
fn find_target(
    player: Option<Res<Player>>,
    phys: Option<ResMut<PhysState>>,
    sim: Res<SimState>,
    play: Res<PlayWorld>,
    visuals: Query<(&RefVisual, &Aabb, &GlobalTransform, &InheritedVisibility)>,
    mut target: ResMut<Target>,
) {
    let Some(player) = player else { return };
    let (eye, dir) = (player.eye(), player.look_dir());
    let hit = phys.and_then(|mut p| {
        let mut filter = p.player.as_ref().map_or_else(Filter::default, |c| Filter::excluding(c.key()));
        // Trigger volumes do not block the view [hypothesis].
        filter.exclude_layers.insert(layers::TRIGGER);
        let h = p.world.queries().ray(eye, dir, REACH, &filter)?;
        let form_id = match h.owner {
            Owner::Ref { form_id, .. } => Some(form_id),
            _ => None,
        };
        Some(RayHit { form_id, distance: h.distance })
    });
    // Render bounds are in Bevy space; distances are game units in both.
    let (o, d) = (space::to_bevy(eye), space::to_bevy(dir));
    let bounds = visuals.iter().filter(|(_, _, _, v)| v.get()).filter_map(|(r, aabb, gt, _)| {
        let t = gt.affine();
        let (c, h) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
        let (sc, sr) = pick::bounding_sphere(&t, c, h);
        if sc.distance(o) - sr > REACH {
            return None;
        }
        Some((r.0, pick::ray_box(o, d, &t, c, h)?))
    });
    let mut described: HashMap<u32, Option<(String, Action)>> = HashMap::new();
    let mut describe_once = |id: u32| described.entry(id).or_insert_with(|| describe(&sim, play.index, id)).clone();
    let picked = pick::choose(hit, bounds, REACH, |id| describe_once(id).is_some());
    let new = picked.and_then(|Pick { form_id, distance, source }| {
        let (name, action) = describe_once(form_id)?;
        Some(Targeted { form_id, name, action, distance, source })
    });
    let key = |t: &Option<Targeted>| t.as_ref().map(|t| (t.form_id, t.action.clone()));
    if key(&target.0) != key(&new) {
        match &new {
            Some(t) => info!("target 0x{:08X} {} ({}) by {:?} at {:.0}", t.form_id, t.name, t.action.label(), t.source, t.distance),
            None => info!("target none"),
        }
    }
    if target.0 != new {
        target.0 = new;
    }
}

/// Test aid: times (seconds after start) at which E counts as pressed.
#[derive(Resource, Default)]
struct AutoActivate {
    times: Vec<f32>,
}

impl AutoActivate {
    fn from_env() -> AutoActivate {
        let times = std::env::var("NVF_PLAY_TEST_ACTIVATE")
            .map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect())
            .unwrap_or_default();
        AutoActivate { times }
    }

    /// Whether a time has come since the last call (each fires once).
    fn fire(&mut self, now: f32) -> bool {
        let before = self.times.len();
        self.times.retain(|&t| t > now);
        self.times.len() < before
    }
}

/// E activates the target; the outcome's effects go out as messages.
#[allow(clippy::too_many_arguments)]
fn activate(
    keys: Res<ButtonInput<KeyCode>>,
    time: Option<Res<Time>>,
    auto: Option<ResMut<AutoActivate>>,
    target: Res<Target>,
    mut sim: ResMut<SimState>,
    play: Res<PlayWorld>,
    mut hud: ResMut<HudMessage>,
    mut travel: MessageWriter<Travel>,
    mut open: MessageWriter<OpenContainer>,
    mut changed: MessageWriter<RefChanged>,
) {
    let fired = match (auto, time) {
        (Some(mut a), Some(t)) => a.fire(t.elapsed_secs()),
        _ => false,
    };
    if !keys.just_pressed(KeyCode::KeyE) && !fired {
        return;
    }
    let Some(t) = target.0.as_ref() else { return };
    // The actor's place before, to put back if a teleport cannot be followed.
    let before = sim.world.peek(FormId::PLAYER_REF).map(|p| (p.cell, p.pos, p.rot));
    let a = match sim.world.activate(FormId::PLAYER_REF, FormId(t.form_id)) {
        Ok(a) => a,
        Err(e) => {
            warn!("activate 0x{:08X} {}: {e}", t.form_id, t.name);
            hud.show(format!("{}: {e}", t.name));
            return;
        }
    };
    info!("activate 0x{:08X} {} ({}): {:?}", t.form_id, t.name, t.action.label(), a);
    let name_of = |id: FormId| sim.name(id).map_or_else(|| id.to_string(), str::to_string);
    let effects = outcome::effects(&a, t.form_id, &t.name, play.index, name_of);
    let travels = effects.iter().any(|e| matches!(e, Effect::Travel { .. }));
    if matches!(a.outcome, Outcome::Teleported { .. }) && !travels {
        if let (Some((cell, pos, rot)), Ok(p)) = (before, sim.world.get(FormId::PLAYER_REF)) {
            (p.cell, p.pos, p.rot) = (cell, pos, rot);
        }
    }
    for e in effects {
        match e {
            Effect::RefChanged(form_id) => {
                changed.write(RefChanged { form_id });
            }
            Effect::Travel { world, interior, feet, heading_deg } => {
                let to = match (interior, world) {
                    (Some(c), _) => format!("interior 0x{c:08X}"),
                    (None, Some(w)) => format!("worldspace 0x{w:08X}"),
                    (None, None) => "the worldspace".to_string(),
                };
                info!("travel to {to} at {feet:?} heading {heading_deg:.1}");
                travel.write(Travel { world, interior, feet, heading_deg });
            }
            Effect::OpenContainer(container) => {
                open.write(OpenContainer { container });
            }
            Effect::Message(m) => {
                info!("message: {m}");
                hud.show(m);
            }
        }
    }
}

// ---- HUD -------------------------------------------------------------------------

fn spawn_hud(mut commands: Commands) {
    let line = |below: f32| Node {
        position_type: PositionType::Absolute,
        top: Val::Percent(50.0),
        margin: UiRect::top(Val::Px(below)),
        width: Val::Percent(100.0),
        justify_content: JustifyContent::Center,
        ..default()
    };
    let font = |size: f32| (TextFont { font_size: FontSize::Px(size), ..default() }, TextLayout::justify(Justify::Center));
    commands
        .spawn((
            Hud,
            Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
            Visibility::Inherited,
        ))
        .with_children(|hud| {
            // The crosshair: a small square at the centre of the view.
            hud.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(50.0),
                    top: Val::Percent(50.0),
                    width: Val::Px(4.0),
                    height: Val::Px(4.0),
                    margin: UiRect { left: Val::Px(-2.0), top: Val::Px(-2.0), ..default() },
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 0.85, 0.4, 0.9)),
            ));
            hud.spawn(line(18.0)).with_child((Text::new(""), font(16.0), TextColor(Color::srgb(1.0, 0.85, 0.4)), PromptText));
            hud.spawn(line(72.0)).with_child((Text::new(""), font(15.0), TextColor(Color::NONE), MessageText));
        });
}

fn update_hud(
    state: Res<State<PlayState>>,
    target: Res<Target>,
    time: Res<Time>,
    mut msg: ResMut<HudMessage>,
    mut hud: Query<&mut Visibility, With<Hud>>,
    mut prompt: Query<&mut Text, (With<PromptText>, Without<MessageText>)>,
    mut message: Query<(&mut Text, &mut TextColor), With<MessageText>>,
) {
    let playing = *state.get() == PlayState::Playing;
    for mut v in &mut hud {
        v.set_if_neq(if playing { Visibility::Inherited } else { Visibility::Hidden });
    }
    let want = target.0.as_ref().map_or_else(String::new, |t| outcome::prompt(&t.name, &t.action));
    for mut t in &mut prompt {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
    msg.age += time.delta_secs();
    let alpha = if msg.text.is_empty() { 0.0 } else { (1.0 - (msg.age - MESSAGE_HOLD) / MESSAGE_FADE).clamp(0.0, 1.0) };
    for (mut t, mut c) in &mut message {
        if t.0 != msg.text {
            t.0.clone_from(&msg.text);
        }
        c.set_if_neq(TextColor(Color::srgba(1.0, 1.0, 1.0, alpha)));
    }
}

// ---- visual sync -----------------------------------------------------------------

type NewVisuals<'w, 's> = Query<'w, 's, (&'static RefVisual, &'static mut Visibility), Added<RefVisual>>;
type AllVisuals<'w, 's> = Query<'w, 's, (&'static RefVisual, &'static mut Visibility)>;

/// Hide or show `RefVisual` entities from the simulation's state: every entity of a
/// reference named by `RefChanged`, and new entities as cells stream in (a taken item
/// must not come back when its cell reloads).
fn sync_visuals(
    sim: Res<SimState>,
    mut changes: MessageReader<RefChanged>,
    mut visuals: ParamSet<(NewVisuals, AllVisuals)>,
) {
    let mut present: HashMap<u32, bool> = HashMap::new();
    for (r, mut v) in &mut visuals.p0() {
        if !*present.entry(r.0).or_insert_with(|| sim.present(r.0)) {
            v.set_if_neq(Visibility::Hidden);
        }
    }
    let mut changed: HashMap<u32, usize> = changes.read().map(|m| (m.form_id, 0)).collect();
    if changed.is_empty() {
        return;
    }
    for (r, mut v) in &mut visuals.p1() {
        if let Some(n) = changed.get_mut(&r.0) {
            v.set_if_neq(if sim.present(r.0) { Visibility::Inherited } else { Visibility::Hidden });
            *n += 1;
        }
    }
    for (id, n) in changed {
        info!("reference 0x{id:08X} {}: {n} render entities updated", if sim.present(id) { "present" } else { "gone" });
    }
}
