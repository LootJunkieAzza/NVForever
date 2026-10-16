//! Effect shaders on references (EFSH, docs/spec-notes/effect-shaders.md) and actors'
//! alpha (`SetActorAlpha`).
//!
//! - **What plays**: the shaders scripts play (`PlayMagicShaderVisuals`,
//!   `fnv_game::functions::misc::ScriptLooks`) and those of the magic effects active on
//!   actors (MGEF "effect shader", for as long as the effect lasts).
//! - **Drawing**: the membrane only (§4 of the notes). Every mesh drawn for the reference
//!   gets up to two overlays drawn on the same surface (its mesh, and its skin when it
//!   has one): the fill (texture × colour, panned by the texture speed) and the edge
//!   (the edge colour, faded in towards the outline by the NIF material's view-angle
//!   falloff). Both are unlit, cast no shadow and sit a decal's depth step in front of
//!   the surface. Their alpha follows the record's cycle every frame; a mesh drawn later
//!   (a cell loaded again, armour put on) gets its overlays then.
//! - **Ending**: a script's play with a duration ends after max(duration, one cycle),
//!   without one when stopped; an effect's when the effect ends.
//! - **Actor alpha**: the meshes of an actor with an alpha below 1 get blended copies of
//!   their materials with the alpha multiplied in; 1 gives the originals back.
//!
//! Log: `effect shader: <EDID> on <who> (…)` when one starts, `… ends (…)` when it ends,
//! `actor alpha: <who> <a> (<n> meshes)`, and at exit `effect shaders summary: …`.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::prelude::*;
use fnv_game::data::MagicEffectDef;
use fnv_sim::FormId;
use fnv_world::effect_shader::{blend, EffectShader};
use nvf_assets::model::ModelCtx;
use nvf_assets::nif_material::nif_material;
use nvf_assets::NifMaterial;
use nvf_play::script::ScriptState;
use nvf_play::RefVisual;
use std::collections::{HashMap, HashSet};

use crate::world::WorldData;

pub struct EffectShadersPlugin;

impl Plugin for EffectShadersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shaders>().add_systems(Update, (play, actor_alpha)).add_systems(Last, report);
    }
}

/// What a play comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Source {
    /// A script's play (its serial).
    Script(u64),
    /// An active magic effect (its id).
    Effect(u32),
}

/// A shader playing on a reference.
struct Active {
    on: u32,
    shader: Box<EffectShader>,
    /// `Time::elapsed_secs` when it started.
    since: f32,
    /// Ends at this many seconds from `since`; `None`: until stopped.
    until: Option<f32>,
    /// The persistent part shows after the cycle.
    persist: bool,
    fill: Option<Handle<NifMaterial>>,
    edge: Option<Handle<NifMaterial>>,
    /// The meshes covered, and their overlays.
    covered: HashMap<Entity, Vec<Entity>>,
}

/// Put on an overlay mesh (not covered itself, not faded by actor alpha).
#[derive(Component)]
struct Overlay;

/// On a mesh drawn see-through for its actor's alpha: its own material and the alpha.
#[derive(Component)]
struct Faded {
    original: Handle<NifMaterial>,
    alpha: f32,
}

#[derive(Resource, Default)]
struct Shaders {
    records: HashMap<u32, Option<Box<EffectShader>>>,
    active: HashMap<Source, Active>,
    /// Blended copies of materials for actor alpha: (original, alpha in 1/256) → copy.
    faded: HashMap<(AssetId<NifMaterial>, u16), Handle<NifMaterial>>,
    played: u32,
    ended: u32,
    overlays: u32,
    alpha_calls: u32,
}

fn linear(c: [u8; 3]) -> LinearRgba {
    Color::srgb_u8(c[0], c[1], c[2]).to_linear()
}

/// How a membrane blends onto what lies behind it, from its destination blend (notes §4).
fn alpha_mode(s: &EffectShader) -> AlphaMode {
    match s.dest_blend {
        blend::SOURCE_INVERTED_ALPHA => AlphaMode::Blend,
        blend::DEST_COLOR => AlphaMode::Multiply,
        blend::ONE | blend::SOURCE_ALPHA => AlphaMode::Add,
        _ => AlphaMode::Add,
    }
}

/// The view-angle falloff of the edge: none facing the camera, half at cos θ =
/// 1 − 0.5^(1/fall off), a linear ramp through it (notes §4, design).
fn edge_falloff(fall_off: f32) -> Vec4 {
    let f = if fall_off.is_finite() && fall_off > 0.01 { fall_off } else { 1.0 };
    let half = 1.0 - 0.5f32.powf(1.0 / f);
    Vec4::new(1.0, 2.0 * half - 1.0, 0.0, 1.0)
}

fn overlay_material(s: &EffectShader, color: [u8; 3], texture: Option<Handle<Image>>, falloff: Option<Vec4>) -> NifMaterial {
    let mut m = nif_material(StandardMaterial {
        base_color: Color::from(linear(color)).with_alpha(0.0),
        base_color_texture: texture,
        unlit: true,
        alpha_mode: alpha_mode(s),
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    m.extension.decal = true;
    m.extension.hard_edges = true;
    if let Some(f) = falloff {
        m.extension.falloff = f;
    }
    m
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn play(
    mut commands: Commands,
    time: Res<Time>,
    mut script: Option<ResMut<ScriptState>>,
    world: Option<Res<WorldData>>,
    mut models: ModelCtx,
    mut st: ResMut<Shaders>,
    drawn: Query<(Entity, &RefVisual, &Mesh3d, Option<&SkinnedMesh>), (With<MeshMaterial3d<NifMaterial>>, Without<Overlay>)>,
) {
    let (Some(rt), Some(world)) = (script.as_deref_mut().and_then(|s| s.rt.as_mut()), world) else { return };
    let st = &mut *st;
    let now = time.elapsed_secs();
    let game = &mut rt.game;

    // What should play: (source, reference, shader, duration, persist).
    let mut want: Vec<(Source, u32, u32, Option<f64>, bool)> =
        game.script_looks.shaders.iter().map(|p| (Source::Script(p.serial), p.on.0, p.shader.0, p.duration, true)).collect();
    for e in &game.effects {
        let Some(m) = game.data.magic_effects.get(&e.effect) else { continue };
        if let Some(sh) = m.shader {
            want.push((Source::Effect(e.id), e.target.0, sh.0, None, m.has(MagicEffectDef::FX_PERSIST)));
        }
    }
    let wanted: HashSet<Source> = want.iter().map(|w| w.0).collect();

    // Ends: stopped, the effect over, or its time up.
    let mut ended: Vec<(Source, &'static str)> = st.active.keys().filter(|k| !wanted.contains(k)).map(|k| (*k, "stopped")).collect();
    for (k, a) in &st.active {
        if a.until.is_some_and(|u| now - a.since >= u) {
            ended.push((*k, "its time is up"));
        }
    }
    let mut gone: HashSet<Source> = HashSet::new();
    for (k, why) in ended {
        let Some(a) = st.active.remove(&k) else { continue };
        gone.insert(k);
        for x in a.covered.values().flatten() {
            commands.entity(*x).try_despawn();
        }
        for h in [a.fill, a.edge].into_iter().flatten() {
            models.materials.remove(&h);
        }
        if let Source::Script(serial) = k {
            game.script_looks.end(serial);
        }
        st.ended += 1;
        info!("effect shader: {} on {} ends ({why})", a.shader.edid, game.data.name(FormId(a.on)));
    }

    // Starts.
    for (k, on, shader, duration, persist) in want {
        if st.active.contains_key(&k) || gone.contains(&k) {
            continue;
        }
        let record = st.records.entry(shader).or_insert_with(|| fnv_world::effect_shader::effect_shader(world.lo, world.schema, shader).map(Box::new)).clone();
        let Some(s) = record else {
            // Not an EFSH: nothing to draw; a script's play is let go.
            if let Source::Script(serial) = k {
                warn!("effect shader: {shader:08X} is not an effect shader");
                game.script_looks.end(serial);
            }
            continue;
        };
        let texture = s.fill_texture.as_deref().and_then(|t| {
            let mut path = t.replace('/', "\\").to_ascii_lowercase();
            if !path.starts_with("textures\\") {
                path = format!("textures\\{path}");
            }
            let a = &mut models;
            let image = a.textures.get(&path, true, &a.data, &mut a.images, a.formats.as_deref());
            if image.is_none() {
                warn!("effect shader: {}: fill texture {path} not found", s.edid);
            }
            image
        });
        let shows = |c: &fnv_world::effect_shader::AlphaCycle| c.full > 0.0 || (persist && c.persistent > 0.0);
        let fill = (s.has_membrane() && shows(&s.fill)).then(|| models.materials.add(overlay_material(&s, s.fill_color, texture, None)));
        let edge = (s.has_membrane() && shows(&s.edge)).then(|| models.materials.add(overlay_material(&s, s.edge_color, None, Some(edge_falloff(s.edge_fall_off)))));
        let until = duration.map(|d| (d as f32).max(s.cycle()));
        st.played += 1;
        info!(
            "effect shader: {} on {} ({}{}, {})",
            s.edid,
            game.data.name(FormId(on)),
            if fill.is_some() { "fill" } else { "no fill" },
            if edge.is_some() { " and edge" } else { "" },
            match (until, k) {
                (Some(u), _) => format!("for {u:.1} s"),
                (None, Source::Script(_)) => "until stopped".to_string(),
                (None, Source::Effect(_)) => "while the effect lasts".to_string(),
            }
        );
        st.active.insert(k, Active { on, shader: s, since: now, until, persist, fill, edge, covered: HashMap::new() });
    }

    // Cover the meshes drawn for each reference; animate the materials.
    let mut by_ref: HashMap<u32, Vec<(Entity, &Mesh3d, Option<&SkinnedMesh>)>> = HashMap::new();
    for (e, rv, mesh, skin) in &drawn {
        by_ref.entry(rv.0).or_default().push((e, mesh, skin));
    }
    for a in st.active.values_mut() {
        let meshes = by_ref.get(&a.on).map(Vec::as_slice).unwrap_or(&[]);
        a.covered.retain(|e, _| meshes.iter().any(|(m, ..)| m == e));
        let before = a.covered.len();
        for &(e, mesh, skin) in meshes {
            if a.covered.contains_key(&e) {
                continue;
            }
            let mut overlays = Vec::new();
            for mat in [&a.fill, &a.edge].into_iter().flatten() {
                let mut c = commands.spawn((Mesh3d(mesh.0.clone()), MeshMaterial3d(mat.clone()), Transform::IDENTITY, Overlay, NoFrustumCulling, bevy::light::NotShadowCaster));
                if let Some(skin) = skin {
                    c.insert(skin.clone());
                }
                let x = c.id();
                commands.entity(e).add_child(x);
                overlays.push(x);
            }
            st.overlays += overlays.len() as u32;
            a.covered.insert(e, overlays);
        }
        if a.covered.len() != before && before == 0 && !a.covered.is_empty() {
            info!("effect shader: {} covers {} meshes of {}", a.shader.edid, a.covered.len(), game.data.name(FormId(a.on)));
        }
        let t = now - a.since;
        if let Some(mut m) = a.fill.as_ref().and_then(|h| models.materials.get_mut(h)) {
            m.base.base_color.set_alpha(a.shader.fill.alpha(t, a.persist));
            let [u, v] = a.shader.fill_speed;
            m.base.uv_transform = bevy::math::Affine2::from_translation(Vec2::new((u * t).fract(), (v * t).fract()));
        }
        if let Some(mut m) = a.edge.as_ref().and_then(|h| models.materials.get_mut(h)) {
            m.base.base_color.set_alpha(a.shader.edge.alpha(t, a.persist));
        }
    }
}

/// Draw actors with an alpha below 1 see-through; give the others their materials back.
#[allow(clippy::type_complexity)]
fn actor_alpha(
    mut commands: Commands,
    script: Option<Res<ScriptState>>,
    mut st: ResMut<Shaders>,
    mut materials: ResMut<Assets<NifMaterial>>,
    drawn: Query<(Entity, &RefVisual, &MeshMaterial3d<NifMaterial>, Option<&Faded>), Without<Overlay>>,
) {
    let Some(rt) = script.as_deref().and_then(|s| s.rt.as_ref()) else { return };
    let alpha = &rt.game.script_looks.alpha;
    let st = &mut *st;
    let mut changed: HashMap<u32, (f32, u32)> = HashMap::new();
    for (e, rv, mat, faded) in &drawn {
        let want = alpha.get(&FormId(rv.0)).copied();
        match (want, faded) {
            (None, None) => {}
            (None, Some(f)) => {
                commands.entity(e).insert(MeshMaterial3d(f.original.clone())).remove::<Faded>();
                changed.entry(rv.0).or_insert((1.0, 0)).1 += 1;
            }
            (Some(a), Some(f)) if f.alpha == a => {}
            (Some(a), f) => {
                let original = f.map(|f| f.original.clone()).unwrap_or_else(|| mat.0.clone());
                let key = (original.id(), (a * 256.0).round() as u16);
                let copy = match st.faded.get(&key) {
                    Some(h) => h.clone(),
                    None => {
                        let Some(mut m) = materials.get(&original).cloned() else { continue };
                        // A cut-out keeps its cut while it blends.
                        if let AlphaMode::Mask(cut) = m.base.alpha_mode {
                            m.extension.alpha_cut = cut * a;
                        }
                        if matches!(m.base.alpha_mode, AlphaMode::Opaque | AlphaMode::Mask(_)) {
                            m.base.alpha_mode = AlphaMode::Blend;
                        }
                        let c = m.base.base_color.alpha();
                        m.base.base_color.set_alpha(c * a);
                        let h = materials.add(m);
                        st.faded.insert(key, h.clone());
                        h
                    }
                };
                commands.entity(e).insert((MeshMaterial3d(copy), Faded { original, alpha: a }));
                changed.entry(rv.0).or_insert((a, 0)).1 += 1;
            }
        }
    }
    for (r, (a, n)) in changed {
        st.alpha_calls += 1;
        info!("actor alpha: {} {a:.2} ({n} meshes)", rt.game.data.name(FormId(r)));
    }
}

fn report(mut exit: MessageReader<AppExit>, st: Res<Shaders>) {
    if exit.read().next().is_none() {
        return;
    }
    info!(
        "effect shaders summary: {} played, {} ended, {} overlays drawn, {} actor alpha changes",
        st.played, st.ended, st.overlays, st.alpha_calls
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvf_assets::nif_material::Falloff;

    #[test]
    fn edge_is_half_strong_where_the_fall_off_says() {
        for f in [0.5f32, 0.95, 1.0, 2.5] {
            let p = Falloff { params: edge_falloff(f) };
            let half = 1.0 - 0.5f32.powf(1.0 / f);
            assert!((p.opacity(half) - 0.5).abs() < 1e-4, "fall off {f}");
            assert_eq!(p.opacity(1.0), 0.0);
        }
    }
}
