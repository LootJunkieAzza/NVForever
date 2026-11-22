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
/// drawn mesh (`Tangents::Keep`) its own when it has them, generated otherwise, as for the
/// game's own normal map.
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
    assert_eq!(m.base.normal_map_texture, s.normal);
    assert_eq!(m.base.metallic_roughness_texture.as_ref(), Some(&s.orm));
    assert!(m.extension.legacy.unwrap().tangents);
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
    assert!(m.base.depth_map.is_some() && copy.attribute(Mesh::ATTRIBUTE_TANGENT).is_some(), "a copy of a drawn mesh without tangents has them generated");
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
