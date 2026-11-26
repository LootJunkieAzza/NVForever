//! A cell's bounce light: what its lamps in sight add to what the surfaces show, gathered around points in the air
//! and packed as a small 3D texture, with the constants to show it by. These are the first parts of a solver (#3009,
//! `docs/design/rtgi-solver.md`). Plain data in and out: no Bevy, no files, no GPU, no state between calls. The
//! same input gives the same bytes on any number of threads.
//!
//! [`prepare`] makes the ray scene once; `bvh`, `texture` and `trace` are its parts. [`solve()`] makes the texture,
//! for one state of the light: `light` is the formula of the engine's light sum, `gather` the bounce light around a
//! point, `grid` the points that can be reached, `volume` the texture and its reading. [`reach()`] gives the points
//! and their light as they are. [`fit()`] makes the constants, by what is seen from above a [`Floor`];
//! [`solution()`] is both.
//!
//! Space is Bevy's world in game units: y up, -z north. Colours are linear unless called display values.
//!
//! A room with a lamp: what a ray meets on its floor, the light in its air, what is read of it on the floor, and how
//! strong it is to be shown.
//!
//! ```
//! use nvf_rtgi::*;
//!
//! // A box, 200 a side, open on no side; its surfaces are lit on either.
//! let corner = |i: usize| [(i & 1) as f32 * 200.0 - 100.0, (i >> 1 & 1) as f32 * 200.0, (i >> 2 & 1) as f32 * 200.0 - 100.0];
//! let sides = [[0, 1, 3, 2], [4, 5, 7, 6], [0, 1, 5, 4], [2, 3, 7, 6], [0, 2, 6, 4], [1, 3, 7, 5]];
//! let scene = SceneInput {
//!     meshes: vec![Mesh {
//!         positions: (0..8).map(corner).collect(),
//!         indices: sides.iter().flat_map(|[a, b, c, d]| [*a, *b, *c, *a, *c, *d]).collect(),
//!         ..Mesh::default()
//!     }],
//!     instances: vec![Instance { mesh: 0, material: 0, world_from_local: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]] }],
//!     materials: vec![Material { base: [0.6, 0.5, 0.4, 1.0], cull: Cull::None, double_sided: true, ..Material::default() }],
//!     textures: Vec::new(),
//! };
//! let scene = prepare(scene, &SceneLimits::default(), 1, &|| false)?;
//! assert!(scene.stats().sound());
//!
//! let eye = V3::new(10.0, 64.0, 20.0);
//! let hit = scene.transport(&Ray::new(eye, V3::new(0.0, -1.0, 0.0)), f32::INFINITY).unwrap();
//! assert!(!hit.hidden && hit.p.0[1].abs() < 1e-4);
//! assert!(hit.facing().0[1] > 0.999);
//! assert!((hit.albedo(Lookup::Color)[0] - 0.6).abs() < 1e-5);
//! assert!(scene.sees(hit.p, hit.facing(), eye));
//!
//! let light = LightState {
//!     ambient: V3([0.3; 3]),
//!     directionals: Vec::new(),
//!     lamps: vec![Lamp { position: V3::new(30.0, 150.0, -20.0), display: V3([0.8; 3]), range: 300.0, cone: None }],
//! };
//! let reached = reach(&scene, &light, &[eye], &GatherConfig { rays: 64, ..GatherConfig::default() }, &|| false)?;
//! assert!(reached.coverage.parts == 1 && reached.points.len() == 27);
//! let [mean, ..] = reached.points[&[0, 1, 0]].moments.c;
//! assert!(mean.0[0] > mean.0[2] && mean.0[2] > 0.0);
//!
//! let config = SolveConfig { gather: GatherConfig { rays: 64, ..GatherConfig::default() }, ..SolveConfig::default() };
//! let volume = solve(&scene, &light, &Seeds::at(vec![eye]), &config, &|| false)?;
//! assert!(volume.dimensions == [3, 3, 3] && volume.low == [-1, 1, -1]);
//! let up = V3::new(0.0, 1.0, 0.0);
//! let read = Reader::new(&volume).unwrap().read(hit.p, hit.facing(), up);
//! assert!(read.e().0[0] > read.e().0[2] && read.v() > 0.0);
//!
//! let corner = |x: f32, z: f32| V3::new(x, 0.0, z);
//! let triangles = [[corner(-90.0, -90.0), corner(-90.0, 90.0), corner(90.0, 90.0)], [corner(-90.0, -90.0), corner(90.0, 90.0), corner(90.0, -90.0)]];
//! let seeds = Seeds::of(Floor::new(&triangles, &[], &FloorLimits::default(), &|| false)?);
//! let FitOutcome::Fitted(fitted) = fit(&scene, &light, &volume, &seeds, &FitConfig::default(), &|| false)? else { panic!() };
//! assert!(fitted.b >= 0.25 && fitted.g > 0.0 && fitted.g <= 10.0 && fitted.met.lit == 64 * 16);
//! # Ok::<(), Failed>(())
//! ```

#![forbid(unsafe_code)]

mod bvh;
mod failed;
mod fit;
mod floor;
mod gather;
mod grid;
mod light;
mod parallel;
mod reach;
mod scene;
mod solve;
mod texture;
mod trace;
mod vec3;
mod volume;

pub use bvh::Ray;
pub use failed::{Cancel, Failed, Reason};
pub use fit::{FitConfig, FitOutcome, Fitted, Met, Population, Solution, Stock};
pub use floor::{Floor, FloorLimits, Room, EYE};
pub use gather::{Moments, Probe};
pub use grid::{Coverage, Place};
pub use light::{Cone, Directional, Lamp, LightState};
pub use reach::{GatherConfig, GatherLimits, Reached, MOST_RAYS};
pub use scene::{Cull, Instance, Level, Material, Mesh, SceneInput, Texture, Wrap};
pub use solve::{Counts, Seeds, SolveConfig, SolvedVolume, Stats};
pub use texture::Lookup;
pub use trace::{lift, Hit, PreparedScene, SceneLimits, SceneStats, MOST_THREADS};
pub use vec3::V3;
pub use volume::{Read, Reader, SLABS, THIN};

/// The ray scene of a cell. What is not sound is left out or replaced, and counted ([`SceneStats`]). `cancel`
/// is asked between pieces of the work, of a large tree on its threads too.
pub fn prepare(scene: SceneInput, limits: &SceneLimits, threads: usize, cancel: Cancel) -> Result<PreparedScene, Failed> {
    failed::guarded(|| PreparedScene::of(scene, limits, threads, cancel))
}

/// The points of the air that `seeds`, places in the air, reach, and the bounce light around each. The seeds'
/// order does not matter. `cancel` is asked before every point that is gathered, before every group of up to 64
/// segments of one start, and now and then during the rest, on any thread.
pub fn reach(scene: &PreparedScene, light: &LightState, seeds: &[V3], config: &GatherConfig, cancel: Cancel) -> Result<Reached, Failed> {
    failed::guarded(|| Reached::of(scene, light, seeds, config, cancel))
}

/// The bounce light of the points the seeds reach, as a texture. `cancel` as for [`reach()`], and now and then
/// during the packing.
pub fn solve(scene: &PreparedScene, light: &LightState, seeds: &Seeds, config: &SolveConfig, cancel: Cancel) -> Result<SolvedVolume, Failed> {
    failed::guarded(|| SolvedVolume::of(scene, light, &seeds.points, config, cancel))
}

/// The constants of a cell by its volume, for the state of the light that it was solved in. The places are above
/// the floor of `seeds`; of seeds that are no floor's, some of the points that were reached. That the volume is the
/// scene's and the light's is the caller's word. `cancel` is asked at every cell of the lamps' table, every 4,096
/// texels, before every place, and before every candidate's mean over all that the rays met, on any thread: up to 50 of
/// them, 178 where a gain is below 2^-12 of the cap. The stock mean, worked out twice, is not asked for.
pub fn fit(scene: &PreparedScene, light: &LightState, volume: &SolvedVolume, seeds: &Seeds, config: &FitConfig, cancel: Cancel) -> Result<FitOutcome, Failed> {
    failed::guarded(|| fit::fitted(scene, light, volume, seeds, config, cancel))
}

/// [`solve()`], then [`fit()`] in the same light. What the fit's config or the ambient may not be is refused before
/// the solve.
pub fn solution(scene: &PreparedScene, light: &LightState, seeds: &Seeds, config: &SolveConfig, fit: &FitConfig, cancel: Cancel) -> Result<Solution, Failed> {
    failed::guarded(|| fit::solved(scene, light, seeds, config, fit, cancel))
}

#[cfg(test)]
mod tests {
    use super::*;
    use trace::tests::{quad, scene, SAME};

    /// A panic of the caller's check is a failure, not the caller's end.
    #[test]
    fn panics_do_not_get_out() {
        let input = || scene(vec![(quad(0.0), SAME, Material::default())], Vec::new());
        let check = || -> bool { panic!("expected here") };
        assert_eq!(prepare(input(), &SceneLimits::default(), 1, &check).err(), Some(Failed::Internal));
        assert!(prepare(input(), &SceneLimits::default(), 1, &|| false).is_ok_and(|w| w.stats().sound()));
    }
}
