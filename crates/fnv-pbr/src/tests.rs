//! Path mapping, classification, the index and scalar files (fork design §9 "Path mapping
//! and index", as the format was settled for #2981). Every name is invented for the tests.

use super::*;

const P: &str = "textures\\pbr\\";

fn p(rest: &str) -> String {
    format!("{P}{rest}")
}

#[test]
fn stem_drops_the_extension_whatever_it_is() {
    assert_eq!(stem_for(r"textures\walls\brick07.dds").as_deref(), Some(r"textures\pbr\walls\brick07"));
    assert_eq!(stem_for(r"textures\walls\brick07.tga").as_deref(), Some(r"textures\pbr\walls\brick07"));
    // Only the last extension goes; a dot inside the name stays.
    assert_eq!(stem_for(r"textures\walls\brick.v2.dds").as_deref(), Some(r"textures\pbr\walls\brick.v2"));
    // No extension at all: the whole name is the stem.
    assert_eq!(stem_for(r"textures\walls\brick07").as_deref(), Some(r"textures\pbr\walls\brick07"));
    // Nothing left of the name.
    assert_eq!(stem_for(r"textures\walls\.dds"), None);
    assert_eq!(stem_for(r"textures\walls\"), None);
    assert_eq!(stem_for(""), None);
}

#[test]
fn stem_takes_every_path_form_the_engine_takes() {
    let want = Some(r"textures\pbr\props\crate_a".to_string());
    for raw in [
        r"textures\props\crate_a.dds",
        r"Textures\Props\Crate_A.DDS",
        "textures/props/crate_a.dds",
        r"textures\\props\crate_a.dds",
        r"\textures\props\crate_a.dds",
        r"data\textures\props\crate_a.dds",
        r"C:\Games\Somewhere\Data\Textures\props\crate_a.dds",
        // As a NIF may name it, without the `textures` folder.
        r"props\crate_a.dds",
    ] {
        assert_eq!(stem_for(raw), want, "{raw}");
    }
}

#[test]
fn console_paths_are_kept_as_given() {
    // The `\xbox\` -> `\pc\` rewrite is the caller's (resolved_path, only when the PC
    // diffuse exists); the stem follows the key it is handed.
    assert_eq!(stem_for(r"textures\signs\xbox\board01.dds").as_deref(), Some(r"textures\pbr\signs\xbox\board01"));
    assert_eq!(stem_for(r"textures\signs\pc\board01.dds").as_deref(), Some(r"textures\pbr\signs\pc\board01"));
    let index = PbrIndex::build([p(r"signs\pc\board01_orm.dds")]);
    assert_eq!(index.lookup(r"textures\signs\pc\board01.dds").map(|f| f.stem), Some(p(r"signs\pc\board01")));
    // An unresolved console key (its PC diffuse absent) is not redirected by a PC set.
    assert_eq!(index.lookup(r"textures\signs\xbox\board01.dds"), None);
}

#[test]
fn member_paths_and_the_diffuse_stem() {
    let stem = stem_for(r"textures\walls\brick07.dds").unwrap();
    assert_eq!(member_path(&stem, Member::Orm), p(r"walls\brick07_orm.dds"));
    assert_eq!(member_path(&stem, Member::Normal), p(r"walls\brick07_n.dds"));
    assert_eq!(member_path(&stem, Member::Height), p(r"walls\brick07_p.dds"));
    assert_eq!(member_path(&stem, Member::Scalars), p(r"walls\brick07.toml"));
    assert_eq!(member_path(&stem, Member::Albedo), p(r"walls\brick07_albedo.dds"));
    assert_eq!(diffuse_stem(&stem).as_deref(), Some(r"textures\walls\brick07"));
    assert_eq!(diffuse_stem(r"textures\walls\brick07"), None);
    // Every member classifies back to its stem.
    for m in Member::ALL {
        assert_eq!(classify(&member_path(&stem, m)), Some((stem.clone(), m)));
    }
}

#[test]
fn classification_peels_exactly_one_suffix() {
    let c = |rest: &str| classify(&p(rest));
    assert_eq!(c("foo_orm.dds"), Some((p("foo"), Member::Orm)));
    assert_eq!(c("foo_n.dds"), Some((p("foo"), Member::Normal)));
    assert_eq!(c("foo_p.dds"), Some((p("foo"), Member::Height)));
    assert_eq!(c("foo.toml"), Some((p("foo"), Member::Scalars)));
    assert_eq!(c("foo_albedo.dds"), Some((p("foo"), Member::Albedo)));
    // Never twice: these belong to a diffuse whose own name ends `_n`.
    assert_eq!(c("foo_n_orm.dds"), Some((p("foo_n"), Member::Orm)));
    assert_eq!(c("foo_n_n.dds"), Some((p("foo_n"), Member::Normal)));
    assert_eq!(c("foo_orm_p.dds"), Some((p("foo_orm"), Member::Height)));
    assert_eq!(c("foo_p.toml"), Some((p("foo_p"), Member::Scalars)));
    // Case and slashes of the path do not matter.
    assert_eq!(classify("Textures/PBR/Walls/Foo_ORM.DDS"), Some((p(r"walls\foo"), Member::Orm)));
    // Unmatched: a bare name (no plain-name albedo), other extensions, empty names, other
    // folders.
    for rest in ["foo.dds", "foo.tga", "foo_orm.png", "foo_n.tga", "_orm.dds", "_n.dds", r"walls\_p.dds", ".toml", "foo_orm.dds.bak", "foo_s.dds"] {
        assert_eq!(c(rest), None, "{rest}");
    }
    assert_eq!(classify(r"textures\walls\foo_orm.dds"), None);
    assert_eq!(classify(r"textures\pbrx\foo_orm.dds"), None);
}

#[test]
fn folders_whose_names_end_in_a_suffix_are_left_alone() {
    // Only the final path component is peeled; a folder named like a member is a folder.
    for (diffuse, stem) in [
        (r"textures\walls_n\brick.dds", r"textures\pbr\walls_n\brick"),
        (r"textures\walls_orm.dds\brick.dds", r"textures\pbr\walls_orm.dds\brick"),
        (r"textures\a.toml\b_p\brick_n.tga", r"textures\pbr\a.toml\b_p\brick_n"),
    ] {
        assert_eq!(stem_for(diffuse).as_deref(), Some(stem), "{diffuse}");
        for m in Member::ALL {
            assert_eq!(classify(&member_path(stem, m)), Some((stem.to_string(), m)), "{stem} {m:?}");
        }
    }
    let index = PbrIndex::build([p(r"walls_n\brick_orm.dds")]);
    assert_eq!(index.lookup(r"textures\walls_n\brick.dds").map(|f| f.stem), Some(p(r"walls_n\brick")));
    // A file directly under such a folder with no suffix of its own is unmatched.
    assert_eq!(classify(&p(r"walls_n\brick.dds")), None);
    assert_eq!(classify(&p(r"walls_orm.dds\")), None);
}

#[test]
fn index_states_and_counts() {
    let index = PbrIndex::build([
        // Full set.
        p(r"walls\a_orm.dds"),
        p(r"walls\a_n.dds"),
        p(r"walls\a_p.dds"),
        p(r"walls\a.toml"),
        // ORM only: authored.
        p(r"walls\b_orm.dds"),
        // Height only: valid.
        p(r"walls\c_p.dds"),
        // These alone activate nothing.
        p(r"walls\d_n.dds"),
        p(r"walls\e.toml"),
        p(r"walls\f_albedo.dds"),
        // Unmatched.
        p(r"walls\g.dds"),
        p(r"readme.txt"),
        // Outside the folder: not counted at all.
        r"textures\walls\a.dds".to_string(),
        r"meshes\walls\a.nif".to_string(),
    ]);
    let c = index.counts();
    assert_eq!(
        c,
        Counts { files: 11, sets: 6, authored: 2, height_only: 1, incomplete: 3, orm: 2, normal: 2, height: 2, scalars: 2, albedo: 1, unmatched: 2 }
    );
    assert_eq!(c.active(), 3);
    assert_eq!(index.unmatched(), [p("readme.txt"), p(r"walls\g.dds")].as_slice());
    let state = |stem: &str| index.get(&p(stem)).map(|f| f.state());
    assert_eq!(state(r"walls\a"), Some(SetState::Authored));
    assert_eq!(state(r"walls\b"), Some(SetState::Authored));
    assert_eq!(state(r"walls\c"), Some(SetState::HeightOnly));
    assert_eq!(state(r"walls\d"), Some(SetState::Incomplete));
    assert_eq!(state(r"walls\f"), Some(SetState::Incomplete));
    assert_eq!(state(r"walls\g"), None);
    let a = index.get(&p(r"walls\a")).unwrap();
    assert_eq!(a.members().collect::<Vec<_>>(), [Member::Orm, Member::Normal, Member::Height, Member::Scalars]);
    assert!(!index.is_empty());
    assert_eq!(
        index.summary(),
        "authored materials: 3 sets under textures\\pbr\\ (2 with _orm, 1 height only; files: 2 _orm, 2 _n, 2 _p, 2 .toml, 1 _albedo reserved), 3 incomplete sets and 2 unmatched files ignored"
    );
}

#[test]
fn index_normalises_and_counts_a_path_once() {
    let index = PbrIndex::build(["Textures/PBR/Walls/A_ORM.dds", r"textures\pbr\walls\a_orm.dds", r"textures\\pbr\walls\a_n.dds"]);
    assert_eq!(index.counts().files, 2);
    assert_eq!(index.counts().orm, 1);
    assert_eq!(index.sets().map(|(s, f)| (s.to_string(), f.state())).collect::<Vec<_>>(), [(p(r"walls\a"), SetState::Authored)]);
    assert!(PbrIndex::build(Vec::<String>::new()).is_empty());
    assert!(PbrIndex::default().lookup(r"textures\walls\a.dds").is_none());
}

#[test]
fn lookup_finds_active_sets_only() {
    let index = PbrIndex::build([p(r"walls\a_orm.dds"), p(r"walls\a_n.dds"), p(r"walls\c_p.dds"), p(r"walls\d_n.dds"), p(r"walls\d.toml"), p(r"walls\f_albedo.dds")]);
    let a = index.lookup(r"Textures\Walls\A.dds").unwrap();
    assert_eq!(a.stem, p(r"walls\a"));
    assert_eq!(a.state(), SetState::Authored);
    assert!(!a.first_person_fallback);
    assert_eq!(a.path(Member::Orm), Some(p(r"walls\a_orm.dds")));
    assert_eq!(a.path(Member::Normal), Some(p(r"walls\a_n.dds")));
    assert_eq!(a.path(Member::Height), None);
    // A `.tga` diffuse shares the extension-less stem.
    assert_eq!(index.lookup(r"textures\walls\a.tga").map(|f| f.stem), Some(p(r"walls\a")));
    assert_eq!(index.lookup(r"textures\walls\c.dds").map(|f| f.state()), Some(SetState::HeightOnly));
    // `_n`, `.toml` or the reserved `_albedo` alone: nothing.
    assert_eq!(index.lookup(r"textures\walls\d.dds"), None);
    assert_eq!(index.lookup(r"textures\walls\f.dds"), None);
    assert_eq!(index.lookup(r"textures\walls\none.dds"), None);
}

#[test]
fn first_person_falls_back_to_the_plain_set() {
    let index = PbrIndex::build([p(r"weapons\gun\blaster01_orm.dds"), p(r"weapons\gun\1stpersonrifle02_orm.dds"), p(r"weapons\gun\rifle02_orm.dds"), p(r"weapons\gun\1stpersonpistol03_n.dds"), p(r"weapons\gun\pistol03_p.dds")]);
    // No twin set: the plain one.
    let f = index.lookup(r"textures\weapons\gun\1stPersonBlaster01.dds").unwrap();
    assert_eq!((f.stem.as_str(), f.first_person_fallback), (p(r"weapons\gun\blaster01").as_str(), true));
    // The twin has its own set: that one.
    let f = index.lookup(r"textures\weapons\gun\1stpersonrifle02.dds").unwrap();
    assert_eq!((f.stem.as_str(), f.first_person_fallback), (p(r"weapons\gun\1stpersonrifle02").as_str(), false));
    // The twin's set is incomplete: the plain set is used.
    let f = index.lookup(r"textures\weapons\gun\1stpersonpistol03.dds").unwrap();
    assert_eq!((f.stem.as_str(), f.state(), f.first_person_fallback), (p(r"weapons\gun\pistol03").as_str(), SetState::HeightOnly, true));
    // Neither: nothing. A plain diffuse never takes a twin's set.
    assert_eq!(index.lookup(r"textures\weapons\gun\1stpersonother.dds"), None);
    assert_eq!(index.lookup(r"textures\weapons\gun\1stperson.dds"), None);
    let twin_only = PbrIndex::build([p(r"weapons\gun\1stpersonsmg04_orm.dds")]);
    assert_eq!(twin_only.lookup(r"textures\weapons\gun\smg04.dds"), None);
}

#[test]
fn scalars_valid() {
    let f = parse_scalars(b"parallax_depth = 0.05\nroughness_scale = 0.8\nreflectance = 0\n");
    assert_eq!(f.scalars, Scalars { parallax_depth: Some(0.05), roughness_scale: Some(0.8), reflectance: Some(0.0), metal_gain: None });
    assert!(f.warnings.is_empty(), "{:?}", f.warnings);
    // An empty file, and one with a byte order mark and a comment.
    let empty = parse_scalars(b"");
    assert_eq!((empty.scalars, empty.warnings.len()), (Scalars::default(), 0));
    assert_eq!(empty.scalars.roughness_scale_or_default(), DEFAULT_ROUGHNESS_SCALE);
    assert_eq!(empty.scalars.reflectance_or_default(), DEFAULT_REFLECTANCE);
    assert_eq!(empty.scalars.parallax_depth_or_default(), DEFAULT_PARALLAX_DEPTH);
    assert_eq!(f.scalars.parallax_depth_or_default(), 0.05);
    assert!(PARALLAX_DEPTH_RANGE.contains(&DEFAULT_PARALLAX_DEPTH));
    let bom = parse_scalars(b"\xEF\xBB\xBF# metal sheet\nreflectance = 0.25\n");
    assert_eq!((bom.scalars.reflectance, bom.warnings.len()), (Some(0.25), 0));
}

#[test]
fn scalars_unknown_keys_and_wrong_types_warn() {
    let f = parse_scalars(b"normal_strength = 2.0\nReflectance = 0.3\nroughness_scale = \"rough\"\nreflectance = 0.4\n[extra]\nx = 1\n");
    assert_eq!(f.scalars, Scalars { parallax_depth: None, roughness_scale: None, reflectance: Some(0.4), metal_gain: None });
    assert_eq!(
        f.warnings,
        [
            "unknown key `Reflectance`: ignored",
            "unknown key `extra`: ignored",
            "unknown key `normal_strength`: ignored",
            "`roughness_scale` is a string, not a number: ignored",
        ]
    );
    let nan = parse_scalars(b"parallax_depth = nan\nreflectance = inf\n");
    assert_eq!(nan.scalars, Scalars::default());
    assert_eq!(nan.warnings.len(), 2);
}

#[test]
fn scalars_out_of_range_are_clamped() {
    let f = parse_scalars(b"parallax_depth = 1.5\nroughness_scale = -1\nreflectance = 1.2\n");
    assert_eq!(f.scalars, Scalars { parallax_depth: Some(0.08), roughness_scale: Some(0.0), reflectance: Some(1.0), metal_gain: None });
    assert_eq!(
        f.warnings,
        ["`parallax_depth` = 1.5 is outside 0 to 0.08: clamped to 0.08", "`reflectance` = 1.2 is outside 0 to 1: clamped to 1", "`roughness_scale` = -1 is outside 0 to 2: clamped to 0",]
    );
    // The ends of a range are inside it, 0.08 too (judged in f32, the type it is kept in).
    let ends = parse_scalars(b"parallax_depth = 0.08\nroughness_scale = 2\nreflectance = 1.0\n");
    assert!(ends.warnings.is_empty(), "{:?}", ends.warnings);
    assert_eq!(ends.scalars.parallax_depth, Some(0.08));
    // The old upper end (0.25) and the depths between are clamped now.
    for text in ["0.25", "0.1"] {
        let f = parse_scalars(format!("parallax_depth = {text}\n").as_bytes());
        assert_eq!(f.scalars.parallax_depth, Some(0.08), "{text}");
        assert_eq!(f.warnings, [format!("`parallax_depth` = {text} is outside 0 to 0.08: clamped to 0.08")], "{text}");
    }
    assert_eq!(*PARALLAX_DEPTH_RANGE.end(), 0.08);
}

#[test]
fn scalars_not_toml_are_ignored() {
    for bytes in [&b"reflectance: 0.5"[..], b"reflectance = = 1", b"{\"reflectance\": 0.5}"] {
        let f = parse_scalars(bytes);
        assert_eq!(f.scalars, Scalars::default());
        assert_eq!(f.warnings.len(), 1);
        assert!(f.warnings[0].starts_with("not TOML ("), "{:?}", f.warnings);
        assert!(f.warnings[0].ends_with("): ignored"), "{:?}", f.warnings);
        assert!(f.unreadable);
    }
    // Read files are not unreadable, warnings or not.
    assert!(!parse_scalars(b"normal_strength = 2.0\n").unreadable);
    assert!(!parse_scalars(b"").unreadable);
    let f = parse_scalars(b"reflectance = 0.5\xff\n");
    assert!(f.unreadable);
    assert_eq!((f.scalars, f.warnings), (Scalars::default(), vec!["not UTF-8 text: ignored".to_string()]));
}

#[test]
fn scalars_metal_gain_reads_clamps_and_defaults() {
    let f = parse_scalars(b"metal_gain = 6\n");
    assert_eq!((f.scalars.metal_gain, f.warnings.len()), (Some(6.0), 0));
    // Set in the file: the file's; unset: 12.
    assert_eq!(f.scalars.metal_gain_or_default(), 6.0);
    assert_eq!(Scalars::default().metal_gain_or_default(), DEFAULT_METAL_GAIN);
    assert_eq!(DEFAULT_METAL_GAIN, 12.0);
    assert!(METAL_GAIN_RANGE.contains(&DEFAULT_METAL_GAIN));
    // Out of range: clamped with a warning; the ends are inside.
    let low = parse_scalars(b"metal_gain = 0.5\n");
    assert_eq!(low.scalars.metal_gain, Some(1.0));
    assert_eq!(low.warnings, ["`metal_gain` = 0.5 is outside 1 to 40: clamped to 1"]);
    let high = parse_scalars(b"metal_gain = 100\n");
    assert_eq!(high.scalars.metal_gain, Some(40.0));
    assert_eq!(high.warnings, ["`metal_gain` = 100 is outside 1 to 40: clamped to 40"]);
    let ends = parse_scalars(b"metal_gain = 1\n");
    assert!(ends.warnings.is_empty() && ends.scalars.metal_gain == Some(1.0));
    assert!(parse_scalars(b"metal_gain = 40.0\n").warnings.is_empty());
    // Not a number: ignored with a warning, so the default applies.
    let word = parse_scalars(b"metal_gain = \"steel\"\n");
    assert_eq!(word.scalars.metal_gain, None);
    assert_eq!(word.warnings, ["`metal_gain` is a string, not a number: ignored"]);
}
