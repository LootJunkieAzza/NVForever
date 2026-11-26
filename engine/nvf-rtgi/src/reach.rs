//! From a prepared scene, its light and the seeds to the points of the air and their light. What `reach` and
//! `solve` both do first is here, so that they refuse the same.

use crate::failed::{unavailable, Cancel, Failed, Reason};
use crate::gather::Probe;
use crate::grid::{point, Coverage, Grid, Place, Work};
use crate::light::{LightState, Lights};
use crate::trace::{PreparedScene, MOST_THREADS};
use crate::vec3::V3;
use std::collections::{BTreeMap, BTreeSet};

/// The spacings there may be: their reciprocals and a room's coordinates over them are numbers.
pub(crate) const SPACINGS: std::ops::RangeInclusive<f32> = 1e-3..=1e6;
/// The shares of the unlit surfaces' own light there may be.
pub(crate) const EMISSIONS: std::ops::RangeInclusive<f32> = 0.0..=1.0;
/// The most rays of a point.
pub const MOST_RAYS: usize = 4096;

/// How the light is gathered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GatherConfig {
    /// Between the points, in units, 0.001 to 1,000,000.
    pub spacing: f32,
    /// Of a point, 1 to [`MOST_RAYS`].
    pub rays: usize,
    /// The share of the unlit surfaces' own light that counts, 0 to 1.
    pub emission: f32,
    pub limits: GatherLimits,
    /// 1 to [`MOST_THREADS`].
    pub threads: usize,
}

impl Default for GatherConfig {
    fn default() -> GatherConfig {
        GatherConfig { spacing: 64.0, rays: 512, emission: 0.25, limits: GatherLimits::default(), threads: 1 }
    }
}

/// What it may take; past it there is nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GatherLimits {
    /// Points that are gathered, reached or not. The default is a budget for play.
    pub points: usize,
    pub seeds: usize,
}

impl Default for GatherLimits {
    fn default() -> GatherLimits {
        GatherLimits { points: 40_000, seeds: 1 << 16 }
    }
}

impl GatherConfig {
    fn check(&self) -> Result<(), Failed> {
        let fine = [
            ("spacing", SPACINGS.contains(&self.spacing)),
            ("rays", (1..=MOST_RAYS).contains(&self.rays)),
            ("emission", EMISSIONS.contains(&self.emission)),
            ("threads", (1..=MOST_THREADS).contains(&self.threads)),
        ];
        match fine.iter().find(|f| !f.1) {
            Some((what, _)) => unavailable(Reason::Input(what)),
            None => Ok(()),
        }
    }

    /// The light as the rays ask for it. What may not be is refused before its table is made.
    pub(crate) fn lights<'a>(&self, light: &'a LightState, seeds: &[V3], cancel: Cancel) -> Result<Lights<'a>, Failed> {
        self.check()?;
        let GatherLimits { points, seeds: most } = self.limits;
        if seeds.is_empty() {
            return unavailable(Reason::NoSeeds);
        }
        if seeds.len() > most {
            return unavailable(Reason::TooMany { what: "seeds", limit: most });
        }
        if points == 0 {
            return unavailable(Reason::TooManyPoints { limit: 0 });
        }
        Lights::new(light, cancel)
    }

    /// The grid after its flood, and what is reached: one point at least. The config is one that gave the lights.
    pub(crate) fn flood<'a>(&self, world: &'a PreparedScene, lights: &'a Lights<'a>, seeds: &[V3], cancel: Cancel<'a>) -> Result<(Grid<'a>, BTreeSet<Place>, Coverage), Failed> {
        let GatherConfig { spacing: space, rays, emission, limits, threads } = *self;
        let mut grid = Grid::new(Work { world, lights, emission, rays, space, threads, limit: limits.points, cancel });
        let (reached, coverage) = grid.flood(seeds)?;
        if reached.is_empty() {
            return unavailable(Reason::NothingReached { seeds: seeds.len() });
        }
        Ok((grid, reached, coverage))
    }
}

/// The points that the seeds reach ([`crate::reach()`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Reached {
    pub spacing: f32,
    /// With their light; all of them are valid.
    pub points: BTreeMap<Place, Probe>,
    pub coverage: Coverage,
    /// Points that were gathered, these included.
    pub gathered: usize,
}

impl Reached {
    /// Where a point is.
    pub fn position(&self, at: Place) -> V3 {
        point(at, self.spacing)
    }

    pub(crate) fn of(scene: &PreparedScene, light: &LightState, seeds: &[V3], config: &GatherConfig, cancel: Cancel) -> Result<Reached, Failed> {
        let lights = config.lights(light, seeds, cancel)?;
        let (grid, reached, coverage) = config.flood(scene, &lights, seeds, cancel)?;
        let points = grid.made().filter(|(at, _)| reached.contains(*at)).map(|(at, p)| (*at, *p)).collect();
        Ok(Reached { spacing: config.spacing, points, coverage, gathered: grid.made().count() })
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::gather::directions;
    use crate::gather::tests::{lamp, plain, rectangle, sides, state, traced_again, Part, AMBIENT};
    use crate::light::tests::lights_of;
    use crate::light::{Cone, Directional, Lamp};
    use crate::reach;
    use crate::scene::{Cull, Material, SceneInput, Wrap};
    use crate::texture::tests::rgba;
    use crate::trace::tests::{prepared, scene};
    use std::sync::atomic::{AtomicUsize, Ordering};

    const NEVER: Cancel = &|| false;

    /// Two closed rooms, 20 units between them, with no point of the grid there.
    pub fn walled() -> Vec<Part> {
        let room = |x: f32| sides(V3::new(x, 100.0, 100.0), V3::new(210.0, 150.0, 150.0), plain(), |_, _| true);
        [room(-190.0), room(250.0)].concat()
    }

    /// The lamp is in the left one.
    pub fn light() -> LightState {
        state(AMBIENT, vec![lamp(V3::new(-60.0, 100.0, 100.0))])
    }

    pub fn left() -> V3 {
        V3::new(-100.0, 100.0, 100.0)
    }

    pub fn right() -> V3 {
        V3::new(200.0, 100.0, 100.0)
    }

    fn config() -> GatherConfig {
        GatherConfig { emission: 0.0, threads: 2, ..GatherConfig::default() }
    }

    /// Cutouts, a double-sided wall, a mirrored copy, a solid block, an unlit ceiling, a spot and a sun.
    pub fn furnished() -> (SceneInput, LightState) {
        let mut parts = walled();
        let cut = Material { texture: Some(0), cutoff: 0.5, cull: Cull::None, double_sided: true, ..plain() };
        let (mut fence, place, material) = rectangle(V3::new(-150.0, 100.0, 100.0), V3::new(0.0, 100.0, 0.0), V3::new(0.0, 0.0, 100.0), cut);
        fence.uv0 = vec![[0.0, 0.0], [6.0, 0.0], [6.0, 6.0], [0.0, 6.0]];
        fence.colors = vec![[0.9, 1.0, 0.5, 1.0], [1.0; 4], [1.0, 1.0, 1.0, 0.8], [1.0; 4]];
        parts.push((fence.clone(), place, material));
        parts.push((fence, [[-0.5, 0.0, 0.0, -320.0], [0.0, 0.3, 0.0, 10.0], [0.0, 0.0, 0.4, 30.0]], material));
        parts.push(rectangle(V3::new(-250.0, 249.0, 100.0), V3::new(40.0, 0.0, 0.0), V3::new(0.0, 0.0, 40.0), Material { base: [1.0, 0.9, 0.6, 1.0], unlit: true, ..plain() }));
        parts.extend(sides(V3::new(120.0, 30.0, 100.0), V3([-50.0; 3]), plain(), |_, _| true));
        let holes: Vec<[u8; 4]> = (0..16).map(|i| [200, 160, 90 + i, if (i / 4 + i % 4) % 2 == 0 { 255 } else { 0 }]).collect();
        let spot = Lamp { cone: Some(Cone::new(V3::new(-0.3, -1.0, 0.1), 0.4, 0.9)), display: V3::new(1.0, 0.7, 0.3), ..lamp(V3::new(-120.0, 220.0, 60.0)) };
        let mut light = light();
        light.lamps.extend([spot, Lamp { range: 150.0, ..lamp(V3::new(300.0, 30.0, 140.0)) }]);
        light.directionals.push(Directional { display: V3([0.1, 0.12, 0.2]), to_source: V3::new(0.0, 0.8, 0.6) });
        (scene(parts, vec![Some(rgba(4, 4, &holes, true, Wrap::Repeat))]), light)
    }

    /// A flood stays in its room, and its light is there.
    #[test]
    fn walls() {
        let world = prepared(scene(walled(), Vec::new()), 1);
        let of = |seeds: &[V3], spacing: f32| reach(&world, &light(), seeds, &GatherConfig { spacing, ..config() }, NEVER);
        let one = of(&[left()], 64.0).unwrap();
        assert_eq!(one.coverage, Coverage { seeds: 1, reached: 112, parts: 1, largest: 112, in_bounds: 224, ..Coverage::default() });
        assert!(one.points.len() == 112 && one.gathered == 112 && one.points.values().all(|p| p.valid() && p.hits == 512));
        assert!(one.points.contains_key(&[-6, 0, 3]) && one.position([-6, 0, 3]) == V3::new(-384.0, 0.0, 192.0));
        let both = of(&[left(), right()], 64.0).unwrap();
        assert_eq!(both.coverage, Coverage { seeds: 2, reached: 224, parts: 2, ..one.coverage });
        // The dark room's points are valid and black.
        assert!(both.points.iter().all(|(at, p)| p.valid() && (p.moments.c[0][0] > 0.0) == (at[0] <= 0)));
        assert!(one.points.iter().all(|(at, p)| both.points[at] == *p));
        assert_eq!(of(&[V3::new(30.0, 100.0, 100.0)], 64.0), Err(Failed::Unavailable(Reason::NothingReached { seeds: 1 })));
        // A seed that is nowhere is one that found nothing.
        let with = of(&[V3::new(f32::NAN, 0.0, 0.0), left(), V3([f32::INFINITY; 3])], 64.0).unwrap();
        assert!(with.points == one.points && with.coverage == Coverage { seeds: 3, failed: 2, ..one.coverage });
        let wide = of(&[left()], 96.0).unwrap();
        assert!(wide.points.len() == 45 && wide.position([-4, 0, 0])[0] == -384.0);
    }

    /// The points by hand, all of the two rooms' but those in the solid block, and at each the light of rays
    /// traced again for it alone; the same on any number of threads, from a scene prepared on any.
    #[test]
    fn rays_traced_again() {
        let (input, light) = furnished();
        let of = |threads: usize| {
            let world = prepared(input.clone(), threads);
            reach(&world, &light, &[left(), right(), V3::new(-300.0, 60.0, 60.0)], &GatherConfig { threads, emission: 0.25, ..config() }, NEVER).unwrap()
        };
        let one = of(1);
        for threads in [4, 12] {
            assert!(of(threads) == one, "{threads}");
        }
        let (world, lights, rays) = (prepared(input.clone(), 2), lights_of(&light), directions(512));
        let places: Vec<Place> = (0..14 * 4 * 4).map(|i| [i % 14 - 6, i / 14 % 4, i / 56]).collect();
        let again = crate::parallel::parallel(places.len(), 4, |i| traced_again(&world, &lights, point(places[i], 64.0), &rays, 0.25));
        for (at, (moments, valid)) in places.iter().zip(&again) {
            let solid = at[0] == 2 && at[1] < 2 && (1..3).contains(&at[2]);
            assert_eq!((*valid, one.points.get(at).map(|p| p.moments)), (!solid, (!solid).then_some(*moments)), "{at:?}");
        }
        assert!(one.points.len() == 220 && one.points.values().map(|p| p.shadow_rays).sum::<u32>() > 1000);
    }

    #[test]
    fn what_is_unavailable() {
        let world = prepared(scene(walled(), Vec::new()), 1);
        let why = |world: &PreparedScene, light: &LightState, seeds: &[V3], config: &GatherConfig| match reach(world, light, seeds, config, NEVER) {
            Err(Failed::Unavailable(reason)) => reason,
            other => panic!("{other:?}"),
        };
        let of = |config: GatherConfig| why(&world, &light(), &[left()], &config);
        for spacing in [0.0, -1.0, f32::from_bits(1), 9.9e-4, 1.1e6, f32::NAN, f32::INFINITY] {
            assert_eq!(of(GatherConfig { spacing, ..config() }), Reason::Input("spacing"));
        }
        for emission in [-1.0, 1.001, f32::MAX, f32::NAN, f32::INFINITY] {
            assert_eq!(of(GatherConfig { emission, ..config() }), Reason::Input("emission"));
        }
        for n in [0, MOST_RAYS + 1, usize::MAX] {
            assert_eq!(of(GatherConfig { rays: n, ..config() }), Reason::Input("rays"));
            assert_eq!(of(GatherConfig { threads: n, ..config() }), Reason::Input("threads"));
        }
        let limits = |points, seeds| GatherConfig { limits: GatherLimits { points, seeds }, ..config() };
        assert_eq!(of(limits(20, 1)), Reason::TooManyPoints { limit: 20 });
        assert_eq!(of(limits(111, 1)), Reason::TooManyPoints { limit: 111 });
        assert!(reach(&world, &light(), &[left()], &limits(112, 1), NEVER).is_ok());
        assert_eq!(why(&world, &light(), &[left(), left(), V3([f32::NAN; 3])], &limits(1000, 2)), Reason::TooMany { what: "seeds", limit: 2 });
        assert_eq!(why(&world, &light(), &[], &config()), Reason::NoSeeds);
        assert_eq!(why(&world, &light(), &[V3::new(0.0, f32::NAN, 0.0)], &config()), Reason::NothingReached { seeds: 1 });
        // No budget at all: before the lamps are looked at.
        for (config, reason) in [(limits(0, 1), Reason::TooManyPoints { limit: 0 }), (limits(20, 0), Reason::TooMany { what: "seeds", limit: 0 })] {
            assert_eq!(reach(&world, &light(), &[left()], &config, &|| true), Err(Failed::Unavailable(reason)));
        }
        let spot = |cone| why(&world, &state(AMBIENT, vec![Lamp { cone: Some(cone), ..lamp(left()) }]), &[left()], &config());
        assert_eq!(spot(Cone::new(V3([f32::NAN; 3]), 0.3, 0.6)), Reason::Input("light"));
        assert_eq!(spot(Cone::new(V3::new(0.0, -1.0, 0.0), f32::NAN, 0.6)), Reason::Input("light"));
        assert_eq!(why(&world, &state(AMBIENT, vec![lamp(V3([f32::NAN; 3]))]), &[left()], &config()), Reason::Input("light"));
        assert_eq!(why(&prepared(SceneInput::default(), 4), &light(), &[left()], &config()), Reason::NothingReached { seeds: 1 });
        let said = [Reason::NoSeeds, Reason::NothingReached { seeds: 7 }, Reason::TooManyPoints { limit: 8 }].map(|r| r.to_string());
        assert_eq!(said, ["no seeds", "no point of the grid is reached from 7 seeds", "more than 8 points"]);
    }

    /// A panic of the caller's check is a failure, not the caller's end, on the threads of the work too.
    #[test]
    fn panics_do_not_get_out() {
        let world = prepared(scene(walled(), Vec::new()), 1);
        for after in [0, 40] {
            let asked = AtomicUsize::new(0);
            let check = || {
                assert!(asked.fetch_add(1, Ordering::Relaxed) < after, "expected here");
                false
            };
            assert_eq!(reach(&world, &light(), &[left()], &GatherConfig { threads: 4, ..config() }, &check), Err(Failed::Internal));
        }
    }

    /// Before any work, between the pieces of it, and not after its end.
    #[test]
    fn cancelling() {
        let world = prepared(scene(walled(), Vec::new()), 1);
        let asked = AtomicUsize::new(0);
        let until = |n: usize| {
            asked.store(0, Ordering::Relaxed);
            let cancel = || asked.fetch_add(1, Ordering::Relaxed) >= n;
            reach(&world, &light(), &[left()], &config(), &cancel)
        };
        let whole = until(usize::MAX).unwrap();
        let times = asked.load(Ordering::Relaxed);
        assert!(times > 112, "{times}");
        for n in [0, 1, times / 2, times - 1] {
            assert_eq!(until(n), Err(Failed::Cancelled));
        }
        assert!(until(times).unwrap() == whole);
    }
}
