//! Actual sky producers -> the production report helper -> numeric/source/CTDA.
//! The comparison baseline extracts the same pre-existing report body neutrally.
use super::*;
use super::session_lighting_tests::{fixtures::{fixture, game, query, restore, Fixture}, sky};
use fnv_game::{cond, functions::Ctx, trace::EventKind, Game};
use fnv_script::vm::{ArgValue, Value, XVal};
use fnv_sim::FormId;

const DESTINATION: u32 = 0x901;
const OTHER: u32 = 0x902;

fn call(g: &mut Game<'_>, opcode: u16, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "authored weather fraction".into(), opcode, on: None,
        self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, args).num()
}

fn fraction(g: &mut Game<'_>) -> f64 { call(g, 0x1094, &[]) }

fn report(g: &mut Game<'_>, sky: &SkyState) {
    crate::report_script_weather(g, sky);
}

fn transition() -> SkyState {
    let mut s = sky();
    s.script_weather(Weather { form_id: DESTINATION, trans_delta: Some(255),
        rainy: true, ..Default::default() }, [None; 6], false, true);
    assert_eq!(s.fade, 0.0);
    assert_eq!(s.prev.as_ref().unwrap().form_id, 0);
    s
}

fn condition(f: &Fixture, g: &Game<'_>, expected: f32) -> cond::Condition {
    use fnv_esm::{decode::{decode_sub, DecodeCtx, SubStatus}, raw::Subrecord};
    let mut bytes = vec![0, 0, 0, 0]; // equality, not a global or OR chain
    bytes.extend(expected.to_le_bytes());
    bytes.extend(148_u16.to_le_bytes());
    bytes.extend([0, 0]);
    bytes.extend(0xDEAD_BEEF_u32.to_le_bytes()); // ignored by this zero-arg function
    bytes.extend(0xCAFE_BABE_u32.to_le_bytes());
    bytes.extend(0_u32.to_le_bytes()); // subject; the getter needs no reference
    bytes.extend(0_u32.to_le_bytes());
    assert_eq!(bytes.len(), 28);
    let mut ctx = DecodeCtx { schema: f.schema, on_formid: None };
    let decoded = decode_sub(b"PERK", &Subrecord { sig: *b"CTDA", data: bytes, extended: false }, &mut ctx);
    assert_eq!(decoded.status, SubStatus::Ok, "authored CTDA setup must decode");
    cond::parse(&decoded.value.unwrap(), &g.data.param_types, &|raw| Some(raw)).unwrap()
}

#[test]
fn real_fade_progress_reaches_numeric_getter_with_unchanged_form_and_rain() {
    let f = fixture(); let mut g = game(&f); let mut s = transition();
    s.advance_fade(0.0625); // 7.5 seconds of the existing thirty-second fade
    assert_eq!(s.fade, 0.25);
    report(&mut g, &s);
    assert_eq!(fraction(&mut g), 0.25, "actual quarter-fade must reach the numeric consumer");
    let shown = (g.weather.shown, g.weather.raining);
    s.advance_fade(0.125);
    assert_eq!(s.fade, 0.75);
    report(&mut g, &s);
    assert_eq!((g.weather.shown, g.weather.raining), shown);
    assert_eq!(fraction(&mut g), 0.75, "unchanged shown/rain must not gate progress reporting");
    s.advance_fade(0.0625);
    report(&mut g, &s);
    assert_eq!(fraction(&mut g), 1.0);
    assert!(s.prev.is_none());
    assert_eq!(call(&mut g, 0x103E, &[]), 1.0);
    assert!(!g.unhandled.contains_key(&0x1094));
}

#[test]
fn actual_clock_progress_drives_both_source_aliases_and_raw_zero_arg_ctda() {
    let f = fixture(); let mut g = game(&f); let mut s = transition();
    s.follow_game_clock(12.0625, 25.0);
    assert_eq!(s.fade, 0.25);
    report(&mut g, &s);
    for name in ["GetCurrentWeatherPercent", "getweatherpct"] {
        assert_eq!(query(&mut g, &format!("SetFunctionValue ({name})")), XVal::Num(0.25));
    }
    let c = condition(&f, &g, 0.25);
    let r = g.eval_conditions(std::slice::from_ref(&c), None, None, "authored quarter-fade");
    assert_eq!(r[0].value, 0.25); assert!(r[0].passed);
    assert_eq!(c.opcode(), 0x1094); assert!(c.params.is_empty());
    s.follow_game_clock(12.1875, 25.0);
    assert_eq!(s.fade, 0.75);
    report(&mut g, &s);
    assert!(!g.conditions_pass(std::slice::from_ref(&c), None, None, "old fraction"));
    let current = condition(&f, &g, 0.75);
    assert!(g.conditions_pass(&[current], None, None, "current fraction"));
}

struct Session(Game<'static>);
#[derive(Resource)]
struct ClockHour(f32);

fn report_system(mut g: NonSendMut<Session>, s: Res<SkyState>) { report(&mut g.0, &s); }
fn advance_clock(mut s: ResMut<SkyState>, hour: Res<ClockHour>) { s.follow_game_clock(hour.0, 25.0); }

#[test]
fn report_before_clock_cadence_exposes_last_bridge_sample_without_a_second_timer() {
    let f = Box::leak(Box::new(fixture()));
    let mut a = App::new();
    a.insert_non_send(Session(game(f))).insert_resource(transition())
        .insert_resource(ClockHour(12.0625))
        // Same report-before-clock order as the registered production chain;
        // only this report seam replaces private ScriptState acquisition here.
        .add_systems(Update, (report_system, advance_clock).chain());
    a.update();
    assert_eq!(a.world().resource::<SkyState>().fade, 0.25);
    assert_eq!(fraction(&mut a.world_mut().non_send_mut::<Session>().0), 0.0);
    a.world_mut().resource_mut::<ClockHour>().0 = 12.1875;
    a.update();
    assert_eq!(fraction(&mut a.world_mut().non_send_mut::<Session>().0), 0.25);
    assert_eq!(a.world().resource::<SkyState>().fade, 0.75);
    a.update();
    assert_eq!(fraction(&mut a.world_mut().non_send_mut::<Session>().0), 0.75);
}

#[test]
fn stable_instant_and_legacy_setter_keep_current_rain_outgoing_and_request_contracts() {
    let f = fixture(); let mut g = game(&f); let mut s = sky();
    report(&mut g, &s);
    assert_eq!(fraction(&mut g), 1.0);
    assert_eq!(g.weather.outgoing, None);
    s.script_weather(Weather { form_id: DESTINATION, rainy: true, ..Default::default() }, [None; 6], true, true);
    report(&mut g, &s);
    assert_eq!(fraction(&mut g), 1.0); assert!(s.prev.is_none());
    assert_eq!(g.weather.outgoing, Some(FormId(0)));
    assert_eq!(call(&mut g, 0x103E, &[]), 1.0);
    // Legacy setter keeps its original outgoing/rain behavior, without pretending
    // that a caller supplying only two arguments has measured a new fraction.
    g.set_shown_weather(FormId(DESTINATION), false);
    assert_eq!(fraction(&mut g), 0.0);
    assert_eq!(g.weather.outgoing, Some(FormId(0)));
    let other = ArgValue::Value(Value::Ref(OTHER));
    call(&mut g, 0x1104, &[other.clone(), ArgValue::Value(Value::Num(1.0))]);
    assert_eq!(call(&mut g, 0x1095, std::slice::from_ref(&other)), 1.0, "pending request retains precedence");
    assert_eq!(g.weather.shown, Some(FormId(DESTINATION)));
    let req = g.take_weather_request().unwrap();
    assert_eq!((req.weather, req.instant, req.hold), (FormId(OTHER), false, true));
    call(&mut g, 0x112D, &[other]);
    assert!(g.take_weather_request().is_some_and(|r| r.instant && !r.hold));
    call(&mut g, 0x1158, &[]);
    assert!(g.take_weather_release()); assert!(!g.take_weather_release());
    report(&mut g, &s);
    assert_eq!(fraction(&mut g), 1.0);
}

#[test]
fn cold_fresh_and_restored_sessions_do_not_inherit_a_prior_sky_measurement() {
    let f = fixture(); let mut g = game(&f);
    assert_eq!(fraction(&mut g), 0.0, "explicit unmeasured engine policy");
    let mut s = transition(); s.advance_fade(0.1875); report(&mut g, &s);
    assert_eq!(fraction(&mut g), 0.75);
    let snapshot = g.snapshot();
    let mut loaded = restore(&f, &snapshot);
    assert_eq!(loaded.weather.shown, g.weather.shown);
    assert_eq!(loaded.weather.raining, g.weather.raining);
    assert_eq!(fraction(&mut loaded), 0.0, "shown/rain save fields are not a fresh measurement");
    let mut fresh = game(&f);
    assert_eq!(fraction(&mut fresh), 0.0);
    assert_eq!(fraction(&mut g), 0.75, "other sessions do not clear this one");
    report(&mut loaded, &s);
    assert_eq!(fraction(&mut loaded), 0.75);
}

#[test]
fn invalid_host_measurements_retire_the_previous_fraction_instead_of_clamping() {
    let f = fixture(); let mut g = game(&f); let mut s = transition();
    for bad in [f32::NAN, f32::INFINITY, -0.25, 1.25] {
        s.fade = 0.25; report(&mut g, &s);
        assert_eq!(fraction(&mut g), 0.25);
        s.fade = bad; report(&mut g, &s);
        assert_eq!(fraction(&mut g), 0.0, "invalid measurement {bad} must not leave stale progress");
    }
}

#[test]
fn a_real_weather_fraction_does_not_suppress_unrelated_unknown_diagnostics() {
    let f = fixture(); let mut g = game(&f); let mut s = transition();
    s.advance_fade(0.0625); report(&mut g, &s);
    assert_eq!(fraction(&mut g), 0.25);
    assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::Unhandled { .. } | EventKind::CallFailed { .. })));
    let before = g.unhandled.len();
    assert_eq!(call(&mut g, 0x7FFE, &[]), 0.0);
    assert_eq!(g.unhandled.len(), before + 1);
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::Unhandled { .. })));
    assert_eq!(fraction(&mut g), 0.25);
}

// Classification batch: these fixtures use authored WTHR DATA through the actual
// importer, then the same SkyState producer/report path as the seven controls above.
fn imported_weather(f: &Fixture, classification: u8) -> Weather {
    use super::session_lighting_tests::lighting_build::{record, sub, tes4, top, z};
    let mut data = vec![0; 15]; // WeatherData: eleven bytes, flags, RGB.
    data[3] = 255; // existing thirty-second transition encoding
    data[11] = classification;
    let bytes = [tes4(), top(b"WTHR", &record(b"WTHR", 0, DESTINATION,
        &[sub(b"EDID", z("OwnedQueryWeather")), sub(b"DATA", data)]))].concat();
    let plugins = [fnv_esm::Plugin::parse(&bytes, "OwnedQueryWeather.esm".into()).unwrap()];
    let lo = fnv_esm::LoadOrder::new(&plugins).unwrap();
    let w = fnv_world::sky::weather(&lo, f.schema, DESTINATION).unwrap();
    assert_eq!(w.classification, classification, "authored WTHR DATA must reach the real importer");
    assert_eq!(w.trans_delta, Some(255));
    assert_eq!(w.rainy, classification & 4 != 0);
    w
}

fn classified_transition(f: &Fixture, classification: u8) -> SkyState {
    let mut s = sky();
    s.script_weather(imported_weather(f, classification), [None; 6], false, true);
    assert_eq!(s.fade, 0.0);
    assert!(s.prev.is_some());
    s
}

fn classified_condition(f: &Fixture, g: &Game<'_>, index: u16, expected: f32) -> cond::Condition {
    use fnv_esm::{decode::{decode_sub, DecodeCtx, SubStatus}, raw::Subrecord};
    let mut bytes = vec![0; 4]; // literal equality
    bytes.extend(expected.to_le_bytes());
    bytes.extend(index.to_le_bytes());
    bytes.extend([0; 2]);
    bytes.extend(0xDEAD_BEEF_u32.to_le_bytes());
    bytes.extend(0xCAFE_BABE_u32.to_le_bytes());
    bytes.extend([0; 8]); // subject, no reference
    assert_eq!(bytes.len(), 28);
    let mut ctx = DecodeCtx { schema: f.schema, on_formid: None };
    let d = decode_sub(b"PERK", &Subrecord { sig: *b"CTDA", data: bytes, extended: false }, &mut ctx);
    assert_eq!(d.status, SubStatus::Ok);
    cond::parse(&d.value.unwrap(), &g.data.param_types, &|id| Some(id)).unwrap()
}

fn assert_classified_routes(f: &Fixture, g: &mut Game<'_>, opcode: u16, index: u16, name: &str, expected: f64) {
    assert_eq!(call(g, opcode, &[]), expected, "numeric {name} must consume the actual reported sky");
    assert_eq!(query(g, &format!("SetFunctionValue ({name})")), XVal::Num(expected));
    let c = classified_condition(f, g, index, expected as f32);
    let result = g.eval_conditions(std::slice::from_ref(&c), None, None, "authored weather class");
    assert_eq!(result[0].value, expected);
    assert!(result[0].passed);
    assert_eq!(c.opcode(), opcode);
    assert!(c.params.is_empty());
    assert!(!g.unhandled.contains_key(&opcode));
}

fn classes(g: &mut Game<'_>) -> [f64; 3] {
    [call(g, 0x104B, &[]), call(g, 0x110A, &[]), call(g, 0x110B, &[])]
}

#[test]
fn snow_query_is_binary_and_waits_for_real_destination_settlement() {
    let f = fixture(); let mut g = game(&f); let mut s = classified_transition(&f, 8);
    s.follow_game_clock(12.25, 25.0); report(&mut g, &s);
    assert_classified_routes(&f, &mut g, 0x104B, 75, "IsSnowing", 1.0);
    assert_eq!(classes(&mut g), [1.0, 0.0, 0.0]);
    // Start a fresh genuine transition after proving the positive consumer.
    let mut s = classified_transition(&f, 8);
    for hour in [12.0625, 12.1875] {
        s.follow_game_clock(hour, 25.0); report(&mut g, &s);
        assert!(s.prev.is_some());
        assert_classified_routes(&f, &mut g, 0x104B, 75, "IsSnowing", 0.0);
        assert_eq!(call(&mut g, 0x103E, &[]), 0.0);
    }
    s.follow_game_clock(12.25, 25.0); report(&mut g, &s);
    assert_classified_routes(&f, &mut g, 0x104B, 75, "IsSnowing", 1.0);
    let mut clear = imported_weather(&f, 1); clear.form_id = OTHER;
    s.script_weather(clear, [None; 6], true, false);
    report(&mut g, &s);
    assert_eq!(call(&mut g, 0x104B, &[]), 0.0);
}

#[test]
fn pleasant_query_reaches_numeric_source_and_ctda_quarter_then_three_quarters() {
    let f = fixture(); let mut g = game(&f); let mut s = classified_transition(&f, 1);
    s.follow_game_clock(12.0625, 25.0); report(&mut g, &s);
    assert_classified_routes(&f, &mut g, 0x110A, 266, "IsPleasant", 0.25);
    let shown = (g.weather.shown, g.weather.raining);
    let old = classified_condition(&f, &g, 266, 0.25);
    s.follow_game_clock(12.1875, 25.0); report(&mut g, &s);
    assert_eq!((g.weather.shown, g.weather.raining), shown);
    assert_classified_routes(&f, &mut g, 0x110A, 266, "IsPleasant", 0.75);
    assert!(!g.conditions_pass(&[old], None, None, "stale pleasant fraction"));
    s.follow_game_clock(12.25, 25.0); report(&mut g, &s);
    assert_classified_routes(&f, &mut g, 0x110A, 266, "IsPleasant", 1.0);
    assert_eq!(classes(&mut g), [0.0, 1.0, 0.0]);
}

#[test]
fn cloudy_query_reaches_numeric_source_and_ctda_quarter_then_three_quarters() {
    let f = fixture(); let mut g = game(&f); let mut s = classified_transition(&f, 2);
    s.follow_game_clock(12.0625, 25.0); report(&mut g, &s);
    assert_classified_routes(&f, &mut g, 0x110B, 267, "IsCloudy", 0.25);
    let shown = (g.weather.shown, g.weather.raining);
    let old = classified_condition(&f, &g, 267, 0.25);
    s.follow_game_clock(12.1875, 25.0); report(&mut g, &s);
    assert_eq!((g.weather.shown, g.weather.raining), shown);
    assert_classified_routes(&f, &mut g, 0x110B, 267, "IsCloudy", 0.75);
    assert!(!g.conditions_pass(&[old], None, None, "stale cloudy fraction"));
    s.follow_game_clock(12.25, 25.0); report(&mut g, &s);
    assert_classified_routes(&f, &mut g, 0x110B, 267, "IsCloudy", 1.0);
    assert_eq!(classes(&mut g), [0.0, 0.0, 1.0]);
}

#[test]
fn classification_importer_and_instant_queries_keep_rain_and_other_bits_separate() {
    let f = fixture(); let mut g = game(&f);
    for (class, expected, rain) in [
        (1, [0.0, 1.0, 0.0], 0.0), (2, [0.0, 0.0, 1.0], 0.0),
        (8, [1.0, 0.0, 0.0], 0.0), (4, [0.0, 0.0, 0.0], 1.0),
        (0, [0.0, 0.0, 0.0], 0.0), (0x80, [0.0, 0.0, 0.0], 0.0),
    ] {
        // Each real producer starts from an unrelated weather. Game retains the
        // same destination ID, so its report cannot use identity as a class gate.
        let mut s = sky();
        s.script_weather(imported_weather(&f, class), [None; 6], true, true);
        report(&mut g, &s);
        assert_eq!(classes(&mut g), expected, "classification {class}");
        assert_eq!(fraction(&mut g), 1.0);
        assert_eq!(call(&mut g, 0x103E, &[]), rain);
    }
}

#[test]
fn outgoing_pleasant_does_not_contribute_to_cloudy_destination_or_pending_request() {
    let f = fixture(); let mut g = game(&f); let mut s = sky();
    s.script_weather(imported_weather(&f, 1), [None; 6], true, true);
    report(&mut g, &s);
    assert_eq!(call(&mut g, 0x110A, &[]), 1.0);
    let mut next = imported_weather(&f, 2); next.form_id = OTHER;
    s.script_weather(next, [None; 6], false, true);
    s.follow_game_clock(12.0625, 25.0); report(&mut g, &s);
    assert_eq!(g.weather.outgoing, Some(FormId(DESTINATION)));
    assert_eq!(classes(&mut g), [0.0, 0.0, 0.25]);
    let other = ArgValue::Value(Value::Ref(DESTINATION));
    call(&mut g, 0x1104, &[other.clone(), ArgValue::Value(Value::Num(1.0))]);
    assert_eq!(call(&mut g, 0x1095, &[other]), 1.0);
    assert_eq!(classes(&mut g), [0.0, 0.0, 0.25], "a pending request is not a measured sky");
    assert!(g.take_weather_request().is_some_and(|r| r.hold && !r.instant));
    call(&mut g, 0x1158, &[]);
    assert!(g.take_weather_release());
    assert_eq!(classes(&mut g), [0.0, 0.0, 0.25]);
}

#[test]
fn classification_measurements_are_session_local_and_legacy_writes_invalidate_them() {
    let f = fixture(); let mut g = game(&f); let mut s = classified_transition(&f, 2);
    assert_eq!(classes(&mut g), [0.0; 3]);
    s.follow_game_clock(12.1875, 25.0); report(&mut g, &s);
    assert_eq!(classes(&mut g), [0.0, 0.0, 0.75]);
    let saved = g.snapshot(); let mut loaded = restore(&f, &saved); let mut fresh = game(&f);
    assert_eq!(classes(&mut loaded), [0.0; 3]);
    assert_eq!(classes(&mut fresh), [0.0; 3]);
    assert_eq!(classes(&mut g), [0.0, 0.0, 0.75]);
    report(&mut loaded, &s);
    assert_eq!(classes(&mut loaded), [0.0, 0.0, 0.75]);
    g.set_shown_weather(FormId(DESTINATION), false);
    assert_eq!(classes(&mut g), [0.0; 3]);
    report(&mut g, &s);
    assert_eq!(classes(&mut g), [0.0, 0.0, 0.75]);
}

#[test]
fn invalid_sky_measurement_retires_classification_and_progress_together() {
    let f = fixture(); let mut g = game(&f); let mut s = classified_transition(&f, 1);
    for invalid in [f32::NAN, f32::INFINITY, -0.25, 1.25] {
        s.fade = 0.25; report(&mut g, &s);
        assert_eq!(classes(&mut g), [0.0, 0.25, 0.0]);
        s.fade = invalid; report(&mut g, &s);
        assert_eq!(classes(&mut g), [0.0; 3]);
        assert_eq!(fraction(&mut g), 0.0);
    }
}

#[test]
fn classification_bridge_uses_the_same_report_before_clock_cadence() {
    let f = Box::leak(Box::new(fixture()));
    let mut a = App::new();
    a.insert_non_send(Session(game(f))).insert_resource(classified_transition(f, 1))
        .insert_resource(ClockHour(12.0625))
        .add_systems(Update, (report_system, advance_clock).chain());
    a.update();
    assert_eq!(a.world().resource::<SkyState>().fade, 0.25);
    assert_eq!(classes(&mut a.world_mut().non_send_mut::<Session>().0), [0.0; 3]);
    a.world_mut().resource_mut::<ClockHour>().0 = 12.1875;
    a.update();
    assert_eq!(classes(&mut a.world_mut().non_send_mut::<Session>().0), [0.0, 0.25, 0.0]);
    assert_eq!(a.world().resource::<SkyState>().fade, 0.75);
    a.update();
    assert_eq!(classes(&mut a.world_mut().non_send_mut::<Session>().0), [0.0, 0.75, 0.0]);
}
