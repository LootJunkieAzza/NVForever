//! Diagnostic: the cost of posing one actor from one clip and from two blended clips
//! (`BlendPlayer::sample`, docs/spec-notes/actors.md §8.6), without Bevy's transform
//! writes.
//!
//! cargo run --release -p nvf-assets --example blend_bench -- <Data dir> <nif schema dir>

use bevy::prelude::Transform;
use nvf_assets::anim_play::{BlendPlayer, BoundClip, Clip, Layer};
use nvf_assets::data::GameData;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = GameData::open(a[1].as_ref(), a[2].as_ref()).expect("game data");
    let load = |p: &str| {
        let bytes = data.read(p).expect(p);
        let file = fnv_nif::NifFile::parse(&bytes, Some(&data.schema)).expect(p);
        let dec = fnv_nif::scene::Decoded::new(&file, &bytes, &data.schema);
        Arc::new(Clip::from_sequence(&fnv_nif::anim::sequences(&file, &dec)[0]))
    };
    let dir = r"meshes\characters\_male\locomotion";
    let stance = r"meshes\characters\_male";
    let clips = [
        load(&format!(r"{dir}\mtidle.kf")),
        load(&format!(r"{dir}\male\mtforward.kf")),
        load(&format!(r"{dir}\male\mtfastforward.kf")),
        load(&format!(r"{stance}\2hraim.kf")),
        load(&format!(r"{stance}\2hrequip.kf")),
        load(&format!(r"{stance}\2hrholster.kf")),
    ];
    // Node indices by track name (1 + order of first appearance; 0 is the actor's root).
    let mut names: HashMap<String, usize> = HashMap::new();
    for c in &clips {
        for (n, _) in &c.tracks {
            let next = names.len() + 1;
            names.entry(n.clone()).or_insert(next);
        }
    }
    let bound: Vec<Arc<BoundClip>> = clips.iter().map(|c| Arc::new(BoundClip::bind(c.clone(), |n| names.get(n).copied()))).collect();
    let n = names.len() + 1;
    let nodes = (0..n as u32).map(|i| bevy::prelude::Entity::from_raw_u32(i + 1).unwrap()).collect();
    let mut player = BlendPlayer::new(nodes, Arc::new(vec![Transform::IDENTITY; n]));
    let mut out = Vec::with_capacity(n);
    let frames = 20_000;
    let mut run = |label: &str, layers: [Option<(usize, f32)>; 4]| {
        let t0 = Instant::now();
        let mut tracks = 0;
        for i in 0..frames {
            let phase = i as f32 / frames as f32;
            player.layers = Default::default();
            for (slot, l) in player.layers.iter_mut().zip(layers) {
                *slot = l.map(|(k, weight)| Layer { clip: bound[k].clone(), phase, weight });
            }
            tracks += player.sample(&mut out);
        }
        let per = t0.elapsed().as_secs_f64() / f64::from(frames);
        println!("{label}: {:.2} us per pose, {} tracks per pose", per * 1e6, tracks / frames as usize);
    };
    run("idle (one clip)", [Some((0, 1.0)), None, None, None]);
    run("walk (one clip)", [Some((1, 1.0)), None, None, None]);
    run("idle -> walk (two clips)", [Some((1, 0.5)), Some((0, 0.5)), None, None]);
    run("walk -> run (two clips)", [Some((2, 0.5)), Some((1, 0.5)), None, None]);
    run("idle + holstered rifle (two clips)", [Some((0, 1.0)), None, Some((5, 1.0)), None]);
    run("walk + drawn rifle (two clips)", [Some((1, 1.0)), None, Some((3, 1.0)), None]);
    run("walk -> run + equip -> aim (four clips)", [Some((2, 0.5)), Some((1, 0.5)), Some((3, 0.5)), Some((4, 0.5))]);
}
