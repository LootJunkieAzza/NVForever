//! What `propagate_parent_transforms` has to look at each frame, counted from outside
//! Bevy (issue #1387; docs/design/frame-rate-audit.md §1). A measuring aid
//! (`nvf_play::MEASURING_AIDS`): nothing is in the schedule without it.
//!
//! `NVF_PLAY_TEST_TRANSFORM_ROOTS=<seconds>`: every that many real seconds three lines of
//! per-frame means, `key=value` as `phys-refresh:`, and the same three with `summary` at the
//! exit over the frames after start-up, leaving out those behind a loading screen
//! (`left_out`). That is not the campaign's population: its `warmup` drops the first seconds
//! of its timing after the first frame-time line and does not touch these lines, so a
//! summary here can hold frames the campaign's times leave out, and the reverse:
//!
//! - `transform roots:` the roots with children and those without (the second are not
//!   propagation's, `sync_simple_transforms` has them); `dirty_roots`, those whose tree has
//!   `TransformTreeChanged` this frame; `root_kinds=<kind>:<roots>/<dirty>,…` (`world` for
//!   `World` as a root, `world_parent` for the root above it); `world_depth`, the ancestors
//!   `World` has, and `world_parent_children`, what the root above it holds;
//!   `world_global_changed_frames`, frames `World`'s `GlobalTransform` changed, and
//!   `world_off_local_frames`, those it was not `World`'s local matrix, which particles read
//!   as the conversion. `World` is found by its marker, a root or not. `World`'s
//!   direct children, and those with a dirty subtree, in all and by kind (`cell`; `ref`, a
//!   `RefVisual`, as actors; `mesh`, as LOD; `other`); `no_dirty_cell_frames`, frames in which
//!   `World` was dirty with no cell dirty: nothing marked in a cell. A mark is a `Transform`
//!   written (the same value too), a parent changed or a new `GlobalTransform`, not a move.
//! - `transform walk:` `examined` and `recomputed` (below); `gt_changed`, `GlobalTransform`s
//!   changed, every one in the world; `marked`, `TransformTreeChanged`s set; `written`,
//!   `Transform`s changed, and `written_same`, those whose `GlobalTransform` did not change;
//!   `stable_root_examined` and `stable_root_recomputed`, the same two with `World` under a
//!   parent that does not change, and `world_root_examined` and `world_root_recomputed`, with
//!   `World` a root (below); `collection_ms`, the time spent counting in `count`, without what
//!   summing, formatting and logging the numbers take: not all this aid costs.
//! - `transform census:` a sample on a window's first frame and on the summary's first,
//!   walking whole subtrees: `gt_total`, `under_world`, `under_dirty` (under `World`'s direct
//!   children with a dirty subtree), each with its `most_`.
//!
//! With Bevy's static optimization off for any frame of the lines (`static_optimization=0`),
//! they say `model=not_applicable` and give neither the model's numbers nor the dirty counts:
//! no marks are set (`mark_dirty_trees`, :131) and propagation skips nothing (:524, :707), so
//! both would read 0 while Bevy goes over everything [design].
//!
//! The counting is too slow for a run that times the frame: time without this aid.
//!
//! **`examined` and `recomputed` are a model, not Bevy's own numbers**: the system keeps no
//! count, and it is not patched. [`walk`] goes over the tree after the system by its rule
//! [design: bevy_transform 0.20.1 `src/systems.rs`, parallel `propagate_parent_transforms`]:
//!
//! - a root whose `TransformTreeChanged` is unchanged is passed by. Otherwise its
//!   `GlobalTransform` is assigned (:529), which marks it changed even with the same value;
//! - each child of a node reached is `examined`. A child whose own `TransformTreeChanged` and
//!   whose parent's `GlobalTransform` are both unchanged stops there (:707-712). The others are
//!   `recomputed` (a multiply and `set_if_neq`, :718), and their children are reached even if
//!   the value came out the same.
//!
//! So `World` as a root is changed in every frame anything under it moves, and that costs each
//! clean direct child a multiply and each of *its* children a test; below them a clean
//! subtree stops, as `set_if_neq` left the value alone. That is all a root above `World`
//! saves, and the `stable_root_` numbers take exactly that away (and add `World` itself,
//! then recomputed): with `World` unchanged, a clean direct child stops at its test.
//!
//! `stable_root_` and `world_root_` are worked out from `World`'s children, whatever is above
//! it, so each run gives both arrangements: `world_depth` says which is the game's, whose two
//! numbers are then `examined` and `recomputed` (with `World`'s global unchanged), and the
//! other pair is the model's guess at the arrangement this run is not in.
//!
//! The change ticks read here run from this system's last run, not Bevy's. Only a change
//! applied between Bevy's system and this one can make them differ, by a frame [design].

use bevy::prelude::*;
use bevy::transform::components::TransformTreeChanged;
use bevy::transform::systems::StaticTransformOptimizations;
use bevy::transform::TransformSystems;
use nvf_play::RefVisual;
use std::collections::BTreeMap;
use std::time::Instant;

use crate::world::{CellEntity, Streamer, WorldRoot};

/// The aid's variable: the window in seconds.
const VAR: &str = "NVF_PLAY_TEST_TRANSFORM_ROOTS";

fn window_seconds() -> Option<f32> {
    std::env::var(VAR).ok().and_then(|v| nvf_play::transform_roots_window(&v))
}

pub struct TransformRootsPlugin;

impl Plugin for TransformRootsPlugin {
    fn build(&self, app: &mut App) {
        let Some(every) = window_seconds() else { return };
        info!("test aid {VAR}: transform propagation counts every {every} s");
        app.insert_resource(RootsLog::new(every)).add_systems(PostUpdate, count.after(TransformSystems::Propagate)).add_systems(Last, summary);
    }
}

/// The kinds of `World`'s direct children.
const CHILD_KINDS: [&str; 4] = ["cell", "ref", "mesh", "other"];

type NodeData = (Ref<'static, TransformTreeChanged>, Ref<'static, GlobalTransform>, Option<&'static Children>, Has<CellEntity>, Has<RefVisual>, Has<Mesh3d>);
/// As Bevy's: what has a `Transform` and a parent.
type Nodes<'w, 's> = Query<'w, 's, NodeData, (With<Transform>, With<ChildOf>)>;
type RootKind = (Has<WorldRoot>, Has<Camera>, Has<DirectionalLight>, Has<PointLight>, Has<SpotLight>, Has<Mesh3d>, Option<&'static Name>);
type Roots<'w, 's> = Query<'w, 's, (Entity, Ref<'static, TransformTreeChanged>, Option<&'static Children>, RootKind), (With<Transform>, Without<ChildOf>)>;
/// `World`, a root or not, and what finds its ancestors.
type WorldNode<'w, 's> = (
    Query<'w, 's, (Entity, Ref<'static, TransformTreeChanged>, Ref<'static, GlobalTransform>, &'static Transform, Option<&'static Children>), With<WorldRoot>>,
    Query<'w, 's, &'static ChildOf>,
);
/// Each counted by iterating it: with a `GlobalTransform`, those changed, the markers set,
/// and the `Transform`s written with their `GlobalTransform`.
type Counts<'w, 's> = (
    Query<'w, 's, (), With<GlobalTransform>>,
    Query<'w, 's, (), Changed<GlobalTransform>>,
    Query<'w, 's, (), Changed<TransformTreeChanged>>,
    Query<'w, 's, Ref<'static, GlobalTransform>, Changed<Transform>>,
);

/// What propagation does, by the rule in the module's notes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Walk {
    examined: u64,
    recomputed: u64,
}

impl Walk {
    fn add(&mut self, other: Walk) {
        self.examined += other.examined;
        self.recomputed += other.recomputed;
    }
}

/// The children of a reached node. `parent_changed`: its `GlobalTransform` changed.
fn walk(children: &Children, parent_changed: bool, nodes: &Nodes, n: &mut Walk) {
    for (tree, global, below, ..) in nodes.iter_many(children).matched() {
        n.examined += 1;
        if !tree.is_changed() && !parent_changed {
            continue;
        }
        n.recomputed += 1;
        if let Some(below) = below {
            walk(below, global.is_changed(), nodes, n);
        }
    }
}

fn subtree_size(children: &Children, nodes: &Nodes) -> u64 {
    nodes.iter_many(children).matched().map(|(_, _, below, ..)| 1 + below.map_or(0, |b| subtree_size(b, nodes))).sum()
}

fn root_kind((world, camera, sun, point, spot, mesh, name): (bool, bool, bool, bool, bool, bool, Option<&Name>)) -> String {
    if world {
        "world".into()
    } else if camera {
        "camera".into()
    } else if sun || point || spot {
        "light".into()
    } else if mesh {
        "mesh".into()
    } else {
        // Names as in `profile transforms:`, without their numbers.
        name.map_or("unnamed".into(), |n| n.as_str().split(|c: char| c.is_ascii_digit()).next().unwrap_or("").to_string())
    }
}

/// Sums over some frames, for per-frame means.
#[derive(Clone, Debug, Default)]
struct Tally {
    frames: u64,
    roots: u64,
    childless_roots: u64,
    dirty_roots: u64,
    /// Roots, dirty roots, by kind.
    root_kinds: BTreeMap<String, (u64, u64)>,
    world_dirty_frames: u64,
    no_dirty_cell_frames: u64,
    world_children: [u64; CHILD_KINDS.len()],
    dirty_world_children: [u64; CHILD_KINDS.len()],
    walk: Walk,
    /// `walk` with a parent that does not change above `World`.
    stable: Walk,
    /// `walk` with `World` a root.
    as_root: Walk,
    /// `World`'s ancestors, and what the root above it holds.
    world_depth: u64,
    world_parent_children: u64,
    world_global_changed_frames: u64,
    world_off_local_frames: u64,
    gt_changed: u64,
    marked: u64,
    written: u64,
    written_same: u64,
    collection_ms: f64,
    /// Frames with Bevy's static optimization off: the model does not apply to them.
    static_off_frames: u64,
    census_samples: u64,
    /// Totals of the census samples.
    gt_total: u64,
    under_world: u64,
    under_dirty: u64,
    /// The largest sample of each.
    most: [u64; 3],
}

impl Tally {
    fn add_frame(&mut self, other: &Tally) {
        self.frames += other.frames;
        self.roots += other.roots;
        self.childless_roots += other.childless_roots;
        self.dirty_roots += other.dirty_roots;
        for (k, (r, d)) in &other.root_kinds {
            let e = self.root_kinds.entry(k.clone()).or_default();
            e.0 += r;
            e.1 += d;
        }
        self.world_dirty_frames += other.world_dirty_frames;
        self.no_dirty_cell_frames += other.no_dirty_cell_frames;
        for i in 0..CHILD_KINDS.len() {
            self.world_children[i] += other.world_children[i];
            self.dirty_world_children[i] += other.dirty_world_children[i];
        }
        self.walk.add(other.walk);
        self.stable.add(other.stable);
        self.as_root.add(other.as_root);
        self.world_depth += other.world_depth;
        self.world_parent_children += other.world_parent_children;
        self.world_global_changed_frames += other.world_global_changed_frames;
        self.world_off_local_frames += other.world_off_local_frames;
        self.gt_changed += other.gt_changed;
        self.marked += other.marked;
        self.written += other.written;
        self.written_same += other.written_same;
        self.collection_ms += other.collection_ms;
        self.static_off_frames += other.static_off_frames;
        self.census_samples += other.census_samples;
        self.gt_total += other.gt_total;
        self.under_world += other.under_world;
        self.under_dirty += other.under_dirty;
        for i in 0..self.most.len() {
            self.most[i] = self.most[i].max(other.most[i]);
        }
    }

    /// The three lines (the module's notes).
    fn lines(&self, summary: &str, secs: f32, left_out: u64) -> [String; 3] {
        let frames = self.frames.max(1) as f64;
        let per = |n: u64| n as f64 / frames;
        let by_kind = |n: &[u64; CHILD_KINDS.len()]| CHILD_KINDS.iter().zip(n).map(|(k, v)| format!("{k}:{:.2}", per(*v))).collect::<Vec<_>>().join("/");
        let samples = self.census_samples.max(1);
        let world = format!(
            "world_depth={:.2} world_parent_children={:.2} world_global_changed_frames={} world_off_local_frames={}",
            per(self.world_depth),
            per(self.world_parent_children),
            self.world_global_changed_frames,
            self.world_off_local_frames,
        );
        if self.static_off_frames > 0 {
            let kinds: Vec<String> = self.root_kinds.iter().map(|(k, (r, _))| format!("{k}:{:.2}", per(*r))).collect();
            return [
                format!(
                    "transform roots{summary}: secs={secs:.2} frames={} left_out={left_out} static_optimization=0 static_off_frames={} model=not_applicable roots={:.2} childless_roots={:.2} root_kinds={} {world} world_children={:.2} world_children_by_kind={}",
                    self.frames,
                    self.static_off_frames,
                    per(self.roots),
                    per(self.childless_roots),
                    kinds.join(","),
                    per(self.world_children.iter().sum()),
                    by_kind(&self.world_children),
                ),
                format!(
                    "transform walk{summary}: frames={} model=not_applicable gt_changed={:.1} written={:.1} written_same={:.1} collection_ms={:.3}",
                    self.frames,
                    per(self.gt_changed),
                    per(self.written),
                    per(self.written_same),
                    self.collection_ms / frames,
                ),
                format!(
                    "transform census{summary}: samples={} model=not_applicable gt_total={} most_gt_total={} under_world={} most_under_world={}",
                    self.census_samples,
                    self.gt_total / samples,
                    self.most[0],
                    self.under_world / samples,
                    self.most[1],
                ),
            ];
        }
        let kinds: Vec<String> = self.root_kinds.iter().map(|(k, (r, d))| format!("{k}:{:.2}/{:.2}", per(*r), per(*d))).collect();
        [
            format!(
                "transform roots{summary}: secs={secs:.2} frames={} left_out={left_out} static_optimization=1 roots={:.2} childless_roots={:.2} dirty_roots={:.2} root_kinds={} {world} world_dirty_frames={} no_dirty_cell_frames={} world_children={:.2} world_children_by_kind={} dirty_world_children={:.2} dirty_world_children_by_kind={}",
                self.frames,
                per(self.roots),
                per(self.childless_roots),
                per(self.dirty_roots),
                kinds.join(","),
                self.world_dirty_frames,
                self.no_dirty_cell_frames,
                per(self.world_children.iter().sum()),
                by_kind(&self.world_children),
                per(self.dirty_world_children.iter().sum()),
                by_kind(&self.dirty_world_children),
            ),
            format!(
                "transform walk{summary}: frames={} examined={:.1} recomputed={:.1} gt_changed={:.1} marked={:.1} written={:.1} written_same={:.1} stable_root_examined={:.1} stable_root_recomputed={:.1} world_root_examined={:.1} world_root_recomputed={:.1} collection_ms={:.3}",
                self.frames,
                per(self.walk.examined),
                per(self.walk.recomputed),
                per(self.gt_changed),
                per(self.marked),
                per(self.written),
                per(self.written_same),
                per(self.stable.examined),
                per(self.stable.recomputed),
                per(self.as_root.examined),
                per(self.as_root.recomputed),
                self.collection_ms / frames,
            ),
            format!(
                "transform census{summary}: samples={} gt_total={} most_gt_total={} under_world={} most_under_world={} under_dirty={} most_under_dirty={}",
                self.census_samples,
                self.gt_total / samples,
                self.most[0],
                self.under_world / samples,
                self.most[1],
                self.under_dirty / samples,
                self.most[2],
            ),
        ]
    }

    fn log(&self, summary: &str, secs: f32, left_out: u64) {
        for line in self.lines(summary, secs, left_out) {
            info!("{line}");
        }
    }
}

#[derive(Resource)]
struct RootsLog {
    every: f32,
    window_start: Option<Instant>,
    window: Tally,
    run_start: Option<Instant>,
    run: Tally,
    left_out: u64,
}

impl RootsLog {
    fn new(every: f32) -> Self {
        RootsLog { every, window_start: None, window: Tally::default(), run_start: None, run: Tally::default(), left_out: 0 }
    }
}

fn count(
    mut log: ResMut<RootsLog>,
    roots: Roots,
    nodes: Nodes,
    (world, ancestors): WorldNode,
    (with_global, changed_global, marked, written): Counts,
    statics: Res<StaticTransformOptimizations>,
    (streamer, loading): (Option<Res<Streamer>>, Option<Res<nvf_play::loading::Loading>>),
) {
    let t0 = Instant::now();
    let log = &mut *log;
    let mut f = Tally { frames: 1, static_off_frames: u64::from(!statics.is_enabled()), ..default() };
    // Not the frames that load the start, nor those behind a loading screen, in the summary.
    let counted = !(streamer.as_deref().is_some_and(|s| !s.settled) || loading.as_deref().is_some_and(|l| l.unseen_frames()));
    // A census on a window's first frame, and on the summary's first: a loading screen can
    // hold a window's first frame, and the summary would have none until the next window's.
    let census = log.window.census_samples == 0 || (counted && log.run.census_samples == 0);
    let world = world.iter().next();
    // The root that has `World` in its tree: `World` itself, or the top of what is above it.
    let world_top = world.as_ref().map(|(e, ..)| ancestors.iter_ancestors(*e).last().unwrap_or(*e));
    for (root, tree, children, kind) in &roots {
        let Some(children) = children else {
            f.childless_roots += 1;
            continue;
        };
        f.roots += 1;
        let dirty = tree.is_changed();
        f.dirty_roots += u64::from(dirty);
        let holds_world = world_top == Some(root);
        let above_world = holds_world && !kind.0;
        if above_world {
            f.world_parent_children += children.len() as u64;
        }
        let e = f.root_kinds.entry(if above_world { "world_parent".to_string() } else { root_kind(kind) }).or_default();
        e.0 += 1;
        e.1 += u64::from(dirty);
        if dirty {
            let mut under = Walk::default();
            walk(children, true, &nodes, &mut under);
            f.walk.add(under);
            if !holds_world {
                f.stable.add(under);
                f.as_root.add(under);
            }
        }
    }
    if let Some((entity, tree, global, local, children)) = world {
        f.world_depth = ancestors.iter_ancestors(entity).count() as u64;
        f.world_global_changed_frames = u64::from(global.is_changed());
        f.world_off_local_frames = u64::from(GlobalTransform::from(*local) != *global);
        if let Some(children) = children {
            let dirty = tree.is_changed();
            f.world_dirty_frames += u64::from(dirty);
            let mut any_dirty_cell = false;
            for (tree, global, below, cell, reference, mesh) in nodes.iter_many(children).matched() {
                let k = if cell {
                    0
                } else if reference {
                    1
                } else if mesh {
                    2
                } else {
                    3
                };
                let child_dirty = tree.is_changed();
                f.world_children[k] += 1;
                f.dirty_world_children[k] += u64::from(child_dirty);
                any_dirty_cell |= cell && child_dirty;
                if census {
                    let size = 1 + below.map_or(0, |b| subtree_size(b, &nodes));
                    f.under_world += size;
                    f.under_dirty += if child_dirty { size } else { 0 };
                }
                if !dirty {
                    continue;
                }
                let mut under = Walk::default();
                if let Some(below) = below {
                    walk(below, global.is_changed(), &nodes, &mut under);
                }
                // With `World` a root every child is recomputed.
                f.as_root.add(Walk { examined: 1 + under.examined, recomputed: 1 + under.recomputed });
                // With `World` unchanged a clean child is examined and stops there.
                f.stable.examined += 1;
                if child_dirty {
                    f.stable.examined += under.examined;
                    f.stable.recomputed += 1 + under.recomputed;
                }
            }
            if dirty {
                // `World` itself, then a child of the root above it.
                f.stable.add(Walk { examined: 1, recomputed: 1 });
                f.no_dirty_cell_frames += u64::from(!any_dirty_cell);
            }
        }
    }
    f.gt_changed = changed_global.iter().count() as u64;
    f.marked = marked.iter().count() as u64;
    for global in &written {
        f.written += 1;
        f.written_same += u64::from(!global.is_changed());
    }
    if census {
        f.census_samples = 1;
        f.gt_total = with_global.iter().count() as u64;
        f.most = [f.gt_total, f.under_world, f.under_dirty];
    }
    f.collection_ms = t0.elapsed().as_secs_f64() * 1000.0;

    log.window.add_frame(&f);
    let now = Instant::now();
    let start = *log.window_start.get_or_insert(now);
    if counted {
        log.run_start.get_or_insert(now);
        log.run.add_frame(&f);
    } else {
        log.left_out += 1;
    }
    let secs = now.duration_since(start).as_secs_f32();
    if secs >= log.every {
        log.window.log("", secs, 0);
        log.window = Tally::default();
        log.window_start = Some(now);
    }
}

fn summary(mut exit: MessageReader<AppExit>, log: Res<RootsLog>) {
    if exit.read().next().is_none() {
        return;
    }
    let secs = log.run_start.map_or(0.0, |t| t.elapsed().as_secs_f32());
    log.run.log(" summary", secs, log.left_out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::app::TaskPoolPlugin;
    use bevy::transform::TransformPlugin;

    /// World
    /// ├ cell A ─ ref A ─ leaf (what moves)
    /// ├ cell B ─ ref B1 ─ part, ref B2
    /// └ cell C, no children
    fn scene() -> (App, Entity) {
        scene_with(None)
    }

    /// The same under the identity parent of `world::setup_root`.
    fn under_parent() -> (App, Entity) {
        scene_with(Some(Transform::IDENTITY))
    }

    fn scene_with(above: Option<Transform>) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((TaskPoolPlugin::default(), TransformPlugin));
        app.insert_resource(RootsLog::new(1000.0)).add_systems(PostUpdate, count.after(TransformSystems::Propagate));
        let w = app.world_mut();
        let world = w.spawn((WorldRoot, Transform::from_rotation(Quat::from_rotation_x(1.0)))).id();
        if let Some(above) = above {
            let parent = w.spawn(above).id();
            w.entity_mut(world).insert(ChildOf(parent));
        }
        let cell_a = w.spawn((CellEntity, Transform::from_xyz(1.0, 0.0, 0.0), ChildOf(world))).id();
        let ref_a = w.spawn((RefVisual(1), Transform::default(), ChildOf(cell_a))).id();
        let leaf = w.spawn((Transform::default(), ChildOf(ref_a))).id();
        let cell_b = w.spawn((CellEntity, Transform::from_xyz(2.0, 0.0, 0.0), ChildOf(world))).id();
        let ref_b1 = w.spawn((RefVisual(2), Transform::default(), ChildOf(cell_b))).id();
        w.spawn((RefVisual(3), Transform::default(), ChildOf(cell_b)));
        w.spawn((Transform::default(), ChildOf(ref_b1)));
        w.spawn((CellEntity, Transform::from_xyz(3.0, 0.0, 0.0), ChildOf(world)));
        // Two updates: what was spawned, then one with nothing new.
        app.update();
        app.update();
        (app, leaf)
    }

    fn step(app: &mut App) -> Tally {
        app.world_mut().resource_mut::<RootsLog>().window = Tally::default();
        app.update();
        app.world().resource::<RootsLog>().window.clone()
    }

    #[test]
    fn a_still_tree_is_passed_by() {
        let (mut app, _) = scene();
        let t = step(&mut app);
        assert_eq!((t.roots, t.dirty_roots, t.world_dirty_frames), (1, 0, 0));
        assert_eq!(t.walk, Walk::default());
        assert_eq!(t.gt_changed, 0);
        assert_eq!(t.world_children, [3, 0, 0, 0]);
        assert_eq!((t.gt_total, t.under_world, t.under_dirty), (9, 8, 0));
    }

    #[test]
    fn one_moving_leaf_counts_by_bevys_rule() {
        let (mut app, leaf) = scene();
        app.world_mut().get_mut::<Transform>(leaf).unwrap().translation.x += 1.0;
        let t = step(&mut app);
        assert_eq!((t.dirty_roots, t.world_dirty_frames, t.no_dirty_cell_frames), (1, 1, 0));
        assert_eq!(t.dirty_world_children, [1, 0, 0, 0]);
        // Cells A, B, C; refs A, B1, B2; the leaf. B1's part is not reached.
        assert_eq!(t.walk.examined, 7);
        // Cells A, B, C; ref A; the leaf.
        assert_eq!(t.walk.recomputed, 5);
        // World (assigned though the same) and the leaf.
        assert_eq!(t.gt_changed, 2);
        // The leaf, ref A, cell A, World.
        assert_eq!(t.marked, 4);
        assert_eq!((t.written, t.written_same), (1, 0));
        // A root above. Examined: World; cells A, B, C; ref A; the leaf. Recomputed: World,
        // cell A, ref A, the leaf.
        assert_eq!(t.stable, Walk { examined: 6, recomputed: 4 });
        assert_eq!(t.as_root, t.walk, "World is a root, so that is the real walk");
        assert_eq!(t.under_dirty, 3);
        assert_eq!((t.world_depth, t.world_parent_children, t.world_global_changed_frames, t.world_off_local_frames), (0, 0, 1, 0));
    }

    /// Trips if `World` stops being found under a parent: the root loop alone would count it as
    /// an unknown kind, with no census, no direct children and `stable_root_` copying the walk.
    #[test]
    fn under_a_parent_a_still_tree_is_passed_by_and_world_is_still_counted() {
        let (mut app, _) = under_parent();
        let t = step(&mut app);
        assert_eq!((t.roots, t.dirty_roots, t.world_dirty_frames), (1, 0, 0));
        assert_eq!(t.root_kinds.get("world_parent"), Some(&(1, 0)));
        assert_eq!(t.root_kinds.get("world"), None);
        assert_eq!((t.walk, t.stable, t.as_root), (Walk::default(), Walk::default(), Walk::default()));
        assert_eq!(t.gt_changed, 0);
        assert_eq!(t.world_children, [3, 0, 0, 0]);
        assert_eq!((t.gt_total, t.under_world, t.under_dirty), (10, 8, 0));
        assert_eq!((t.world_depth, t.world_parent_children, t.world_global_changed_frames, t.world_off_local_frames), (1, 1, 0, 0));
    }

    #[test]
    fn under_a_parent_one_moving_leaf_walks_what_the_model_said() {
        let (mut app, leaf) = under_parent();
        app.world_mut().get_mut::<Transform>(leaf).unwrap().translation.x += 1.0;
        let t = step(&mut app);
        assert_eq!((t.dirty_roots, t.world_dirty_frames, t.no_dirty_cell_frames), (1, 1, 0));
        assert_eq!(t.root_kinds.get("world_parent"), Some(&(1, 1)));
        assert_eq!(t.dirty_world_children, [1, 0, 0, 0]);
        // Examined: World; cells A, B, C; ref A; the leaf. Recomputed: World, cell A, ref A,
        // the leaf. From the root down and from `World`'s children, the same by now.
        assert_eq!(t.walk, Walk { examined: 6, recomputed: 4 });
        assert_eq!(t.stable, t.walk);
        // What the other arrangement walks, as in `one_moving_leaf_counts_by_bevys_rule`.
        assert_eq!(t.as_root, Walk { examined: 7, recomputed: 5 });
        // The parent (assigned though the same) and the leaf: not `World`.
        assert_eq!(t.gt_changed, 2);
        // The leaf, ref A, cell A, World, the parent.
        assert_eq!(t.marked, 5);
        assert_eq!(t.under_dirty, 3);
        assert_eq!((t.world_depth, t.world_parent_children, t.world_global_changed_frames, t.world_off_local_frames), (1, 1, 0, 0));
    }

    #[test]
    fn under_a_parent_a_write_of_the_same_value_changes_only_the_parent() {
        let (mut app, leaf) = under_parent();
        app.world_mut().get_mut::<Transform>(leaf).unwrap().set_changed();
        let t = step(&mut app);
        assert_eq!((t.written, t.written_same), (1, 1));
        assert_eq!(t.gt_changed, 1, "only the parent");
        assert_eq!(t.world_global_changed_frames, 0);
        assert_eq!(t.walk, Walk { examined: 6, recomputed: 4 });
    }

    /// Trips if a parent that moves goes unseen: `World`'s global would no longer be the
    /// conversion particles read from its local.
    #[test]
    fn a_parent_that_moves_is_reported() {
        let (mut app, _) = under_parent();
        let w = app.world_mut();
        let world = w.query_filtered::<Entity, With<WorldRoot>>().single(&*w).unwrap();
        let parent = w.get::<ChildOf>(world).unwrap().parent();
        w.get_mut::<Transform>(parent).unwrap().translation.x += 1.0;
        let t = step(&mut app);
        assert_eq!((t.world_global_changed_frames, t.world_off_local_frames), (1, 1));
        // `World` and everything under it.
        assert_eq!(t.walk, Walk { examined: 9, recomputed: 9 });
        // Nothing under `World` is dirty, so its own numbers do not claim that walk.
        assert_ne!(t.stable, t.walk);
    }

    /// Trips if the lines stop saying which arrangement they were counted in.
    #[test]
    fn the_lines_say_where_world_is() {
        let (mut app, leaf) = under_parent();
        app.world_mut().get_mut::<Transform>(leaf).unwrap().translation.x += 1.0;
        let [roots, walk, census] = step(&mut app).lines("", 1.0, 0);
        assert!(roots.contains("root_kinds=world_parent:1.00/1.00 world_depth=1.00 world_parent_children=1.00 world_global_changed_frames=0 world_off_local_frames=0 "), "{roots}");
        assert!(walk.contains(" examined=6.0 recomputed=4.0 ") && walk.contains(" stable_root_examined=6.0 stable_root_recomputed=4.0 world_root_examined=7.0 world_root_recomputed=5.0 "), "{walk}");
        assert!(census.contains(" under_world=8 "), "{census}");

        let (mut app, _) = scene();
        let [roots, ..] = step(&mut app).lines("", 1.0, 0);
        assert!(roots.contains("root_kinds=world:1.00/0.00 world_depth=0.00 world_parent_children=0.00 "), "{roots}");

        // The arrangement is not part of the model: it is said with the optimization off too.
        let (mut app, _) = under_parent();
        app.insert_resource(StaticTransformOptimizations::Disabled);
        let t = step(&mut app);
        let [roots, ..] = t.lines("", 1.0, 0);
        assert!(roots.contains("model=not_applicable") && roots.contains("root_kinds=world_parent:1.00 world_depth=1.00 world_parent_children=1.00 world_global_changed_frames=0 world_off_local_frames=0 "), "{roots}");
    }

    #[test]
    fn a_write_of_the_same_value_marks_but_changes_nothing_below_the_root() {
        let (mut app, leaf) = scene();
        app.world_mut().get_mut::<Transform>(leaf).unwrap().set_changed();
        let t = step(&mut app);
        assert_eq!((t.written, t.written_same), (1, 1));
        assert_eq!(t.gt_changed, 1, "only World");
        assert_eq!(t.walk.recomputed, 5);
    }

    /// As main.rs's, but for what `count` reads: the cells around the start not in yet.
    fn unsettled() -> Streamer {
        Streamer {
            radius: 0,
            loaded: default(),
            fades: default(),
            fade_secs: 0.0,
            settled: false,
            arrival: None,
            per_frame: 1,
            spawning: default(),
            dying: default(),
            ahead: default(),
            hide: default(),
            interior: None,
            light_scale: 1.0,
            managed: default(),
            manage_actors: false,
            stats: default(),
            buffer: default(),
            centre: None,
            waiting: default(),
            model_overrides: default(),
        }
    }

    /// Trips if the summary's census waits for a window's first frame: this window's was
    /// taken before the frames it counts, so the summary would say `samples=0` over a loaded
    /// world.
    #[test]
    fn the_summary_takes_its_census_though_the_windows_first_frame_was_left_out() {
        let (mut app, _) = scene();
        app.insert_resource(unsettled());
        app.world_mut().resource_mut::<RootsLog>().run = Tally::default();
        let window_samples = app.world().resource::<RootsLog>().window.census_samples;
        assert!(window_samples > 0, "the window has its sample already");

        app.update();
        let log = app.world().resource::<RootsLog>();
        assert_eq!((log.run.frames, log.run.census_samples, log.left_out), (0, 0, 1), "left out");
        assert_eq!(log.window.census_samples, window_samples, "and no second sample for the window");

        app.world_mut().resource_mut::<Streamer>().settled = true;
        app.update();
        let log = app.world().resource::<RootsLog>();
        assert_eq!((log.run.frames, log.run.census_samples, log.run.under_world), (1, 1, 8));

        app.update();
        let log = app.world().resource::<RootsLog>();
        assert_eq!((log.run.frames, log.run.census_samples), (2, 1), "one is enough");
    }

    /// Trips if the lines give the model's numbers (or the dirty counts, all 0 with no marks
    /// set) with Bevy's optimization off, or if they stop giving them with it on.
    #[test]
    fn the_model_is_not_reported_with_static_optimization_off() {
        let (mut app, leaf) = scene();
        let on = step(&mut app).lines("", 1.0, 0).join("\n");
        assert!(on.contains("static_optimization=1 ") && on.contains(" examined=") && on.contains(" dirty_roots=") && on.contains(" under_dirty="), "{on}");
        assert!(!on.contains("model=not_applicable"), "{on}");

        app.insert_resource(StaticTransformOptimizations::Disabled);
        app.world_mut().get_mut::<Transform>(leaf).unwrap().translation.x += 1.0;
        let t = step(&mut app);
        assert_eq!(t.static_off_frames, 1);
        let [roots, walk, census] = t.lines("", 1.0, 0);
        for line in [&roots, &walk, &census] {
            assert!(line.contains("model=not_applicable"), "{line}");
            assert!(!line.contains("dirty") && !line.contains("examined") && !line.contains("recomputed") && !line.contains("marked"), "{line}");
        }
        assert!(roots.contains("static_optimization=0 ") && roots.contains("root_kinds=world:1.00 "), "{roots}");
    }
}
