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
//!   `TransformTreeChanged` this frame; `root_kinds=<kind>:<roots>/<dirty>,…`; `World`'s
//!   direct children, and those with a dirty subtree, in all and by kind (`cell`; `ref`, a
//!   `RefVisual`, as actors; `mesh`, as LOD; `other`); `no_dirty_cell_frames`, frames in which
//!   `World` was dirty with no cell dirty: nothing marked in a cell. A mark is a `Transform`
//!   written (the same value too), a parent changed or a new `GlobalTransform`, not a move.
//! - `transform walk:` `examined` and `recomputed` (below); `gt_changed`, `GlobalTransform`s
//!   changed, every one in the world; `marked`, `TransformTreeChanged`s set; `written`,
//!   `Transform`s changed, and `written_same`, those whose `GlobalTransform` did not change;
//!   `stable_root_examined` and `stable_root_recomputed`, the same two with a root that does
//!   not change above `World` (below); `collection_ms`, the time spent counting in `count`,
//!   without what summing, formatting and logging the numbers take: not all this aid costs.
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
//! So `World`, a root, is changed in every frame anything under it moves, and that costs each
//! clean direct child a multiply and each of *its* children a test; below them a clean
//! subtree stops, as `set_if_neq` left the value alone. That is all a root above `World`
//! would save, and the `stable_root_` numbers take exactly that away (and add `World` itself,
//! then recomputed): with `World` unchanged, a clean direct child stops at its test. The
//! change ticks read here run from this system's last run, not Bevy's. Only a change applied
//! between Bevy's system and this one can make them differ, by a frame [design].

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
type Roots<'w, 's> = Query<'w, 's, (Ref<'static, TransformTreeChanged>, Option<&'static Children>, RootKind), (With<Transform>, Without<ChildOf>)>;
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
    /// `walk` as it would be with a root that does not change above `World`.
    stable: Walk,
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
        self.walk.examined += other.walk.examined;
        self.walk.recomputed += other.walk.recomputed;
        self.stable.examined += other.stable.examined;
        self.stable.recomputed += other.stable.recomputed;
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
        if self.static_off_frames > 0 {
            let kinds: Vec<String> = self.root_kinds.iter().map(|(k, (r, _))| format!("{k}:{:.2}", per(*r))).collect();
            return [
                format!(
                    "transform roots{summary}: secs={secs:.2} frames={} left_out={left_out} static_optimization=0 static_off_frames={} model=not_applicable roots={:.2} childless_roots={:.2} root_kinds={} world_children={:.2} world_children_by_kind={}",
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
                "transform roots{summary}: secs={secs:.2} frames={} left_out={left_out} static_optimization=1 roots={:.2} childless_roots={:.2} dirty_roots={:.2} root_kinds={} world_dirty_frames={} no_dirty_cell_frames={} world_children={:.2} world_children_by_kind={} dirty_world_children={:.2} dirty_world_children_by_kind={}",
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
                "transform walk{summary}: frames={} examined={:.1} recomputed={:.1} gt_changed={:.1} marked={:.1} written={:.1} written_same={:.1} stable_root_examined={:.1} stable_root_recomputed={:.1} collection_ms={:.3}",
                self.frames,
                per(self.walk.examined),
                per(self.walk.recomputed),
                per(self.gt_changed),
                per(self.marked),
                per(self.written),
                per(self.written_same),
                per(self.stable.examined),
                per(self.stable.recomputed),
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
    (with_global, changed_global, marked, written): Counts,
    statics: Res<StaticTransformOptimizations>,
    streamer: Option<Res<Streamer>>,
    loading: Option<Res<nvf_play::loading::Loading>>,
) {
    let t0 = Instant::now();
    let log = &mut *log;
    let mut f = Tally { frames: 1, static_off_frames: u64::from(!statics.is_enabled()), ..default() };
    // Not the frames that load the start, nor those behind a loading screen, in the summary.
    let counted = !(streamer.as_deref().is_some_and(|s| !s.settled) || loading.as_deref().is_some_and(|l| l.unseen_frames()));
    // A census on a window's first frame, and on the summary's first: a loading screen can
    // hold a window's first frame, and the summary would have none until the next window's.
    let census = log.window.census_samples == 0 || (counted && log.run.census_samples == 0);
    for (tree, children, kind) in &roots {
        let Some(children) = children else {
            f.childless_roots += 1;
            continue;
        };
        f.roots += 1;
        let dirty = tree.is_changed();
        f.dirty_roots += u64::from(dirty);
        let is_world = kind.0;
        let e = f.root_kinds.entry(root_kind(kind)).or_default();
        e.0 += 1;
        e.1 += u64::from(dirty);
        if !is_world {
            if dirty {
                let mut under = Walk::default();
                walk(children, true, &nodes, &mut under);
                f.walk.examined += under.examined;
                f.walk.recomputed += under.recomputed;
                f.stable.examined += under.examined;
                f.stable.recomputed += under.recomputed;
            }
            continue;
        }
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
            f.walk.examined += 1;
            f.walk.recomputed += 1;
            let mut under = Walk::default();
            if let Some(below) = below {
                walk(below, global.is_changed(), &nodes, &mut under);
            }
            f.walk.examined += under.examined;
            f.walk.recomputed += under.recomputed;
            // With `World` unchanged a clean child is examined and stops there.
            f.stable.examined += 1;
            if child_dirty {
                f.stable.examined += under.examined;
                f.stable.recomputed += 1 + under.recomputed;
            }
        }
        if dirty {
            // `World` itself, then a child of the root above it.
            f.stable.examined += 1;
            f.stable.recomputed += 1;
            f.no_dirty_cell_frames += u64::from(!any_dirty_cell);
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
        let mut app = App::new();
        app.add_plugins((TaskPoolPlugin::default(), TransformPlugin));
        app.insert_resource(RootsLog::new(1000.0)).add_systems(PostUpdate, count.after(TransformSystems::Propagate));
        let w = app.world_mut();
        let world = w.spawn((WorldRoot, Transform::from_rotation(Quat::from_rotation_x(1.0)))).id();
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
        assert_eq!(t.under_dirty, 3);
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
