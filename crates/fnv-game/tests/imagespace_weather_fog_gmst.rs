//! Live `SetImageSpaceTrait`, `SetCellImageSpace`, `SetWeatherTraitNumeric` fog traits
//! (15..=20), revision/instance-id isolation, and `SetNumericGameSetting` /
//! `SetStringSetting` live GMST readback (docs/spec-notes/world.md §6).

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL_INT: u32 = 0xD00;
const IMGS_A: u32 = 0xD01;
const IMGS_B: u32 = 0xD02;
const WTHR_A: u32 = 0xD03;
const GMST_F: u32 = 0xD04;
const GMST_I: u32 = 0xD05;
const GMST_S: u32 = 0xD06;

fn imgs_dnam(saturation: f32, brightness: f32, tint: [f32; 3], tint_amount: f32) -> Vec<u8> {
    // 152-byte DNAM layout (14 HDR + 1 skin dimmer + 3 bloom + 3 get hit + 4 night eye +
    // 8 cinematic + 5 trailing floats).
    let mut vals = [0.0f32; 38];
    vals[0] = 0.5; // eye_adapt_speed
    vals[3] = 1.0; // emissive_mult
    vals[4] = 1.4; // target_lum
    vals[6] = 1.0; // bright_scale
    vals[11] = 1.1; // sunlight_dimmer
    vals[14] = 1.0; // skin_dimmer
    vals[25] = saturation;
    vals[26] = 0.5; // contrast_avg_lum
    vals[27] = 1.0; // contrast
    vals[28] = brightness;
    vals[29] = tint[0];
    vals[30] = tint[1];
    vals[31] = tint[2];
    vals[32] = tint_amount;
    vals.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn wthr_fnam(fog: [f32; 6]) -> Vec<u8> {
    fog.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn plugin_bytes() -> Vec<u8> {
    let imgs = [
        record(b"IMGS", 0, IMGS_A, &[sub(b"EDID", z("OwnedImgA")), sub(b"DNAM", imgs_dnam(1.0, 1.0, [0.0, 0.0, 0.0], 0.0))]),
        record(b"IMGS", 0, IMGS_B, &[sub(b"EDID", z("OwnedImgB")), sub(b"DNAM", imgs_dnam(0.5, 1.2, [0.5, 0.25, 0.0], 0.2))]),
    ]
    .concat();
    let wthr = record(
        b"WTHR",
        0,
        WTHR_A,
        &[
            sub(b"EDID", z("OwnedWeatherA")),
            sub(b"FNAM", wthr_fnam([1000.0, 100_000.0, 500.0, 50_000.0, 1.0, 1.0])),
            sub(b"DATA", vec![0; 15]),
        ],
    );
    let gmsts = [
        record(b"GMST", 0, GMST_F, &[sub(b"EDID", z("fOwnedCartoFloat")), sub(b"DATA", 12.5f32.to_le_bytes().to_vec())]),
        record(b"GMST", 0, GMST_I, &[sub(b"EDID", z("iOwnedCartoInt")), sub(b"DATA", 7i32.to_le_bytes().to_vec())]),
        record(b"GMST", 0, GMST_S, &[sub(b"EDID", z("sOwnedCartoText")), sub(b"DATA", z("OriginalText"))]),
    ]
    .concat();
    let cell = record(
        b"CELL",
        0,
        CELL_INT,
        &[
            sub(b"EDID", z("OwnedIntCell")),
            sub(b"DATA", vec![1]), // interior
            sub(b"XCIM", u32s(&[IMGS_A])),
        ],
    );
    let player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    [
        tes4(),
        top(b"GMST", &gmsts),
        top(b"NPC_", &player),
        top(b"IMGS", &imgs),
        top(b"WTHR", &wthr),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "CartographerCore.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() })
}

fn q(g: &mut Game<'_>, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default())
        .unwrap_or_else(|e| panic!("{body}: {e}"))
}

#[test]
fn imagespace_and_weather_fog_tokens_track_live_edits_and_isolate_instances() {
    let f = fixture();
    let mut g = game(&f);

    let (id0, img_rev0) = g.image_space_sync_token();
    let (wid0, fog_rev0) = g.weather_fog_sync_token();
    assert_eq!(id0, wid0);
    assert_eq!((img_rev0, fog_rev0), (0, 0));

    // SetImageSpaceTrait across HDR, Bloom, GetHit, NightEye, Cinematic bumps image_space_revision.
    q(
        &mut g,
        "SetImageSpaceTrait OwnedImgA 0 0.85\n\
         SetImageSpaceTrait OwnedImgA 15 0.05\n\
         SetImageSpaceTrait OwnedImgA 18 0.4\n\
         SetImageSpaceTrait OwnedImgA 21 34\n\
         SetImageSpaceTrait OwnedImgA 25 0.7\n\
         SetImageSpaceTrait OwnedImgA 29 250\n\
         SetImageSpaceTrait OwnedImgA 32 0.25",
    );
    let (_, img_rev1) = g.image_space_sync_token();
    assert!(img_rev1 > img_rev0, "image_space_revision must advance on trait edits");
    assert_eq!(g.weather_fog_sync_token().1, fog_rev0, "IMGS edits must not bump weather_fog_revision");

    // Idempotent write of the same value does not churn image_space_revision.
    q(&mut g, "SetImageSpaceTrait OwnedImgA 25 0.7");
    assert_eq!(g.image_space_sync_token().1, img_rev1);
    assert_eq!(q(&mut g, "SetFunctionValue (GetImageSpaceTrait OwnedImgA 29)"), XVal::Num(250.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetImageSpaceTrait OwnedImgA 21)"), XVal::Num(34.0));

    let ovs = g.image_space_trait_overrides(FormId(IMGS_A));
    assert!(ovs.contains(&(0, 0.85)));
    assert!(ovs.contains(&(15, 0.05)));
    assert!(ovs.contains(&(25, 0.7)));
    assert!(ovs.contains(&(29, 250.0)));
    assert!(ovs.contains(&(32, 0.25)));

    // SetCellImageSpace records the assignment for the next cell load (documented GECK wiki
    // rule) and must NOT bump the live SetImageSpaceTrait revision token.
    q(&mut g, "SetCellImageSpace OwnedIntCell OwnedImgB");
    assert_eq!(g.forms.cell_image_spaces.get(&FormId(CELL_INT)), Some(&FormId(IMGS_B)));
    assert_eq!(g.image_space_sync_token().1, img_rev1);

    // Non-fog weather trait (wind speed = 4) must NOT bump weather_fog_revision.
    q(&mut g, "SetWeatherTraitNumeric OwnedWeatherA 4 0.5");
    assert_eq!(g.weather_fog_sync_token().1, fog_rev0);

    // Fog traits 15..=20 bump weather_fog_revision and populate weather_fog_override(s).
    q(
        &mut g,
        "SetWeatherTraitNumeric OwnedWeatherA 15 200\n\
         SetWeatherTraitNumeric OwnedWeatherA 16 12000\n\
         SetWeatherTraitNumeric OwnedWeatherA 17 150\n\
         SetWeatherTraitNumeric OwnedWeatherA 18 6000\n\
         SetWeatherTraitNumeric OwnedWeatherA 19 0.8",
    );
    let fog_rev1 = g.weather_fog_sync_token().1;
    assert!(fog_rev1 > fog_rev0);
    assert_eq!(
        g.weather_fog_override(FormId(WTHR_A)),
        Some([Some(200.0), Some(12_000.0), Some(150.0), Some(6_000.0), Some(0.8), None])
    );
    assert_eq!(g.weather_fog_overrides().get(&WTHR_A), g.weather_fog_override(FormId(WTHR_A)).as_ref());
    // Repeating identical fog value does not churn weather_fog_revision.
    q(&mut g, "SetWeatherTraitNumeric OwnedWeatherA 16 12000");
    assert_eq!(g.weather_fog_sync_token().1, fog_rev1);

    // Finite f64 values outside f32 range (1e300) must not leak f32::INFINITY into overrides.
    q(
        &mut g,
        "let float e10 := 10000000000.0\n\
         let float e50 := e10 * e10 * e10 * e10 * e10\n\
         let float e150 := e50 * e50 * e50\n\
         let float e300 := e150 * e150\n\
         SetWeatherTraitNumeric OwnedWeatherA 20 e300",
    );
    match q(&mut g, "SetFunctionValue (GetWeatherTraitNumeric OwnedWeatherA 20)") {
        XVal::Num(v) => assert!(v.is_finite() && !(v as f32).is_finite() && v >= 1e299, "expected stored 1e300 f64, got {v}"),
        other => panic!("expected numeric weather trait readback, got {other:?}"),
    }
    assert_eq!(
        g.weather_fog_override(FormId(WTHR_A)),
        Some([Some(200.0), Some(12_000.0), Some(150.0), Some(6_000.0), Some(0.8), None])
    );
    assert_eq!(g.weather_fog_overrides().get(&WTHR_A), g.weather_fog_override(FormId(WTHR_A)).as_ref());

    // Restoring into a fresh Game gets a distinct instance_id and clears session-only traits
    // while preserving saved SetCellImageSpace.
    let snap = g.snapshot();
    let world2 = World::new(&f.forms, &f.refs, SimConfig::default());
    let g2 = Game::restore(&f.data, world2, GameConfig { strict: true, ..GameConfig::default() }, &snap).unwrap();
    assert_ne!(g2.image_space_sync_token().0, id0, "new/restored Game must have distinct instance_id");
    assert_eq!(g2.image_space_sync_token().1, 0);
    assert!(g2.image_space_trait_overrides(FormId(IMGS_A)).is_empty());
    assert!(g2.weather_fog_overrides().is_empty());
    assert_eq!(g2.forms.cell_image_spaces.get(&FormId(CELL_INT)), Some(&FormId(IMGS_B)));
}

#[test]
fn game_settings_presets_update_vanilla_gmst_live_and_reject_unknown_safely() {
    let f = fixture();
    let mut g = game(&f);

    assert_eq!(g.world.setting_f32("fOwnedCartoFloat"), Some(12.5));
    assert_eq!(g.world.setting_f32("iOwnedCartoInt"), Some(7.0));
    assert_eq!(
        g.world.setting_value("sOwnedCartoText"),
        Some(&fnv_sim::settings::Setting::Str("OriginalText".into()))
    );

    // Numeric float and integer GMSTs update live in World::setting_f32 and getters.
    assert_eq!(q(&mut g, "SetFunctionValue (SetNumericGameSetting \"fOwnedCartoFloat\" 42.25)"), XVal::Num(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (SetNumericGameSetting \"iOwnedCartoInt\" 19)"), XVal::Num(1.0));
    assert_eq!(g.world.setting_f32("fOwnedCartoFloat"), Some(42.25));
    assert_eq!(g.world.setting_f32("iOwnedCartoInt"), Some(19.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetNumericGameSetting \"fOwnedCartoFloat\")"), XVal::Num(42.25));
    assert_eq!(q(&mut g, "SetFunctionValue (GetNumericGameSetting \"iOwnedCartoInt\")"), XVal::Num(19.0));

    // String GMSTs update live via SetStringSetting.
    q(&mut g, "SetStringSetting \"sOwnedCartoText\" \"UpdatedPreset\"");
    assert_eq!(
        g.world.setting_value("sOwnedCartoText"),
        Some(&fnv_sim::settings::Setting::Str("UpdatedPreset".into()))
    );
    assert_eq!(
        q(&mut g, "SetFunctionValue (GetStringGameSetting \"sOwnedCartoText\")"),
        XVal::Str("UpdatedPreset".into())
    );

    // Non-vanilla shader/injector settings (not authored GMST records) return 0 cleanly
    // without disturbing vanilla GMSTs.
    assert_eq!(q(&mut g, "SetFunctionValue (SetNumericGameSetting \"uSyntheticUnknownSetting\" 7)"), XVal::Num(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (SetNumericGameSetting \"bSyntheticUnknownToggle\" 0)"), XVal::Num(0.0));
    assert_eq!(g.world.setting_f32("fOwnedCartoFloat"), Some(42.25));
}
