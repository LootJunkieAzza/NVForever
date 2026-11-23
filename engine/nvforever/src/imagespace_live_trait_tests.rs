//! Compiled source writes through the actual grade system; authored records only.
use super::*;
use crate::authored_test_records::{record, sub, tes4, top, z};
use fnv_game::game::RunCtx;
use fnv_script::vm::XVal;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use nvf_play::script::{with_world, ScriptState};
use nvf_play::sim::SimState;
use nvf_play::{PlayData, PlayState, PlayWorld};

const MAIN: u32 = 0x800;
const OTHER: u32 = 0x801;
const ANIMATED: u32 = 0x802;

fn records() -> (WorldData, PlayData, &'static FormStore, &'static RefTable) {
    let imad = |id, name, animated: bool, tint: [f32; 4]| {
        let mut header = vec![0; 244];
        header[0..4].copy_from_slice(&u32::from(animated).to_le_bytes());
        header[4..8].copy_from_slice(&8_f32.to_le_bytes());
        header[176..180].copy_from_slice(&1_u32.to_le_bytes());
        let color: Vec<u8> = [0.0]
            .into_iter()
            .chain(tint)
            .flat_map(f32::to_le_bytes)
            .collect();
        let mut subs = vec![sub(b"EDID", z(name)), sub(b"DNAM", header)];
        if id == MAIN {
            // IMAD DNAM holds one key count per multiply/add track, starting at byte 8.
            for (index, (sig, value)) in [
                (b"\x12IAD", 0.0_f32),
                (b"RIAD", 0.25),
                (b"\x13IAD", 1.5),
                (b"SIAD", 0.125),
            ]
            .into_iter()
            .enumerate()
            {
                let count = 8 + 0x12 * 8 + index * 4;
                subs[1].data[count..count + 4].copy_from_slice(&1_u32.to_le_bytes());
                subs.push(sub(
                    sig,
                    [0.0, value].into_iter().flat_map(f32::to_le_bytes).collect(),
                ));
            }
        }
        subs.push(sub(b"TNAM", color));
        record(b"IMAD", 0, id, &subs)
    };
    let bytes = [
        tes4(),
        top(
            b"IMAD",
            &[
                imad(MAIN, "AuthoredRadiationEffect", false, [0.1, 0.3, 0.8, 0.7]),
                imad(
                    OTHER,
                    "AuthoredNeighborEffect",
                    false,
                    [0.8, 0.2, 0.1, 0.25],
                ),
                imad(
                    ANIMATED,
                    "AuthoredAnimatedEffect",
                    true,
                    [0.2, 0.4, 0.6, 0.5],
                ),
            ]
            .concat(),
        ),
    ]
    .concat();
    let plugins = Box::leak(
        vec![fnv_esm::Plugin::parse(&bytes, "AuthoredLiveEffects.esm".into()).unwrap()]
            .into_boxed_slice(),
    );
    let lo = Box::leak(Box::new(fnv_esm::LoadOrder::new(plugins).unwrap()));
    let schema = Box::leak(Box::new(
        fnv_esm::Schema::load_dir(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv"),
        )
        .unwrap(),
    ));
    let index = Box::leak(Box::default());
    let forms = Box::leak(Box::new(FormStore::build(lo, schema)));
    let refs = Box::leak(Box::new(RefTable::build(lo, schema)));
    let world = WorldData {
        lo,
        schema,
        index,
        world: 0,
        persistent_by_grid: Default::default(),
        imposters: Box::leak(Box::new(fnv_world::imposter::ImposterRegions::build(
            lo, schema,
        ))),
        weather_regions: &[],
        dropped_plugins: Vec::new(),
    };
    let play = PlayData {
        lo,
        schema,
        index,
        world: 0,
        interior: None,
        eye: [0.0; 3],
        heading_deg: 0.0,
        pitch_deg: 0.0,
        auto_walk: 0.0,
        start_at_cell_spot: false,
        packages: false,
    };
    (world, play, forms, refs)
}

fn runtime(
    play: PlayData,
    forms: &'static FormStore,
    refs: &'static RefTable,
) -> (ScriptState, SimState) {
    let mut boot = App::new();
    boot.add_plugins(MinimalPlugins)
        .insert_resource(PlayWorld(play))
        .insert_resource(SimState {
            forms,
            refs,
            world: World::new(forms, refs, SimConfig::default()),
        })
        .insert_resource(nvf_play::Location {
            world: 0,
            interior: Some(0x900),
        })
        .insert_resource(State::new(PlayState::Playing))
        .init_resource::<NextState<PlayState>>()
        .insert_resource(nvf_play::player::Player {
            heading: 0.0,
            pitch: 0.0,
            eye_height: 64.0,
            feet: [0.0; 3],
            body_eye: None,
            body_view: None,
            placed: None,
        })
        .init_resource::<nvf_play::phys::PhysState>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<bevy::input::mouse::AccumulatedMouseScroll>()
        .init_resource::<nvf_play::bindings::Bindings>()
        .init_resource::<nvf_play::pause::PlayClock>()
        .init_resource::<nvf_play::pause::Paused>()
        .init_resource::<nvf_play::pause::Page>()
        .init_resource::<nvf_play::loading::Loading>()
        .init_resource::<nvf_play::tiles::mods::ModsPage>()
        // The fork's hardcore consume animations, which `run_scripts` reads.
        .init_resource::<nvf_play::ConsumeAnims>()
        .init_resource::<nvf_play::tutorial_menu::TutorialView>()
        .init_resource::<nvf_play::control::ControlLocks>()
        .init_resource::<nvf_play::control::LockStats>()
        .init_resource::<nvf_play::menu::ActiveMenu>()
        .init_resource::<nvf_play::wait::WaitMenu>()
        .init_resource::<nvf_play::terminal::TerminalScreen>()
        .init_resource::<nvf_play::lockpick::LockpickScreen>()
        .init_resource::<nvf_play::console::Console>()
        .init_resource::<nvf_play::poker::PokerScreen>()
        .init_resource::<nvf_play::credits::Credits>()
        .init_resource::<Assets<Image>>()
        .add_message::<bevy::input::mouse::MouseWheel>()
        .add_message::<nvf_play::tutorial_menu::TutorialInput>()
        .add_message::<nvf_play::RefChanged>()
        .add_message::<nvf_play::fade::RefFade>()
        .add_message::<nvf_play::RefRepose>()
        .add_message::<nvf_play::loading::TravelRequest>()
        .add_message::<nvf_play::OpenContainer>()
        .add_message::<nvf_play::ItemUsed>()
        .add_message::<nvf_play::furniture::UseFurniture>()
        .add_message::<nvf_play::terminal::OpenTerminal>()
        .add_message::<nvf_play::save::LoadRefused>()
        .add_message::<nvf_play::pause::MainMenuRequest>()
        .configure_sets(Update, nvf_play::PlaySet::Ui.run_if(|| false))
        .configure_sets(Update, nvf_play::PlaySet::Sync.run_if(|| false))
        .add_plugins(nvf_play::script::ScriptPlugin);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        boot.update();
        let script = boot.world().resource::<ScriptState>();
        assert!(
            script.off.is_none(),
            "public script bootstrap refused: {:?}",
            script.off
        );
        if script.rt.is_some() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "public script bootstrap timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    (
        boot.world_mut().remove_resource::<ScriptState>().unwrap(),
        boot.world_mut().remove_resource::<SimState>().unwrap(),
    )
}

fn app() -> (App, Entity, Entity) {
    let (world, play, forms, refs) = records();
    let (script, sim) = runtime(play, forms, refs);
    let mut a = App::new();
    a.insert_resource(Time::<()>::default())
        .insert_resource(world)
        .insert_resource(crate::sky::authored_session_sky())
        .insert_resource(script)
        .insert_resource(sim)
        .init_resource::<ScreenModifiers>()
        .add_systems(Update, update_grade);
    let main = a.world_mut().spawn(ImageSpaceGrade::default()).id();
    let lens = a
        .world_mut()
        .spawn((ImageSpaceGrade::default(), LensGrade))
        .id();
    (a, main, lens)
}

fn source(a: &mut App, body: &str) -> XVal {
    a.world_mut()
        .resource_scope(|world, mut script: Mut<ScriptState>| {
            let mut sim = world.resource_mut::<SimState>();
            with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| {
                rt.game
                    .run_query(
                        &format!("Begin Function {{}}\n{body}\nEnd"),
                        RunCtx::default(),
                    )
                    .unwrap_or_else(|e| panic!("source {body}: {e}"))
            })
        })
}
fn selected(a: &mut App, name: &str) {
    source(a, &format!("IMOD {name}"));
}
fn set(a: &mut App, trait_id: i64, value: f64) {
    source(
        a,
        &format!("SetImageSpaceModTrait AuthoredRadiationEffect {trait_id} {value}"),
    );
    assert_eq!(
        source(
            a,
            &format!("SetFunctionValue (GetImageSpaceModTrait AuthoredRadiationEffect {trait_id})")
        ),
        XVal::Num(value)
    );
}
fn round_trip(a: &mut App, trait_id: i64, expected: f32) {
    let getter = format!("GetImageSpaceModTrait AuthoredRadiationEffect {trait_id}");
    let XVal::Num(value) = source(a, &format!("SetFunctionValue ({getter})")) else {
        panic!("trait {trait_id} did not return a number");
    };
    close(value as f32, expected);
    source(
        a,
        &format!("SetImageSpaceModTrait AuthoredRadiationEffect {trait_id} ({getter})"),
    );
}
fn grade(a: &App, e: Entity) -> ImageSpaceGrade {
    *a.world().get::<ImageSpaceGrade>(e).unwrap()
}
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.0001, "actual grade {a} expected {b}");
}
fn tint(a: Vec4, b: Vec4) {
    assert!(a.abs_diff_eq(b, 0.0001), "actual tint {a:?} expected {b:?}");
}

#[test]
fn compiled_radiation_edits_reach_warm_grade_live_updates_and_real_removal() {
    let (mut a, main, _) = app();
    selected(&mut a, "AuthoredRadiationEffect");
    a.update();
    tint(grade(&a, main).tint, Vec4::new(0.1, 0.3, 0.8, 0.7));
    close(grade(&a, main).contrast, 1.625);
    close(grade(&a, main).contrast_avg_lum, 0.25);
    for (id, value) in [(39, 0.0), (40, 0.25), (41, 1.5), (42, 0.125), (50, 178.5)] {
        round_trip(&mut a, id, value);
    }
    a.update();
    let g = grade(&a, main);
    close(g.contrast, 1.625);
    close(g.contrast_avg_lum, 0.25);
    tint(g.tint, Vec4::new(0.1, 0.3, 0.8, 0.7));
    for (id, value) in [
        (37, 1.4),
        (38, 0.2),
        (39, 1.3),
        (40, 0.1),
        (41, 1.2),
        (42, 0.3),
        (43, 0.8),
        (44, 0.2),
        (45, 2.25),
        (46, 0.04),
        (47, 204.0),
        (48, 51.0),
        (49, 102.0),
        (50, 63.75),
    ] {
        set(&mut a, id, value);
    }
    a.update();
    let g = grade(&a, main);
    tint(g.tint, Vec4::new(0.8, 0.2, 0.4, 0.25));
    close(g.saturation, 1.6);
    close(g.contrast, 1.5);
    close(g.contrast_avg_lum, 0.75);
    close(g.brightness, 1.0);
    close(g.blur, 2.25);
    close(g.radial2.w, 0.04);
    set(&mut a, 47, 51.0);
    set(&mut a, 50, 127.5);
    set(&mut a, 45, 0.75);
    a.update();
    tint(grade(&a, main).tint, Vec4::new(0.2, 0.2, 0.4, 0.5));
    close(grade(&a, main).blur, 0.75);
    source(&mut a, "RIMOD AuthoredRadiationEffect");
    a.update();
    tint(grade(&a, main).tint, ImageSpaceGrade::default().tint);
    close(grade(&a, main).blur, 0.0);
    close(grade(&a, main).saturation, 1.0);
    assert!(a.world().resource::<ScreenModifiers>().shown.is_empty());
    selected(&mut a, "AuthoredRadiationEffect");
    a.update();
    tint(grade(&a, main).tint, Vec4::new(0.2, 0.2, 0.4, 0.5));
}

#[test]
fn lens_uses_live_traits_while_other_and_animated_records_keep_their_own_tracks() {
    let (mut a, main, lens) = app();
    a.world_mut()
        .resource_mut::<ScreenModifiers>()
        .hold_lens(MAIN, true);
    selected(&mut a, "AuthoredNeighborEffect");
    round_trip(&mut a, 50, 178.5);
    a.update();
    tint(grade(&a, lens).tint, Vec4::new(0.1, 0.3, 0.8, 0.7));
    set(&mut a, 47, 153.0);
    set(&mut a, 48, 102.0);
    set(&mut a, 49, 51.0);
    set(&mut a, 50, 127.5);
    a.update();
    tint(grade(&a, lens).tint, Vec4::new(0.6, 0.4, 0.2, 0.5));
    tint(grade(&a, main).tint, Vec4::new(0.8, 0.2, 0.1, 0.25));
    source(
        &mut a,
        "SetImageSpaceModTrait AuthoredAnimatedEffect 50 255",
    );
    a.world_mut()
        .resource_mut::<ScreenModifiers>()
        .hold_lens(MAIN, false);
    a.world_mut()
        .resource_mut::<ScreenModifiers>()
        .hold_lens(ANIMATED, true);
    a.update();
    tint(grade(&a, lens).tint, Vec4::new(0.2, 0.4, 0.6, 0.5));
    a.world_mut().remove_resource::<ScriptState>();
    a.world_mut()
        .resource_mut::<ScreenModifiers>()
        .hold_lens(ANIMATED, false);
    a.world_mut()
        .resource_mut::<ScreenModifiers>()
        .hold_lens(MAIN, true);
    a.update();
    tint(grade(&a, lens).tint, Vec4::new(0.1, 0.3, 0.8, 0.7));
    tint(grade(&a, main).tint, ImageSpaceGrade::default().tint);
}

#[test]
fn checked_restore_does_not_inherit_old_edits_and_source_load_reset_restores_base() {
    let (mut a, main, _) = app();
    set(&mut a, 47, 230.0);
    set(&mut a, 48, 25.0);
    set(&mut a, 49, 76.0);
    set(&mut a, 50, 102.0);
    selected(&mut a, "AuthoredRadiationEffect");
    a.update();
    close(grade(&a, main).tint.w, 0.4);
    a.world_mut()
        .resource_scope(|world, mut script: Mut<ScriptState>| {
            let mut sim = world.resource_mut::<SimState>();
            let (forms, refs) = (sim.forms, sim.refs);
            with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| {
                let saved = rt.game.checked_snapshot().unwrap();
                rt.game = fnv_game::Game::restore(
                    rt.game.data,
                    World::new(forms, refs, SimConfig::default()),
                    fnv_game::GameConfig::default(),
                    &saved,
                )
                .unwrap();
            });
        });
    a.update();
    tint(grade(&a, main).tint, Vec4::new(0.1, 0.3, 0.8, 0.7));
    // The original-style load controller explicitly removes its still-active modifier.
    source(&mut a, "RIMOD AuthoredRadiationEffect");
    a.update();
    tint(grade(&a, main).tint, ImageSpaceGrade::default().tint);
    set(&mut a, 47, 25.5);
    set(&mut a, 48, 204.0);
    set(&mut a, 49, 51.0);
    set(&mut a, 50, 51.0);
    selected(&mut a, "AuthoredRadiationEffect");
    a.update();
    tint(grade(&a, main).tint, Vec4::new(0.1, 0.8, 0.2, 0.2));
}

/// The installed radiation controller's own units: RGB and alpha 0–255, alpha written as
/// `strength * 255` (default strength 0.3). A full-strength effect is a 30 % tint, not an
/// opaque one (the playtest's whole screen turned cyan in irradiated water).
#[test]
fn radiation_controller_alpha_is_0_to_255_like_its_rgb() {
    let (mut a, main, _) = app();
    set(&mut a, 47, 0.0);
    set(&mut a, 48, 255.0);
    set(&mut a, 49, 33.0);
    source(&mut a, "SetImageSpaceModTrait AuthoredRadiationEffect 50 (1 * 0.3 * 255)");
    selected(&mut a, "AuthoredRadiationEffect");
    a.update();
    tint(grade(&a, main).tint, Vec4::new(0.0, 1.0, 33.0 / 255.0, 0.3));
}

#[test]
fn nonanimated_hdr_fade_radial_dof_and_motion_traits_reach_grade_and_restore() {
    use fnv_world::imagespace::{HDR_BRIGHT_SCALE, HDR_EMISSIVE_MULT, HDR_TARGET_LUM};
    let (mut a, main, _) = app();
    selected(&mut a, "AuthoredRadiationEffect");
    a.update();
    for (id, value) in [
        (1, 0.25),
        (2, 0.75),
        // HDR_EMISSIVE_MULT = slot 3 -> traits 9 (mult), 10 (add).
        (9, 2.0),
        (10, 0.5),
        // HDR_TARGET_LUM = slot 4 -> trait 11 (mult).
        (11, 1.5),
        // HDR_BRIGHT_SCALE = slot 6 -> trait 15 (mult).
        (15, 3.0),
        // Fade RGB (0–255) and normalized alpha.
        (51, 255.0),
        (52, 127.5),
        (53, 0.0),
        (54, 0.4),
        // Radial blur tracks.
        (55, 2.5),
        (56, 0.6),
        (57, 0.2),
        (58, 0.4),
        (59, 0.8),
        // Depth of field tracks.
        (60, 0.75),
        (61, 40.0),
        (62, 320.0),
        // Motion blur.
        (63, 0.35),
    ] {
        set(&mut a, id, value);
    }
    a.update();
    let g = grade(&a, main);
    tint(g.fade, Vec4::new(1.0, 0.5, 0.0, 0.4));
    tint(g.radial, Vec4::new(2.5, 0.6, 0.2, 0.4));
    tint(g.radial2, Vec4::new(0.8, 0.25, 0.75, 0.0));
    tint(g.dof, Vec4::new(0.75, 40.0, 320.0, -1.0));
    close(g.motion.x, 0.35);
    let mods = a.world().resource::<ScreenModifiers>();
    close(mods.hdr_ratio(HDR_EMISSIVE_MULT), 2.5);
    close(mods.hdr_ratio(HDR_TARGET_LUM), 1.5);
    close(mods.hdr_ratio(HDR_BRIGHT_SCALE), 3.0);

    source(&mut a, "RIMOD AuthoredRadiationEffect");
    a.update();
    let g = grade(&a, main);
    tint(g.fade, Vec4::ZERO);
    tint(g.radial, Vec4::ZERO);
    tint(g.radial2, Vec4::new(0.0, 0.5, 0.5, 0.0));
    tint(g.dof, Vec4::new(0.0, 0.0, 0.0, -1.0));
    close(g.motion.x, 0.0);
    let mods = a.world().resource::<ScreenModifiers>();
    close(mods.hdr_ratio(HDR_EMISSIVE_MULT), 1.0);
    close(mods.hdr_ratio(HDR_TARGET_LUM), 1.0);
    close(mods.hdr_ratio(HDR_BRIGHT_SCALE), 1.0);
}

#[test]
fn nonfinite_and_f32_overflow_trait_overrides_leave_grade_and_hdr_finite() {
    use fnv_world::imagespace::HDR_EMISSIVE_MULT;
    let (mut a, main, _) = app();
    selected(&mut a, "AuthoredRadiationEffect");
    // Valid sibling edit alongside NaN, +/-inf, and finite f64 that overflows f32.
    set(&mut a, 48, 204.0);
    a.world_mut()
        .resource_scope(|world, mut script: Mut<ScriptState>| {
            let mut sim = world.resource_mut::<SimState>();
            with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| {
                const IMAD_FAMILY: u8 = 4;
                let form = fnv_sim::FormId(MAIN);
                rt.game.traits.values.insert((form, IMAD_FAMILY, 9), 1.0e300);
                rt.game.traits.values.insert((form, IMAD_FAMILY, 37), f64::NEG_INFINITY);
                rt.game.traits.values.insert((form, IMAD_FAMILY, 45), f64::INFINITY);
                rt.game.traits.values.insert((form, IMAD_FAMILY, 47), f64::NAN);
                rt.game.traits.values.insert((form, IMAD_FAMILY, 50), 1.0e300);
            });
        });
    a.update();
    let g = grade(&a, main);
    tint(g.tint, Vec4::new(0.1, 0.8, 0.8, 0.7));
    close(g.saturation, 1.0);
    close(g.blur, 0.0);
    close(a.world().resource::<ScreenModifiers>().hdr_ratio(HDR_EMISSIVE_MULT), 1.0);
}
