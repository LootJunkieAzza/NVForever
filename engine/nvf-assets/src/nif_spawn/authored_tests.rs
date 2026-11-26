//! The authored material branch of `shape_material_with` (#2981): the fields a set fills,
//! what it leaves, the shapes it never touches, and the legacy look kept for swaps.

use super::*;
use crate::authored::{AuthoredSet, LegacyLook, Parallax, PbrSet, AUTHORED_MARK};
use crate::texture::bc5_tests::triangle;
use fnv_pbr::Scalars;

/// Textures as handles made on demand, one per path, with an authored set for one key.
struct SetTextures {
    images: Assets<Image>,
    by_path: HashMap<String, Handle<Image>>,
    /// The set and the resolved key it belongs to.
    set: Option<(String, PbrSet)>,
    /// The game's normal maps carry a specular mask.
    masked: bool,
    asked: Vec<String>,
    /// The run's setting.
    on: bool,
}

impl SetTextures {
    fn new(set: Option<(&str, AuthoredSet)>) -> SetTextures {
        SetTextures::with(set.map(|(k, s)| (k, PbrSet::Authored(s))))
    }

    fn with(set: Option<(&str, PbrSet)>) -> SetTextures {
        SetTextures { images: Assets::default(), by_path: HashMap::new(), set: set.map(|(k, s)| (k.to_string(), s)), masked: true, asked: Vec::new(), on: true }
    }

    fn handle(&mut self, path: &str) -> Handle<Image> {
        let key = fnv_vfs::texture_path(path);
        if let Some(h) = self.by_path.get(&key) {
            return h.clone();
        }
        let h = self.images.add(Image::default());
        self.by_path.insert(key, h.clone());
        h
    }
}

impl TextureSource for SetTextures {
    fn texture(&mut self, raw_path: &str, _: bool) -> Option<Handle<Image>> {
        Some(self.handle(raw_path))
    }
    fn normal(&mut self, raw_path: &str) -> Option<(Handle<Image>, bool)> {
        Some((self.handle(raw_path), !self.masked))
    }
    fn lacks_alpha(&self, _: &Handle<Image>) -> bool {
        !self.masked
    }
    fn alpha_is_mask(&self, raw_path: &str) -> bool {
        raw_path.contains("mask")
    }
    fn alpha_is_opaque(&self, _: &str) -> bool {
        false
    }
    fn resolve(&self, raw_path: &str) -> String {
        fnv_vfs::texture_path(raw_path)
    }
    fn authored_on(&self) -> bool {
        self.on
    }
    fn pbr_set(&mut self, key: &str) -> Option<PbrSet> {
        self.asked.push(key.to_string());
        self.set.as_ref().filter(|(k, _)| k == key).map(|(_, s)| s.clone())
    }
}

const KEY: &str = r"textures\walls\brick.dds";

fn set(images: &mut Assets<Image>, normal: bool, scalars: Scalars) -> AuthoredSet {
    AuthoredSet { stem: r"textures\pbr\walls\brick".into(), orm: images.add(Image::default()), normal: normal.then(|| images.add(Image::default())), height: None, scalars, metal_gain: 7.0 }
}

/// A lit, masked, glossy shape with the game's diffuse and normal map.
fn wall() -> Props {
    Props { textures: vec![r"walls\brick.dds".into(), r"walls\brick_n.dds".into()], specular: true, glossiness: Some(50.0), ..Default::default() }
}

/// Everything a material draws with, as text (the handles included), for field-by-field
/// comparison.
fn drawn(m: &NifMaterial) -> String {
    let mut e = m.extension.clone();
    e.legacy = None;
    format!("{:?} {:?}", m.base, e)
}

#[test]
fn an_authored_set_fills_its_fields_over_the_legacy_ones() {
    let mut scratch = Assets::<Image>::default();
    let s = set(&mut scratch, true, Scalars { parallax_depth: None, roughness_scale: Some(0.7), reflectance: Some(0.3), metal_gain: None });
    let mut source = SetTextures::new(Some((KEY, s.clone())));
    let mut mesh = triangle();
    let (m, missing) = shape_material_with(&wall(), &mut mesh, &mut source, Tangents::Generate);
    assert!(missing.is_none());
    assert_eq!(source.asked, [KEY], "looked up once, by the resolved diffuse");
    let (diffuse, game_normal) = (source.handle(r"walls\brick.dds"), source.handle(r"walls\brick_n.dds"));
    let b = &m.base;
    assert_eq!((b.metallic_roughness_texture.as_ref(), b.occlusion_texture.as_ref()), (Some(&s.orm), Some(&s.orm)), "one ORM in both slots");
    assert!(b.specular_texture.is_none(), "the normal map's alpha is no mask any more");
    assert_eq!((b.perceptual_roughness, b.reflectance, b.metallic), (0.7, 0.3, 0.0));
    assert_eq!(b.normal_map_texture, s.normal);
    assert_eq!(b.base_color_texture.as_ref(), Some(&diffuse), "the base colour stays the game's diffuse");
    assert!(b.flip_normal_map_y && matches!(b.alpha_mode, AlphaMode::Opaque) && !b.unlit);
    assert_eq!(m.extension.emissive_rule, Vec4::new(0.0, 0.0, 0.0, AUTHORED_MARK));
    assert_eq!(m.extension.soft, Vec4::ZERO, "soft stays zero: terrain blending still takes the material");
    assert_eq!(m.extension.metal_gain, s.metal_gain, "the set's metal gain in its lane");
    assert_eq!(
        m.extension.legacy,
        Some(LegacyLook { normal: Some(game_normal.clone()), specular: Some(game_normal), roughness: gloss_roughness(50.0), reflectance: 2.0, metallic: 0.0, tangents: true, specular_tint: Color::WHITE, parallax: Parallax::default(), scripted_tint: None })
    );
}

#[test]
fn defaults_apply_without_scalars_and_the_game_normal_stays_without_an_n() {
    let mut scratch = Assets::<Image>::default();
    let s = set(&mut scratch, false, Scalars::default());
    let mut source = SetTextures::new(Some((KEY, s.clone())));
    let (m, _) = shape_material_with(&wall(), &mut triangle(), &mut source, Tangents::Generate);
    let game_normal = source.handle(r"walls\brick_n.dds");
    assert_eq!((m.base.perceptual_roughness, m.base.reflectance), (1.0, 0.5));
    assert_eq!(m.base.normal_map_texture.as_ref(), Some(&game_normal));
    assert!(m.base.specular_texture.is_none());
    assert_eq!(m.base.metallic_roughness_texture.as_ref(), Some(&s.orm));
}

/// A shape without a normal map that gains one from `_n` gets tangents on the mesh being
/// built, from the caller's tangent source: generated for a new mesh, and for a copy of a
/// drawn mesh (`Tangents::Keep`) its own when it has them. A copy without them still has them
/// generated (as for the game's own normal map), but they never reach the mesh the actor
/// draws, so the legacy look records none and the `_n` is not put on (#2979 review: the
/// same rule as a height map's).
#[test]
fn a_first_normal_from_n_brings_tangents_to_the_mesh() {
    let mut scratch = Assets::<Image>::default();
    let s = set(&mut scratch, true, Scalars::default());
    let bare = Props { textures: vec![r"walls\brick.dds".into()], ..Default::default() };
    let mut source = SetTextures::new(Some((KEY, s.clone())));
    let mut mesh = triangle();
    assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_none());
    let (m, _) = shape_material_with(&bare, &mut mesh, &mut source, Tangents::Generate);
    assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some(), "tangents for the first normal map");
    assert_eq!(m.base.normal_map_texture, s.normal);
    assert_eq!(m.extension.legacy.as_ref().map(|l| (l.normal.is_none(), l.tangents, l.reflectance)), Some((true, true, 0.3)));
    let mut copy = triangle();
    let (m, _) = shape_material_with(&bare, &mut copy, &mut source, Tangents::Keep);
    assert!(copy.attribute(Mesh::ATTRIBUTE_TANGENT).is_some(), "a copy without tangents has them generated");
    assert_eq!(m.base.normal_map_texture, None, "the drawn mesh has no tangents: no _n (the game gives no normal map)");
    assert_eq!(m.base.metallic_roughness_texture.as_ref(), Some(&s.orm));
    assert!(!m.extension.legacy.unwrap().tangents, "the drawn mesh's tangents, not the copy's");
    let (m, _) = shape_material_with(&bare, &mut copy, &mut source, Tangents::Keep);
    assert_eq!(m.base.normal_map_texture, s.normal, "a copy that has tangents keeps them and takes the _n");
}

#[test]
fn shapes_a_set_never_touches_keep_the_game_material_and_ask_nothing() {
    let mut scratch = Assets::<Image>::default();
    let s = set(&mut scratch, true, Scalars::default());
    let add = 1 | (6 << 1); // blend, SRC_ALPHA / ONE
    let multiply = 1 | (1 << 1) | (2 << 5); // blend, ZERO / SRC_COLOR
    for (why, props) in [
        ("hair", Props { hair: true, ..wall() }),
        ("skin", Props { skin: true, ..wall() }),
        ("unlit", Props { unlit: true, ..wall() }),
        ("refracting", Props { refraction: Some(0.1), ..wall() }),
        ("additive", Props { alpha: Some((add, 0)), ..wall() }),
        ("multiplied", Props { alpha: Some((multiply, 0)), ..wall() }),
        ("excluded by its caller", Props { no_authored: true, ..wall() }),
    ] {
        let mut source = SetTextures::new(Some((KEY, s.clone())));
        let (m, _) = shape_material_with(&props, &mut triangle(), &mut source, Tangents::Generate);
        assert!(source.asked.is_empty(), "{why}: no lookup");
        assert!(m.base.metallic_roughness_texture.is_none() && m.base.occlusion_texture.is_none(), "{why}");
        assert_ne!(m.extension.emissive_rule.w, AUTHORED_MARK, "{why}");
        assert!(m.extension.legacy.is_none(), "{why}: no baseline either");
    }
    for key in [r"textures\pipboy3000\pc\english\pipboyarm01.dds", r"textures\interface\icons\x.dds"] {
        let mut source = SetTextures::new(Some((key, s.clone())));
        let props = Props { textures: vec![key.into()], ..wall() };
        let (m, _) = shape_material_with(&props, &mut triangle(), &mut source, Tangents::Generate);
        assert!(source.asked.is_empty() && m.base.metallic_roughness_texture.is_none(), "{key}");
    }
    // No UVs: the maps could not be read.
    let mut source = SetTextures::new(Some((KEY, s.clone())));
    let mut no_uv = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::MAIN_WORLD);
    no_uv.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
    assert!(shape_material_with(&wall(), &mut no_uv, &mut source, Tangents::Generate).0.base.metallic_roughness_texture.is_none());
}

/// The drawn fields (everything but the CPU-only legacy look) of a material with the run's
/// setting off, with it on and no set, and of an authored material put back by its legacy
/// look are the same. Off, the material carries nothing new: no legacy look, no lookup.
/// The legacy looks of the no-set and the authored builds agree but for `tangents` where the
/// set's `_n` gave a shape without a normal map tangents on its drawn mesh.
#[test]
fn off_no_set_and_restored_materials_draw_the_same() {
    let mut scratch = Assets::<Image>::default();
    let s = set(&mut scratch, true, Scalars { parallax_depth: None, roughness_scale: Some(0.2), reflectance: Some(0.9), metal_gain: None });
    let bare = Props { textures: vec![wall().textures[0].clone()], ..wall() };
    for (props, mask, gains_tangents) in [(wall(), true, false), (bare, false, true), (wall(), false, false)] {
        let mut source = SetTextures::new(Some((KEY, s.clone())));
        source.masked = mask;
        source.on = false;
        let (off, _) = shape_material_with(&props, &mut triangle(), &mut source, Tangents::Generate);
        assert!(source.asked.is_empty() && off.extension.legacy.is_none(), "off: no lookup, no legacy look");
        source.on = true;
        let set = source.set.take();
        let (no_set, _) = shape_material_with(&props, &mut triangle(), &mut source, Tangents::Generate);
        source.set = set;
        let (mut authored, _) = shape_material_with(&props, &mut triangle(), &mut source, Tangents::Generate);
        assert_eq!(drawn(&off), drawn(&no_set), "{props:?}");
        assert_ne!(drawn(&off), drawn(&authored));
        let look = authored.extension.legacy.clone().unwrap();
        crate::authored::restore(&mut authored, &look);
        assert_eq!(drawn(&off), drawn(&authored), "{props:?}");
        let mut no_set_look = no_set.extension.legacy.clone().unwrap();
        assert_eq!(look.tangents, no_set_look.tangents || gains_tangents, "{props:?}");
        no_set_look.tangents = look.tangents;
        assert_eq!(no_set_look, look, "{props:?}");
    }
}

/// The cut-out classification still comes from the game's diffuse.
#[test]
fn masks_and_cut_outs_stay_the_game_diffuses() {
    let mut scratch = Assets::<Image>::default();
    let key = r"textures\landscape\plants\maize_mask.dds";
    let s = set(&mut scratch, true, Scalars::default());
    let corn = Props { textures: vec![key.into()], alpha: Some((0x12ED, 0)), ..Default::default() };
    let mut source = SetTextures::new(Some((key, s.clone())));
    let (m, _) = shape_material_with(&corn, &mut triangle(), &mut source, Tangents::Generate);
    assert_eq!(m.extension.blended_cutout, Some(BLENDED_CUTOUT_CUT));
    assert!(matches!(m.base.alpha_mode, AlphaMode::Blend));
    assert_eq!(m.base.metallic_roughness_texture.as_ref(), Some(&s.orm));
}

/// Through the real synchronous source and a generated pack: the plain texture's set applies;
/// a height-only set (a first-person twin's own included, which wins over the plain set)
/// leaves the shape the game's but for its height map, and an `_orm` that does not load
/// leaves it legacy.
#[test]
fn the_real_source_applies_sets_and_falls_back_to_legacy() {
    let pack = crate::authored::tests::Pack::new();
    let mut cache = TextureCache::default();
    let mut images = Assets::default();
    let mut build = |diffuse: &str| shape_material(&Props { textures: vec![diffuse.into()], ..Default::default() }, &mut triangle(), &mut cache, &pack.data, &mut images, None).0;
    let gun = build(r"weapons\gun.dds");
    let twin = build(r"weapons\1stpersongun.dds");
    let brick = build(r"walls\brick.dds");
    let stone = build(r"walls\stone.dds");
    let bad = build(r"walls\bad.dds");
    let plain = build(r"walls\plain.dds");
    assert!(gun.base.metallic_roughness_texture.is_some() && gun.extension.emissive_rule.w == AUTHORED_MARK);
    assert!(brick.base.normal_map_texture.is_some() && brick.base.reflectance == 0.25 && brick.base.perceptual_roughness == 0.5);
    for (name, m) in [("height only", &stone), ("height-only twin", &twin), ("invalid _orm", &bad), ("no set", &plain)] {
        assert!(m.base.metallic_roughness_texture.is_none() && m.extension.emissive_rule.w == 0.0, "{name}");
        assert_eq!(m.base.reflectance, 0.3, "{name}: the legacy value");
    }
    // Stage 4: the height-only sets add their height map, nothing else.
    assert!(stone.base.depth_map.is_some() && twin.base.depth_map.is_some());
    assert!(bad.base.depth_map.is_none() && plain.base.depth_map.is_none() && gun.base.depth_map.is_none());
}

/// A height map (#2981 stage 4) for the test sets.
fn height(images: &mut Assets<Image>, depth: f32) -> Parallax {
    Parallax::height(images.add(Image::default()), depth)
}

/// A height-only set: the game's own material with the depth map and its settings, nothing
/// else; the mark stays clear and the legacy look keeps the no-depth-map baseline. Tangents
/// are made for it on the drawn mesh of a shape without a normal map.
#[test]
fn a_height_only_set_adds_its_height_map_to_the_game_material_only() {
    let mut scratch = Assets::<Image>::default();
    let h = height(&mut scratch, 0.04);
    let bare = Props { textures: vec![r"walls\brick.dds".into()], ..Default::default() };
    for props in [wall(), bare] {
        let mut source = SetTextures::with(Some((KEY, PbrSet::HeightOnly(crate::authored::HeightSet { stem: r"textures\pbr\walls\brick".into(), height: h.clone() }))));
        let mut mesh = triangle();
        let (m, _) = shape_material_with(&props, &mut mesh, &mut source, Tangents::Generate);
        assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some(), "{props:?}: tangents on the drawn mesh");
        assert_eq!(Parallax::of(&m.base), h, "{props:?}");
        assert_ne!(m.extension.emissive_rule.w, AUTHORED_MARK, "the mark stays clear: scripted gloss still applies");
        assert!(m.base.metallic_roughness_texture.is_none() && m.base.occlusion_texture.is_none());
        let legacy = m.extension.legacy.clone().unwrap();
        assert_eq!(legacy.parallax, Parallax::default(), "the baseline has no depth map");
        assert!(legacy.tangents && crate::authored::height_on(&m));
        // Off, and on without the set: the same game material, without the height map.
        source.set = None;
        let (no_set, _) = shape_material_with(&props, &mut triangle(), &mut source, Tangents::Generate);
        let mut stripped = m.clone();
        Parallax::default().put(&mut stripped.base);
        stripped.extension.height_overlay = false;
        assert_eq!(drawn(&stripped), drawn(&no_set), "{props:?}: the height map is the only difference");
    }
}

/// An authored set with `_p`: both on; [`crate::authored::restore`] takes both off.
#[test]
fn an_authored_height_map_comes_off_with_the_set() {
    let mut scratch = Assets::<Image>::default();
    let mut s = set(&mut scratch, true, Scalars::default());
    s.height = Some(height(&mut scratch, 0.06));
    let mut source = SetTextures::new(Some((KEY, s.clone())));
    let (mut m, _) = shape_material_with(&wall(), &mut triangle(), &mut source, Tangents::Generate);
    assert!(crate::authored::is_authored(&m));
    assert_eq!(m.base.depth_map, s.height.as_ref().unwrap().depth_map);
    assert_eq!(m.base.parallax_depth_scale, 0.06);
    source.on = false;
    let (off, _) = shape_material_with(&wall(), &mut triangle(), &mut source, Tangents::Generate);
    let look = m.extension.legacy.clone().unwrap();
    crate::authored::restore(&mut m, &look);
    assert_eq!(drawn(&off), drawn(&m), "depth map, depth, method and layers back too");
}

/// The alpha-tested switch controls cutouts. Blends, double-sided shapes, decals, and copies
/// of meshes without tangents cannot take the height map; they keep the rest of the set.
#[test]
fn height_map_eligibility_honours_the_mask_switch_and_shape_constraints() {
    let mut scratch = Assets::<Image>::default();
    let mut s = set(&mut scratch, false, Scalars::default());
    s.height = Some(height(&mut scratch, 0.03));
    let bare = || Props { textures: vec![r"walls\brick.dds".into()], ..Default::default() };
    use crate::authored::HeightSkip;
    let masked = crate::authored::parallax_mask();
    for (why, props, eligibility) in [
        // NiAlphaProperty 0x12EC: tested, not blended; 0x10ED: blended, not tested.
        ("alpha tested", Props { alpha: Some((0x12EC, 127)), ..bare() }, if masked { Ok(()) } else { Err(HeightSkip::AlphaTested) }),
        ("blended", Props { alpha: Some((0x10ED, 0)), ..bare() }, Err(HeightSkip::Blended)),
        ("double-sided", Props { double_sided: true, ..bare() }, Err(HeightSkip::DoubleSided)),
        ("decal", Props { decal: true, ..bare() }, Err(HeightSkip::Decal)),
    ] {
        let mut source = SetTextures::new(Some((KEY, s.clone())));
        let mut mesh = triangle();
        let (m, _) = shape_material_with(&props, &mut mesh, &mut source, Tangents::Generate);
        assert_eq!(crate::authored::height_gate(&m, true), eligibility, "{why}: eligibility matches the draw");
        assert_eq!(m.base.depth_map.is_some(), eligibility.is_ok(), "{why}: only eligible shapes take the height map");
        assert!(m.base.metallic_roughness_texture.is_some(), "{why}: the rest of the set applies");
        assert_eq!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some(), eligibility.is_ok(), "{why}: tangents are made for an eligible height map");
    }
    let mut source = SetTextures::new(Some((KEY, s.clone())));
    let mut copy = triangle();
    let (m, _) = shape_material_with(&bare(), &mut copy, &mut source, Tangents::Keep);
    // A copy of a drawn mesh without tangents: tangents made on the copy would never reach the
    // drawn mesh, so the height map stays off and none are made for it (#2979 review).
    assert!(m.base.depth_map.is_none() && copy.attribute(Mesh::ATTRIBUTE_TANGENT).is_none(), "a copy of a drawn mesh without tangents takes no height map");
    assert!(m.base.metallic_roughness_texture.is_some(), "the rest of the set still applies");
    let mut copy = triangle();
    copy.generate_tangents().unwrap();
    let (m, _) = shape_material_with(&bare(), &mut copy, &mut source, Tangents::Keep);
    assert!(m.base.depth_map.is_some(), "a copy of a drawn mesh that has tangents takes it");
    assert_eq!(crate::authored::height_gate(&m, false), Err(crate::authored::HeightSkip::NoTangents), "without tangents on the mesh the gate refuses");
}

/// Later copies (#2981 review A3): a fade's blended copy and a holed copy drop the height map
/// and put the baseline back; an opaque copy keeps it. Role copies made legacy and unlit
/// copies drop it too, with the baseline depth, method and layers.
#[test]
fn copies_that_stop_being_eligible_drop_the_height_map() {
    let mut scratch = Assets::<Image>::default();
    let h = height(&mut scratch, 0.05);
    let mut source = SetTextures::with(Some((KEY, PbrSet::HeightOnly(crate::authored::HeightSet { stem: r"textures\pbr\walls\brick".into(), height: h.clone() }))));
    let (m, _) = shape_material_with(&wall(), &mut triangle(), &mut source, Tangents::Generate);
    assert!(m.base.depth_map.is_some());
    let mut kept = m.clone();
    assert!(!crate::authored::recheck_height(&mut kept), "still opaque");
    assert_eq!(Parallax::of(&kept.base), h);
    let mut fading = m.clone();
    fading.base.alpha_mode = AlphaMode::Blend;
    assert!(crate::authored::recheck_height(&mut fading));
    assert_eq!(Parallax::of(&fading.base), Parallax::default());
    let mut holed = m.clone();
    holed.extension.holes = Vec4::new(40.0, 4.0, 0.0, 0.0);
    holed.base.alpha_mode = AlphaMode::Mask(0.0);
    assert!(crate::authored::recheck_height(&mut holed) && holed.base.depth_map.is_none());
    let mut role = m.clone();
    assert!(crate::authored::make_legacy(&mut role));
    assert_eq!((Parallax::of(&role.base), role.extension.legacy.is_none()), (Parallax::default(), true), "a role copy: the height map off for good");
    let mut unlit = m.clone();
    crate::authored::clear_for_unlit_copy(&mut unlit);
    assert_eq!(Parallax::of(&unlit.base), Parallax::default(), "an unlit copy: no depth map, the baseline's settings");
}

/// #2981 review H3: a cloak's refracting copy (`refraction_material`, the shared step of the
/// player's and actors' see-through copies) drops a height overlay; a refracting copy of a
/// material without one is left as it was.
#[test]
fn a_refracting_copy_drops_the_height_map() {
    let mut scratch = Assets::<Image>::default();
    let h = height(&mut scratch, 0.05);
    let mut source = SetTextures::with(Some((KEY, PbrSet::HeightOnly(crate::authored::HeightSet { stem: r"textures\pbr\walls\brick".into(), height: h }))));
    let (m, _) = shape_material_with(&wall(), &mut triangle(), &mut source, Tangents::Generate);
    assert!(crate::authored::height_on(&m) && m.base.depth_map.is_some());
    let mut cloak = m.clone();
    refraction_material(&mut cloak, 0.2, 0.0);
    assert!(cloak.extension.refract && cloak.base.depth_map.is_none() && !crate::authored::height_on(&cloak));
    assert_eq!(Parallax::of(&cloak.base), Parallax::default(), "the legacy look's parallax settings back");
    source.set = None;
    let (plain, _) = shape_material_with(&wall(), &mut triangle(), &mut source, Tangents::Generate);
    let mut plain_cloak = plain.clone();
    refraction_material(&mut plain_cloak, 0.2, 0.0);
    assert_eq!(Parallax::of(&plain_cloak.base), Parallax::of(&plain.base));
}

/// #2981 review H5: the unlit cleanup touches the parallax settings only where this module
/// put a height overlay on; a copy's own settings without one stay (only the depth map goes,
/// as in stage 3).
#[test]
fn the_unlit_cleanup_leaves_parallax_settings_it_did_not_put_on() {
    let mut own = crate::nif_material::nif_material(StandardMaterial { parallax_depth_scale: 0.2, max_parallax_layer_count: 32.0, ..default() });
    crate::authored::clear_for_unlit_copy(&mut own);
    assert_eq!((own.base.parallax_depth_scale, own.base.max_parallax_layer_count, own.base.depth_map.is_none()), (0.2, 32.0, true));
    // Restoring a material without an overlay changes no parallax field either.
    let look = LegacyLook { normal: None, specular: None, roughness: 0.5, reflectance: 0.5, metallic: 0.0, tangents: true, specular_tint: Color::WHITE, parallax: Parallax::default(), scripted_tint: None };
    let mut authored = own.clone();
    authored.extension.emissive_rule.w = AUTHORED_MARK;
    crate::authored::restore(&mut authored, &look);
    assert_eq!((authored.base.parallax_depth_scale, authored.base.max_parallax_layer_count), (0.2, 32.0));
}

/// A lit shape whose texture set names a height map in slot 3, with the parallax flag
/// (BSShaderFlags bit 11) when `flag` (#2979).
fn flagstones(flag: bool) -> Props {
    Props {
        textures: vec![r"architecture\mccarran\flagstone.dds".into(), r"architecture\mccarran\flagstone_n.dds".into(), String::new(), r"architecture\mccarran\flagstone_p.dds".into()],
        parallax: flag,
        ..Default::default()
    }
}

const FLAGSTONE_KEY: &str = r"textures\architecture\mccarran\flagstone.dds";

/// #2979: slot 3 is drawn as the game's own height map only with the parallax flag, at the
/// game depth, with the set height maps' method and layers, whatever the Authored materials
/// setting; it is the game's material (no overlay mark), so the legacy look keeps it. Shapes
/// that may draw no height map stay flat and load no file; a bare shape gains tangents for it.
#[test]
fn the_games_own_height_map_is_drawn_on_flagged_shapes() {
    let depth = crate::authored::game_parallax_depth();
    if depth <= 0.0 {
        return; // NVF_GAME_PARALLAX_DEPTH=0 in this test's environment: nothing to draw.
    }
    for on in [true, false] {
        let mut source = SetTextures::with(None);
        source.on = on;
        let (m, _) = shape_material_with(&flagstones(true), &mut triangle(), &mut source, Tangents::Generate);
        let p = source.handle(r"architecture\mccarran\flagstone_p.dds");
        assert_eq!(Parallax::of(&m.base), Parallax::game(p.clone()), "setting on={on}: the slot-3 map at the game depth");
        assert_eq!(m.base.parallax_depth_scale, depth);
        assert!(!crate::authored::height_on(&m), "the game's own map is no overlay");
        assert!(!crate::authored::is_authored(&m));
        assert_eq!(m.extension.legacy.as_ref().map(|l| l.parallax.depth_map.clone()), on.then_some(Some(p)), "the legacy look's baseline holds it");
    }
    // No flag, or an empty slot: flat.
    let mut source = SetTextures::with(None);
    let (m, _) = shape_material_with(&flagstones(false), &mut triangle(), &mut source, Tangents::Generate);
    assert!(m.base.depth_map.is_none(), "slot 3 without the flag is not drawn");
    let mut empty = flagstones(true);
    empty.textures[3].clear();
    let (m, _) = shape_material_with(&empty, &mut triangle(), &mut source, Tangents::Generate);
    assert!(m.base.depth_map.is_none());
    // The shapes a set's height map is refused on refuse the game's too, without loading it.
    for (why, props) in [
        ("alpha blended", Props { alpha: Some((0x10ED, 0)), ..flagstones(true) }),
        ("double-sided", Props { double_sided: true, ..flagstones(true) }),
        ("decal", Props { decal: true, ..flagstones(true) }),
        ("refracting", Props { refraction: Some(0.2), ..flagstones(true) }),
    ] {
        let mut source = SetTextures::with(None);
        let mut mesh = triangle();
        let (m, _) = shape_material_with(&props, &mut mesh, &mut source, Tangents::Generate);
        assert!(m.base.depth_map.is_none(), "{why}: flat");
        assert!(!source.by_path.contains_key(&fnv_vfs::texture_path(r"architecture\mccarran\flagstone_p.dds")), "{why}: the height map is not loaded");
    }
    // A shape without a game normal map gains tangents for its height map.
    let mut bare = flagstones(true);
    bare.textures[1].clear();
    let mut mesh = triangle();
    let (m, _) = shape_material_with(&bare, &mut mesh, &mut SetTextures::with(None), Tangents::Generate);
    assert!(m.base.depth_map.is_some() && mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
}

/// #2979 and #2981: a set's `_p` wins over the game's slot-3 map; a set without `_p` leaves
/// the game's map drawn; taking the set off puts the game's map back.
#[test]
fn an_authored_height_map_wins_over_the_games_own() {
    if crate::authored::game_parallax_depth() <= 0.0 {
        return;
    }
    let mut scratch = Assets::<Image>::default();
    let authored_p = height(&mut scratch, 0.06);
    let mut with_p = set(&mut scratch, false, Scalars::default());
    with_p.height = Some(authored_p.clone());
    for s in [PbrSet::Authored(with_p.clone()), PbrSet::HeightOnly(crate::authored::HeightSet { stem: with_p.stem.clone(), height: authored_p.clone() })] {
        let mut source = SetTextures::with(Some((FLAGSTONE_KEY, s.clone())));
        let (mut m, _) = shape_material_with(&flagstones(true), &mut triangle(), &mut source, Tangents::Generate);
        let game = Parallax::game(source.handle(r"architecture\mccarran\flagstone_p.dds"));
        assert_eq!(Parallax::of(&m.base), authored_p, "{}: the set's _p is drawn", s.stem());
        let legacy = m.extension.legacy.clone().unwrap();
        assert_eq!(legacy.parallax, game, "the game's map is the legacy baseline");
        crate::authored::take_off(&mut m, &legacy);
        assert_eq!(Parallax::of(&m.base), game, "taken off: the game's map is back");
    }
    let no_p = set(&mut scratch, true, Scalars::default());
    let mut source = SetTextures::new(Some((FLAGSTONE_KEY, no_p)));
    let (m, _) = shape_material_with(&flagstones(true), &mut triangle(), &mut source, Tangents::Generate);
    assert!(crate::authored::is_authored(&m));
    assert_eq!(m.base.depth_map, Some(source.handle(r"architecture\mccarran\flagstone_p.dds")), "a set without _p keeps the game's map");
}

/// #2979: copies that stop qualifying (a fade, a cloak) drop the game's own height map as
/// they drop a set's; an opaque copy keeps it.
#[test]
fn copies_that_stop_being_eligible_drop_the_games_own_height_map() {
    if crate::authored::game_parallax_depth() <= 0.0 {
        return;
    }
    for on in [true, false] {
        let mut source = SetTextures::with(None);
        source.on = on;
        let (m, _) = shape_material_with(&flagstones(true), &mut triangle(), &mut source, Tangents::Generate);
        let mut kept = m.clone();
        assert!(!crate::authored::recheck_height(&mut kept), "still opaque");
        assert!(kept.base.depth_map.is_some());
        let mut fading = m.clone();
        fading.base.alpha_mode = AlphaMode::Blend;
        assert!(crate::authored::recheck_height(&mut fading), "setting on={on}");
        assert!(fading.base.depth_map.is_none());
        let mut cloak = m.clone();
        refraction_material(&mut cloak, 0.2, 0.0);
        assert!(cloak.base.depth_map.is_none(), "setting on={on}: a refracting copy");
    }
}

/// #2979 review: an actor's texture-set copy (`Tangents::Keep`, the actor draws the original
/// mesh) whose record gives a flagged shape a slot-3 map: the material takes it only where the
/// drawn mesh has tangents, and no tangents are made on the copy for it; mesh and material
/// are judged together.
#[test]
fn a_copy_of_a_drawn_mesh_takes_the_games_height_map_only_with_its_own_tangents() {
    if crate::authored::game_parallax_depth() <= 0.0 {
        return;
    }
    // No game normal map, so nothing else asks for tangents.
    let mut bare = flagstones(true);
    bare.textures[1].clear();
    let mut source = SetTextures::with(None);
    let mut drawn = triangle();
    let (m, _) = shape_material_with(&bare, &mut drawn, &mut source, Tangents::Keep);
    assert!(drawn.attribute(Mesh::ATTRIBUTE_TANGENT).is_none(), "no tangents made on the copy");
    assert!(m.base.depth_map.is_none(), "the drawn mesh has no tangents: no height map");
    assert!(!source.by_path.contains_key(&fnv_vfs::texture_path(r"architecture\mccarran\flagstone_p.dds")), "and it is not loaded");
    let mut drawn = triangle();
    drawn.generate_tangents().unwrap();
    let (m, _) = shape_material_with(&bare, &mut drawn, &mut source, Tangents::Keep);
    assert!(drawn.attribute(Mesh::ATTRIBUTE_TANGENT).is_some() && m.base.depth_map.is_some(), "with its own tangents the copy takes it");
}

/// #2979 review: an effect shader's holes start at 255 (nothing eaten yet) and fall by
/// animation alone, which asks the height gate no more; a holes copy therefore drops its
/// height map (the game's or a set's) from the start, whatever the alpha-tested switch.
#[test]
fn a_holes_copy_drops_its_height_map_before_the_holes_open() {
    let mut scratch = Assets::<Image>::default();
    let h = height(&mut scratch, 0.05);
    let mut sources: Vec<(SetTextures, Props)> = vec![(SetTextures::with(Some((KEY, PbrSet::HeightOnly(crate::authored::HeightSet { stem: r"textures\pbr\walls\brick".into(), height: h })))), wall())];
    if crate::authored::game_parallax_depth() > 0.0 {
        sources.push((SetTextures::with(None), flagstones(true)));
    }
    for (mut source, props) in sources {
        let (m, _) = shape_material_with(&props, &mut triangle(), &mut source, Tangents::Generate);
        assert!(m.base.depth_map.is_some());
        for (x, texture) in [(255.0, true), (255.0, false), (-1.0, true)] {
            let mut holed = m.clone();
            holed.extension.holes = Vec4::new(x, 4.0, 0.0, 0.0);
            holed.extension.holes_texture = texture.then(Handle::default);
            assert_eq!(crate::authored::height_gate(&holed, true), Err(crate::authored::HeightSkip::Other), "holes {x}, texture {texture}");
            assert!(crate::authored::recheck_height(&mut holed) && holed.base.depth_map.is_none(), "holes {x}, texture {texture}");
        }
    }
}

