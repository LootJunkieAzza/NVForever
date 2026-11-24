//! Script-requested player animation layers. Separate from cinematic IDLE slots:
//! an animated wrist display must leave walking and mouse input with the player.

use bevy::prelude::*;
use fnv_actor::idle::meshes_path;
use fnv_actor::idle_play::IdleKeys;
use fnv_game::nvse_anim::ScriptSequence;
use fnv_game::nvse_knvse::SequenceInfo;
use fnv_sim::FormId;
use nvf_assets::actor::{ActorCtx, ActorParts};
use nvf_assets::anim_blend::AdditivePose;
use nvf_assets::anim_play::Clip;
use nvf_assets::model::ModelCtx;
use nvf_play::script::ScriptState;
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct Sample {
    pub request: ScriptSequence,
    pub clip: Arc<Clip>,
    pub time: f32,
    pub weight: f32,
    pub fade_in: f32,
    pub additive: Option<AdditivePose>,
}

#[derive(Resource, Default)]
pub(crate) struct PlayerSequences {
    pub first: Vec<Sample>,
    pub third: Vec<Sample>,
    /// The device-menu frame, also when the virtual world clock is stopped.
    pub first_menu_dt: f32,
}

/// Consume actual admitted menu time once. Physics can keep preparing a pose
/// while the script runtime is held, with its last frame configuration retained.
fn prepare_frame(game: &fnv_game::Game<'_>, samples: &mut PlayerSequences, clock: &mut Option<f64>) {
    samples.first.clear();
    samples.third.clear();
    samples.first_menu_dt = 0.0;
    let now = game.first_person_menu_animation_time();
    let elapsed = clock.replace(now).map_or(0.0, |before| (now - before).max(0.0));
    if !game.loading && game.in_menu() && game.menu_open(fnv_game::game::menus::PIPBOY) && elapsed.is_finite() {
        samples.first_menu_dt = elapsed as f32;
    }
}

fn additive_ref_path(ref_pose: &str) -> String {
    meshes_path(ref_pose)
}

pub(crate) fn prepare(
    mut script: Option<ResMut<ScriptState>>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    mut samples: ResMut<PlayerSequences>,
    mut told: Local<HashSet<u32>>,
    mut menu_clock: Local<Option<f64>>,
) {
    samples.first.clear();
    samples.third.clear();
    samples.first_menu_dt = 0.0;
    let Some(script) = script.as_deref_mut() else { *menu_clock = None; return; };
    let Some(runtime) = script.rt.as_mut() else { *menu_clock = None; return; };
    let g = &mut runtime.game;
    prepare_frame(g, &mut samples, &mut menu_clock);
    let requests: Vec<_> = g.anim_paths.sequences.iter()
        .filter(|((actor, _, _), _)| *actor == FormId::PLAYER_REF)
        .map(|(_, request)| request.clone()).collect();
    let active: HashSet<_> = requests.iter().map(|r|r.serial).collect();
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    for request in requests {
        let path = meshes_path(&request.path);
        if !ctx.clip_ready(&path) { continue; }
        let Some(clip) = ctx.clip(&path) else {
            if told.insert(request.serial) { warn!("script sequence: {:?} could not load; not rendered", request.path); }
            g.anim_paths.sequences.remove(&(FormId::PLAYER_REF, request.first, request.path.replace('/', "\\").to_ascii_lowercase()));
            if let Some(paths) = g.anim_paths.played.get_mut(&FormId::PLAYER_REF) {
                paths.retain(|p| !p.eq_ignore_ascii_case(&request.path));
            }
            continue;
        };
        g.set_sequence_info(&request.path, SequenceInfo {
            name:clip.name.clone(), accum_root:clip.accum_root.clone().unwrap_or_default(),
            frequency:f64::from(clip.frequency), begin:f64::from(clip.start), end:f64::from(clip.stop),
            cycle:clip.cycle_type, text_keys:clip.text_keys.iter().map(|(t,s)| (f64::from(*t),s.clone())).collect(),
        });
        let clock = g.script_sequence_time_for(FormId::PLAYER_REF, &request.path, request.first);
        let weight = f64::from(request.weight);
        let phase = (clock * f64::from(clip.frequency)) as f32;
        if !phase.is_finite() || !weight.is_finite() { continue; }
        let additive = match g.sequence_additive(FormId::PLAYER_REF, request.first, &request.path) {
            Some((ref_pose, ref_time)) if !ref_pose.is_empty() => {
                let ref_path = additive_ref_path(ref_pose);
                if !ctx.clip_ready(&ref_path) {
                    continue;
                }
                Some(AdditivePose { reference: ctx.clip(&ref_path), ref_time })
            }
            Some((_, ref_time)) => Some(AdditivePose { reference: None, ref_time }),
            None => None,
        };
        let keys = IdleKeys::read(clip.start, clip.stop, &clip.text_keys);
        let sample = Sample { time:phase,
            weight:weight.clamp(0.0,1.0) as f32, fade_in:request.ease_in.unwrap_or(keys.blend_in),
            additive, request, clip };
        if told.insert(sample.request.serial) {
            info!("script sequence: player {} {:?}, priority {}, weight {}, {} tracks", if sample.request.first {"first"} else {"third"}, sample.request.path, sample.request.priority, sample.weight, sample.clip.tracks.len());
        }
        if sample.request.first { samples.first.push(sample); } else { samples.third.push(sample); }
    }
    samples.first.sort_by_key(|s|s.request.serial);
    samples.third.sort_by_key(|s|s.request.serial);
    told.retain(|serial|active.contains(serial));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource)]
    struct ClockGame(fnv_game::Game<'static>);

    fn consume_frame(game: Res<ClockGame>, mut samples: ResMut<PlayerSequences>, mut clock: Local<Option<f64>>) {
        // The same admission consumer used by prepare; decoded asset loading and
        // ScriptState/Runtime construction are deliberately outside this seam.
        prepare_frame(&game.0, &mut samples, &mut clock);
    }

    #[test]
    fn prepare_frame_uses_admitted_menu_time_once_when_runtime_ticks_are_skipped() {
        let forms: &'static fnv_sim::FormStore = Box::leak(Box::new(crate::outfits::tests::actor_forms(vec![])));
        let refs: &'static fnv_sim::RefTable = Box::leak(Box::default());
        let mut game = crate::outfits::tests::game(fnv_sim::World::new(forms, refs, Default::default()));
        game.cfg.frame_seconds = 0.125;
        game.menu = Some(fnv_game::game::menus::INVENTORY);
        let mut app = App::new();
        app.insert_resource(ClockGame(game)).init_resource::<PlayerSequences>().add_systems(Update, consume_frame);
        app.update(); // Establish the runtime's clock origin.
        assert_eq!(app.world().resource::<PlayerSequences>().first_menu_dt, 0.0);
        app.world_mut().resource_mut::<ClockGame>().0.tick();
        app.update();
        assert_eq!(app.world().resource::<PlayerSequences>().first_menu_dt, 0.125);
        let frame = app.world().resource::<ClockGame>().0.frame;
        // A retained Pip-Boy context and positive cfg are not a new script tick.
        // This covers the actual producer/prepare-frame boundary without claiming
        // that ordinary Escape input can enter pause over an active engine menu.
        for _ in 0..3 {
            app.update();
            assert_eq!(app.world().resource::<ClockGame>().0.frame, frame);
            assert_eq!(app.world().resource::<ClockGame>().0.cfg.frame_seconds, 0.125);
            assert_eq!(app.world().resource::<PlayerSequences>().first_menu_dt, 0.0, "retained configuration must not animate a held pose");
        }
        app.world_mut().resource_mut::<ClockGame>().0.page_tick(fnv_game::game::menus::START);
        app.update();
        assert_eq!(app.world().resource::<PlayerSequences>().first_menu_dt, 0.0, "a Mods page frame does not admit device animation time");
        app.world_mut().resource_mut::<ClockGame>().0.tick();
        app.update();
        assert_eq!(app.world().resource::<PlayerSequences>().first_menu_dt, 0.125, "resuming admits one current frame");
        {
            let mut game = app.world_mut().resource_mut::<ClockGame>();
            game.0.loading = true;
            game.0.tick();
        }
        app.update();
        assert_eq!(app.world().resource::<PlayerSequences>().first_menu_dt, 0.0, "loading keeps the device clock held");
        // A replacement runtime starts a fresh cumulative clock. The renderer's
        // old cursor must not turn the reversal into a frame or replay past time.
        for _ in 0..2 {
            let mut replacement = crate::outfits::tests::game(fnv_sim::World::new(forms, refs, Default::default()));
            replacement.cfg.frame_seconds = 0.125;
            replacement.menu = Some(fnv_game::game::menus::INVENTORY);
            app.world_mut().resource_mut::<ClockGame>().0 = replacement;
            app.update();
            assert_eq!(app.world().resource::<PlayerSequences>().first_menu_dt, 0.0, "a reset/reversed clock establishes a fresh origin");
            app.world_mut().resource_mut::<ClockGame>().0.tick();
            app.update();
            assert_eq!(app.world().resource::<PlayerSequences>().first_menu_dt, 0.125, "the replacement runtime resumes with only its admitted frame");
            app.update();
            assert_eq!(app.world().resource::<PlayerSequences>().first_menu_dt, 0.0, "a current frame is never replayed");
        }
    }

    #[test]
    fn additive_ref_path_normalizes_forward_slashes_without_double_meshes_prefix() {
        assert_eq!(additive_ref_path("meshes/foo/bar.kf"), "meshes\\foo\\bar.kf");
        assert_eq!(additive_ref_path("Meshes\\Foo/Bar.kf"), "meshes\\foo\\bar.kf");
        assert_eq!(additive_ref_path("foo/bar.kf"), "meshes\\foo\\bar.kf");
    }
}
