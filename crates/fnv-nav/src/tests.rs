//! Graph, path and walker tests on synthetic navmeshes (no game data).

use crate::graph::{EdgeKind, NavGraph, Space, NONE};
use crate::path::{PathError, PathParams, Place};
use crate::record::{link_kind, tri_flags, DoorLink, EdgeLink, NavMeshRecord, Triangle};
use crate::{WalkEvent, Walker};
use std::collections::HashSet;

const W: Space = Space::World(0x3C);

/// A navmesh over a grid of `nx` × `ny` squares of side `size` from `origin`, each
/// square two counter-clockwise triangles (south-east and north-west of its diagonal),
/// except the squares in `holes`. Heights follow `z(x, y)`.
fn grid(form_id: u32, origin: [f32; 2], nx: usize, ny: usize, size: f32, holes: &[(usize, usize)], z: impl Fn(f32, f32) -> f32) -> NavMeshRecord {
    let vid = |i: usize, j: usize| (j * (nx + 1) + i) as u16;
    let mut vertices = Vec::new();
    for j in 0..=ny {
        for i in 0..=nx {
            let (x, y) = (origin[0] + i as f32 * size, origin[1] + j as f32 * size);
            vertices.push([x, y, z(x, y)]);
        }
    }
    let holes: HashSet<(usize, usize)> = holes.iter().copied().collect();
    // Triangle index of (square, half): half 0 = (a, b, c), half 1 = (a, c, d).
    let mut index = std::collections::HashMap::new();
    for j in 0..ny {
        for i in 0..nx {
            if !holes.contains(&(i, j)) {
                for h in 0..2 {
                    let n = index.len() as i16;
                    index.insert((i, j, h), n);
                }
            }
        }
    }
    let at = |i: isize, j: isize, h: usize| -> i16 {
        if i < 0 || j < 0 {
            return -1;
        }
        index.get(&(i as usize, j as usize, h)).copied().unwrap_or(-1)
    };
    let mut triangles = vec![Triangle { v: [0; 3], edges: [-1; 3], flags: 0, cover: 0 }; index.len()];
    for (&(i, j, h), &n) in &index {
        let (a, b, c, d) = (vid(i, j), vid(i + 1, j), vid(i + 1, j + 1), vid(i, j + 1));
        let (ii, jj) = (i as isize, j as isize);
        triangles[n as usize] = if h == 0 {
            Triangle { v: [a, b, c], edges: [at(ii, jj - 1, 1), at(ii + 1, jj, 1), at(ii, jj, 1)], flags: 0, cover: 0 }
        } else {
            Triangle { v: [a, c, d], edges: [at(ii, jj, 0), at(ii, jj + 1, 0), at(ii - 1, jj, 0)], flags: 0, cover: 0 }
        };
    }
    NavMeshRecord { form_id, vertices, triangles, ..Default::default() }
}

/// Link `a`'s east boundary edges to `b`'s west boundary edges (and back) where they
/// overlap, as NVEX portals.
fn seam(a: &mut NavMeshRecord, b: &mut NavMeshRecord, kind: u32) {
    let ax = a.vertices.iter().map(|v| v[0]).fold(f32::MIN, f32::max);
    let east: Vec<(usize, usize, [f32; 3], [f32; 3])> = edges_at(a, ax);
    let west = edges_at(b, ax);
    for &(at, ae, p0, p1) in &east {
        let (lo, hi) = (p0[1].min(p1[1]), p0[1].max(p1[1]));
        let mid = (lo + hi) / 2.0;
        if let Some(&(bt, be, ..)) = west.iter().find(|w| w.2[1].min(w.3[1]) <= mid && mid <= w.2[1].max(w.3[1])) {
            link(a, at, ae, b.form_id, bt, kind);
            link(b, bt, be, a.form_id, at, kind);
        }
    }
}

/// Boundary edges of `m` lying on the line x = `x`: (triangle, edge, ends).
fn edges_at(m: &NavMeshRecord, x: f32) -> Vec<(usize, usize, [f32; 3], [f32; 3])> {
    let mut out = Vec::new();
    for (ti, t) in m.triangles.iter().enumerate() {
        for e in 0..3 {
            let (p, q) = (m.vertices[t.v[e] as usize], m.vertices[t.v[(e + 1) % 3] as usize]);
            if t.edges[e] == -1 && (p[0] - x).abs() < 1e-3 && (q[0] - x).abs() < 1e-3 {
                out.push((ti, e, p, q));
            }
        }
    }
    out
}

fn link(m: &mut NavMeshRecord, t: usize, e: usize, to: u32, tt: usize, kind: u32) {
    m.triangles[t].flags |= 1 << e;
    m.triangles[t].edges[e] = m.edge_links.len() as i16;
    m.edge_links.push(EdgeLink { kind, navmesh: Some(to), triangle: tt as u16 });
}

fn graph(meshes: &[(NavMeshRecord, Space)]) -> NavGraph {
    let input: Vec<_> = meshes.iter().map(|(m, s)| (m, Some(0), Some(*s))).collect();
    NavGraph::assemble(&input)
}

fn flat(_: f32, _: f32) -> f32 {
    0.0
}

/// Every point along the polyline (every `step` units) has navmesh under it.
fn on_mesh(g: &NavGraph, space: Space, points: &[[f32; 3]], step: f32) -> bool {
    points.windows(2).all(|w| {
        let n = ((w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]) / step).ceil().max(1.0) as usize;
        (0..=n).all(|k| {
            let s = k as f32 / n as f32;
            let p = [w[0][0] + (w[1][0] - w[0][0]) * s, w[0][1] + (w[1][1] - w[0][1]) * s, w[0][2] + (w[1][2] - w[0][2]) * s];
            g.locate(space, p, 1000.0).is_some()
        })
    })
}

#[test]
fn internal_edges_resolve_and_outline_is_marked() {
    let g = graph(&[(grid(1, [0.0, 0.0], 3, 2, 100.0, &[], flat), W)]);
    assert_eq!(g.tris.len(), 12);
    assert_eq!(g.stats.internal_edges, 12 * 3 - 10); // 36 triangle edges, 10 on the outline
    assert_eq!(g.stats.boundary_edges, 10);
    // Every internal neighbour points back.
    for (i, t) in g.tris.iter().enumerate() {
        for e in 0..3 {
            if t.kind[e] == EdgeKind::Internal {
                assert!(g.tris[t.next[e] as usize].next.contains(&(i as u32)));
            }
        }
    }
    // The centre vertex (1, 1) of the grid is inside; the corner (0, 0) on the outline.
    assert!(g.boundary_vertex[0]);
    assert!(!g.boundary_vertex[4 + 1]);
    let t = g.locate(W, [150.0, 50.0, 10.0], 100.0).expect("triangle under the point");
    assert!(g.contains_xy(t, 150.0, 50.0));
    assert_eq!(g.locate(W, [350.0, 50.0, 0.0], 100.0), None);
}

#[test]
fn straight_path_in_the_open() {
    let g = graph(&[(grid(1, [0.0, 0.0], 10, 10, 100.0, &[], flat), W)]);
    let p = g.find_path(Place { space: W, pos: [150.0, 150.0, 0.0] }, Place { space: W, pos: [850.0, 750.0, 0.0] }, &PathParams::default()).unwrap();
    assert_eq!(p.legs.len(), 1);
    assert_eq!(p.legs[0].points.len(), 2, "{:?}", p.legs[0].points);
    assert!((p.length - (700.0f32).hypot(600.0)).abs() < 1.0);
}

#[test]
fn path_goes_round_a_wall_with_clearance() {
    // A wall of holes across x = 400..500 from y = 0 to 800; the way round is at the north.
    let holes: Vec<(usize, usize)> = (0..8).map(|j| (4, j)).collect();
    let g = graph(&[(grid(1, [0.0, 0.0], 10, 10, 100.0, &holes, flat), W)]);
    let params = PathParams { radius: 24.0, ..Default::default() };
    let p = g.find_path(Place { space: W, pos: [150.0, 150.0, 0.0] }, Place { space: W, pos: [850.0, 150.0, 0.0] }, &params).unwrap();
    let pts = &p.legs[0].points;
    assert!(pts.len() >= 4, "{pts:?}");
    assert!(on_mesh(&g, W, pts, 5.0), "{pts:?}");
    // Every corner keeps the clearance from the wall (x 400-500, y 0-800).
    for c in &pts[1..pts.len() - 1] {
        let dx = (400.0 - c[0]).max(c[0] - 500.0).max(0.0);
        let dy = (c[1] - 800.0).max(0.0);
        assert!(dx.hypot(dy) >= 23.9, "corner too close to the wall: {c:?} in {pts:?}");
    }
    // Rounding the wall's end, the path never turns back west.
    assert!(pts.windows(2).all(|w| w[1][0] >= w[0][0] - 1e-3), "{pts:?}");
    assert!(p.length > 1400.0 && p.length < 1700.0, "{}", p.length);
}

#[test]
fn path_crosses_a_seam_between_navmeshes_whose_grids_differ() {
    let mut a = grid(1, [0.0, 0.0], 4, 4, 100.0, &[], flat);
    let mut b = grid(2, [400.0, -50.0], 3, 3, 150.0, &[], flat);
    seam(&mut a, &mut b, link_kind::PORTAL);
    let g = graph(&[(a, W), (b, W)]);
    assert!(g.stats.portal_edges > 0);
    assert_eq!(g.stats.unresolved_links, 0);
    let p = g.find_path(Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: W, pos: [800.0, 300.0, 0.0] }, &PathParams::default()).unwrap();
    assert!(on_mesh(&g, W, &p.legs[0].points, 5.0));
    assert!((p.length - (750.0f32).hypot(250.0)).abs() < 2.0, "{} {:?}", p.length, p.legs[0].points);
    assert_eq!(p.legs[0].points.len(), 2, "{:?}", p.legs[0].points);
    let meshes: HashSet<u32> = p.legs[0].corridor.iter().map(|&t| g.tris[t as usize].mesh).collect();
    assert_eq!(meshes.len(), 2);
}

#[test]
fn unlinked_navmeshes_are_unreachable_and_ledges_are_opt_in() {
    let a = grid(1, [0.0, 0.0], 2, 2, 100.0, &[], flat);
    let b = grid(2, [200.0, 0.0], 2, 2, 100.0, &[], |_, _| -300.0);
    let g = graph(&[(a.clone(), W), (b.clone(), W)]);
    let from = Place { space: W, pos: [50.0, 50.0, 0.0] };
    let to = Place { space: W, pos: [350.0, 50.0, -300.0] };
    assert!(matches!(g.find_path(from, to, &PathParams::default()), Err(PathError::Unreachable { .. })));
    // The same two joined by a ledge down (and up, back).
    let (mut a, mut b) = (a, b);
    let (ea, eb) = (edges_at(&a, 200.0), edges_at(&b, 200.0));
    for &(at, ae, ..) in &ea {
        link(&mut a, at, ae, 2, eb[0].0, link_kind::LEDGE_DOWN);
    }
    for &(bt, be, ..) in &eb {
        link(&mut b, bt, be, 1, ea[0].0, link_kind::LEDGE_UP);
    }
    let g = graph(&[(a, W), (b, W)]);
    assert!(g.stats.ledge_edges > 0);
    assert!(g.find_path(from, to, &PathParams::default()).is_err());
    let p = g.find_path(from, to, &PathParams { ledges: true, ..Default::default() }).unwrap();
    assert_eq!(p.end().map(|e| e[2]), Some(-300.0));
}

#[test]
fn doors_join_spaces_into_legs() {
    let inside = Space::Interior(0x77);
    let mut a = grid(1, [0.0, 0.0], 4, 4, 100.0, &[], flat);
    let mut b = grid(2, [5000.0, 5000.0], 3, 3, 100.0, &[], |_, _| 50.0);
    // Door 0xD1 next to a's triangle at (350, 350); door 0xD2 next to b's at (5050, 5050).
    let ta = (0..a.triangles.len()).find(|&t| {
        let v = a.triangles[t].v.map(|i| a.vertices[i as usize]);
        v.iter().all(|p| p[0] >= 300.0 && p[1] >= 300.0)
    });
    a.door_links.push(DoorLink { door: 0xD1, triangle: ta.unwrap() as u16 });
    a.triangles[ta.unwrap()].flags |= tri_flags::DOOR;
    b.door_links.push(DoorLink { door: 0xD2, triangle: 0 });
    let meshes = [(a, W), (b, inside)];
    let input: Vec<_> = meshes.iter().map(|(m, s)| (m, Some(0), Some(*s))).collect();
    let mut g = NavGraph::assemble(&input);
    g.join_doors(&input, |door| match door {
        0xD1 => Some((0xD2, [5050.0, 5030.0, 50.0], 0.5, inside)),
        0xD2 => Some((0xD1, [340.0, 340.0, 0.0], 3.0, W)),
        _ => None,
    });
    assert_eq!(g.stats.doors, 2);
    let p = g.find_path(Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: inside, pos: [5250.0, 5250.0, 50.0] }, &PathParams::default()).unwrap();
    assert_eq!(p.legs.len(), 2);
    assert_eq!(p.legs[0].space, W);
    assert_eq!(p.legs[0].door.map(|d| d.door), Some(0xD1));
    assert_eq!(p.legs[1].space, inside);
    assert_eq!(p.legs[1].points[0], [5050.0, 5030.0, 50.0]);
    // Without doors there is no way in.
    assert!(g.find_path(Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: inside, pos: [5250.0, 5250.0, 50.0] }, &PathParams { doors: false, ..Default::default() }).is_err());

    // A walker goes all the way through the door.
    let mut w = Walker::new(W, [50.0, 50.0, 0.0], 0.0, 100.0);
    w.go_to(&g, Place { space: inside, pos: [5250.0, 5250.0, 50.0] }, &PathParams::default()).unwrap();
    let mut events = Vec::new();
    for _ in 0..2000 {
        match w.update(&g, 0.05) {
            WalkEvent::Moved => {}
            e => events.push(e),
        }
        if w.follower.is_none() {
            break;
        }
    }
    assert!(matches!(events[0], WalkEvent::UsedDoor(_)), "{events:?}");
    assert_eq!(events.last(), Some(&WalkEvent::Arrived));
    assert_eq!(w.space, inside);
    assert!((w.feet[0] - 5250.0).abs() < 1.0 && (w.feet[1] - 5250.0).abs() < 1.0);
}

#[test]
fn routes_chain_interiors_and_worldspaces_and_respect_locks() {
    use crate::graph::DoorLock;
    // Worldspace W -> interior A -> interior B -> worldspace V, each its own navmesh far
    // apart; a door near each mesh's north-east corner leads on to the next one's
    // south-west corner. B's door on to V is locked with key 0x4B.
    let (a, b, v) = (Space::Interior(0xA), Space::Interior(0xB), Space::World(0x56));
    let spaces = [W, a, b, v];
    let mut meshes: Vec<NavMeshRecord> = (0..4).map(|i| grid(10 + i as u32, [i as f32 * 10_000.0, 0.0], 4, 4, 100.0, &[], flat)).collect();
    let corner = |m: &NavMeshRecord| {
        let x0 = m.vertices[0][0];
        (0..m.triangles.len()).find(|&t| m.triangles[t].v.iter().all(|&i| m.vertices[i as usize][0] >= x0 + 300.0 && m.vertices[i as usize][1] >= 300.0)).unwrap()
    };
    for (i, m) in meshes.iter_mut().enumerate().take(3) {
        let t = corner(m);
        m.door_links.push(DoorLink { door: 0xD0 + i as u32, triangle: t as u16 });
    }
    let input: Vec<_> = meshes.iter().zip(spaces).map(|(m, s)| (m, Some(0), Some(s))).collect();
    let mut g = NavGraph::assemble(&input);
    g.join_doors(&input, |door| {
        let i = (door - 0xD0) as usize;
        let next = (i + 1) as f32 * 10_000.0;
        Some((0xE0 + i as u32, [next + 50.0, 50.0, 0.0], 0.0, spaces[i + 1]))
    });
    g.set_door_lock(0xD2, Some(DoorLock { level: 255, key: Some(0x4B) }));
    assert_eq!(g.stats.doors, 3);
    let from = Place { space: W, pos: [50.0, 50.0, 0.0] };
    let to = Place { space: v, pos: [30_350.0, 350.0, 0.0] };
    let open = PathParams::default();
    // Locked without the key: no way through.
    assert!(!g.can_reach(from, to, &open));
    assert!(matches!(g.find_path(from, to, &open), Err(PathError::Unreachable { .. })));
    // An interior short of the lock is still reachable, through one door.
    let inside_a = Place { space: a, pos: [10_200.0, 200.0, 0.0] };
    assert_eq!(g.door_route(from, inside_a, &open).unwrap().len(), 1);
    // With the key: three doors, four legs, ending in the other worldspace.
    let keyed = PathParams { keys: vec![0x4B], ..Default::default() };
    let route = g.door_route(from, to, &keyed).unwrap();
    assert_eq!(route.iter().map(|d| d.door).collect::<Vec<_>>(), vec![0xD0, 0xD1, 0xD2]);
    let p = g.find_path(from, to, &keyed).unwrap();
    assert_eq!(p.legs.iter().map(|l| l.space).collect::<Vec<_>>(), spaces.to_vec());
    assert_eq!(p.end(), Some([30_350.0, 350.0, 0.0]));
    // Unlocking the door at run time opens the way for anyone.
    g.set_door_lock(0xD2, None);
    assert!(g.can_reach(from, to, &open));
}

#[test]
fn walker_follows_the_surface_on_a_slope() {
    let slope = |x: f32, _y: f32| x * 0.25;
    let g = graph(&[(grid(1, [0.0, 0.0], 8, 8, 100.0, &[(3, 3), (3, 4), (4, 3), (4, 4)], slope), W)]);
    let mut w = Walker::new(W, [50.0, 50.0, 12.5], 0.0, 150.0);
    w.go_to(&g, Place { space: W, pos: [750.0, 750.0, 187.5] }, &PathParams::default()).unwrap();
    let mut steps = 0;
    while w.follower.is_some() && steps < 10_000 {
        w.update(&g, 1.0 / 60.0);
        steps += 1;
        assert!((w.feet[2] - slope(w.feet[0], w.feet[1])).abs() < 0.5, "off the surface at {:?}", w.feet);
        assert!(w.tri == NONE || g.contains_xy(w.tri, w.feet[0], w.feet[1]));
    }
    assert!(w.follower.is_none(), "did not arrive");
    assert!((w.feet[0] - 750.0).abs() < 1.0 && (w.feet[1] - 750.0).abs() < 1.0, "{:?}", w.feet);
    // It went round the hole in the middle, so further than the straight line.
    assert!(w.travelled > 700.0 * std::f32::consts::SQRT_2);
}

#[test]
fn disabled_navmesh_blocks_paths() {
    let mut a = grid(1, [0.0, 0.0], 2, 2, 100.0, &[], flat);
    let mut b = grid(2, [200.0, 0.0], 2, 2, 100.0, &[], flat);
    seam(&mut a, &mut b, link_kind::PORTAL);
    let mut g = graph(&[(a, W), (b, W)]);
    let (from, to) = (Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: W, pos: [350.0, 150.0, 0.0] });
    assert_eq!(g.find_path(from, to, &PathParams::default()).unwrap().goal_snap, 0.0);
    assert!(g.set_enabled(2, false));
    // The goal is no longer on a usable triangle: it snaps 150 units onto the first
    // navmesh's edge, and the path says so.
    assert_eq!(g.locate(W, to.pos, 10.0), None);
    let p = g.find_path(from, to, &PathParams::default()).unwrap();
    assert!((p.goal_snap - 150.0).abs() < 0.5, "{}", p.goal_snap);
    assert!(!g.set_enabled(9, false));
}
