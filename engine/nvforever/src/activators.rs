//! Activators' on and off looks: radio receivers show whether they are switched on
//! (docs/spec-notes/activators.md §3). Drawn activators whose model has sequences carry
//! [`ActivatorRig`]; for a receiver (an activator naming a radio station, nvf-play
//! `radio`), [`drive`] follows the runtime's on state (`fnv_game` `radio_on`, false
//! for a destroyed receiver):
//!
//! - **Open and Close** (the vintage radios, `jukeboxdirty01.nif`): switched on plays
//!   "Open" and holds its end, switched off "Close" [hypothesis: as a door's Open and
//!   Close; the radios' Open lights the dial (emissive 0 → 1 in 0.67 s, with a flicker)
//!   and Close darkens it, and their stored look is the dark one, observed]. Never
//!   switched: the stored look. Timing and turning back halfway as a door
//!   (`fnv_collide::anim::DoorMotion`).
//! - **Idle only** (the jukeboxes `jukeboxdirtynv.nif`, `jukeboxclean01.nif`): the idle
//!   plays while on (lights flicker, notes and bubbles scroll); while off it stands still
//!   and the lights it drives are dark [hypothesis: by analogy with the radios' dark
//!   "Close" end; the model has no off look of its own, and the owner expects the
//!   jukebox to light up when on].
//!
//! Without the script runtime (no `--play`) nothing changes: idles play from load and
//! Open/Close models keep their stored look. The sequences scripts play by name
//! (PlayGroup Forward, Left, …) are object_anims.rs.

use bevy::prelude::*;
use nvf_assets::anim_play::Playing;
use nvf_assets::material_anim::RigMaterials;
use nvf_assets::model::PreparedModel;
use nvf_assets::nif_material::NifMaterial;
use nvf_play::doors::DoorTiming;
use nvf_play::radio::RadioPlaces;
use nvf_play::script::ScriptState;
use fnv_collide::anim::DoorMotion;
use fnv_sim::FormId;
use std::collections::HashMap;
use std::sync::Arc;

/// On the root of a drawn activator whose model has sequences.
#[derive(Component)]
pub struct ActivatorRig {
    pub form_id: u32,
    pub base: u32,
    model: Arc<PreparedModel>,
    source: String,
    nodes: HashMap<String, Entity>,
    /// Open/Close models: the clock, once the runtime's state is known.
    motion: Option<DoorMotion>,
    /// The sequence (true: "Open") and time last shown.
    shown: Option<(bool, f32)>,
    /// The on state last seen.
    on: Option<bool>,
}

impl ActivatorRig {
    pub fn new(form_id: u32, base: u32, model: Arc<PreparedModel>, source: &str, nodes: HashMap<String, Entity>) -> ActivatorRig {
        ActivatorRig { form_id, base, model, source: source.to_string(), nodes, motion: None, shown: None, on: None }
    }
}

/// Counts for the summary.
#[derive(Resource, Default)]
pub struct ActivatorStats {
    receivers: std::collections::BTreeSet<u32>,
    on: u32,
    off: u32,
}

/// Follow each drawn receiver's on state (module notes).
#[allow(clippy::type_complexity)]
pub fn drive(
    time: Res<Time>,
    places: Option<Res<RadioPlaces>>,
    script: Option<Res<ScriptState>>,
    mut stats: ResMut<ActivatorStats>,
    mut rigs: Query<(&mut ActivatorRig, Option<&mut Playing>, Option<&mut RigMaterials>)>,
    mut transforms: Query<&mut Transform>,
    mut materials: ResMut<Assets<NifMaterial>>,
) {
    let (Some(places), Some(script)) = (places, script) else { return };
    // Not before the scripts' first frame: until then (the start's loading screen) the
    // runtime holds the stored state, and a receiver its OnLoad switches on would count
    // as switched on while drawn.
    let Some(rt) = script.rt.as_ref().filter(|rt| rt.ticked) else { return };
    let dt = time.delta_secs();
    for (mut rig, playing, mut mats) in &mut rigs {
        let Some(receiver) = places.forms.receivers.get(&rig.base) else { continue };
        let id = FormId(rig.form_id);
        let on = rt.game.radio_on(id) && !rt.game.extra.get(&id).is_some_and(|e| e.destroyed);
        let clips = rig.model.rig.as_ref().and_then(|r| r.door.clone());
        if rig.on != Some(on) {
            let name = receiver.edid.clone().unwrap_or_else(|| format!("{:08X}", rig.base));
            let how = match (&clips, on) {
                (Some(c), true) => format!("plays {} ({:.2} s)", c.open.name, c.open.length()),
                (Some(c), false) if rig.on.is_some() => format!("plays {} ({:.2} s)", c.close.name, c.close.length()),
                (Some(_), false) => "keeps its stored look".to_string(),
                (None, true) => "plays its idle".to_string(),
                (None, false) => "holds its idle, lights dark".to_string(),
            };
            info!("activator: {:08X} ({name}) {}: {how}", rig.form_id, if on { "on" } else { "off" });
            if rig.on.is_some() {
                if on {
                    stats.on += 1;
                } else {
                    stats.off += 1;
                }
            }
            stats.receivers.insert(rig.form_id);
            rig.on = Some(on);
        }
        match clips {
            None => {
                // Idle only: play while on; still and dark while off.
                if let Some(mut p) = playing {
                    p.held = !on;
                }
                if let Some(m) = mats.as_mut() {
                    m.dark = !on;
                }
            }
            Some(clips) => {
                let timing = DoorTiming::new(
                    (clips.open.start, clips.open.stop, &clips.open.text_keys),
                    (clips.close.start, clips.close.stop, &clips.close.text_keys),
                );
                // First seen off: the stored look (shown as Close's end only once switched).
                let motion = rig.motion.get_or_insert_with(|| DoorMotion::new(false, timing.open_end, timing.close_end));
                motion.set_open(on);
                motion.update(dt);
                let want = motion.pose();
                if rig.shown == Some(want) || (rig.shown.is_none() && !on) {
                    continue;
                }
                let clip = if want.0 { &clips.open } else { &clips.close };
                let mut p = Playing::new(&rig.source, clip, |n| rig.nodes.get(n).copied());
                p.time = clip.start + want.1;
                p.step(0.0, true, &mut transforms);
                if let Some(m) = mats.as_mut() {
                    if !m.slots.iter().all(|s| s.own) {
                        // Its own materials are not bound yet: pose it next frame.
                        continue;
                    }
                    m.apply(&clip.materials, clip.start + want.1, want.1, false, &mut materials);
                }
                rig.shown = Some(want);
            }
        }
    }
}

/// `activator summary: …` at exit.
pub fn report(mut exit: MessageReader<AppExit>, stats: Res<ActivatorStats>) {
    if exit.read().last().is_some() {
        info!(
            "activator summary: {} receivers drawn with an on/off look; switched on {} times, off {} times while drawn",
            stats.receivers.len(),
            stats.on,
            stats.off
        );
    }
}
