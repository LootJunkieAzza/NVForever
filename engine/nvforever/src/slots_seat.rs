//! The player at a slot machine in the world (queue item 170, part A; where and when:
//! `nvf_play::slots::seat`): the body's slot machine idles and the win celebration, on
//! this player's own body and on the others' puppets.
//!
//! - **The idles** [observed: FalloutNV.esm IDLE records, findings §4]: `NVSlotMachine`
//!   (0x0015AB98) is a group with no file of its own under the stool-sitting tree; under it
//!   `NVSlotMachineInsertCoin` (0x0015AB99, `SpecialIdle_NVinsertCoin.kf`, 5.33 s) and
//!   `NVSlotMachineLoose` (0x0015CC26, `SpecialIdle_NVLoose.kf`, 3.33 s). So the body sits
//!   on the stool's own loop (the furniture's, `crate::furniture`) or stands, plays
//!   InsertCoin as each spin starts and Loose when a spin loses.
//! - **The win celebration** (item 127's cheer, `crate::match_end::CHEER`) plays for the
//!   length the spin's result gives (`fnv_game::slots::celebration`, 0.8 to 3 s) and is then
//!   faded out.
//! - **Upper body only** [design: owner, "torso only (stays seated)", "legs planted"]: all
//!   three play on the upper body (`crate::pipboy_check::upper_clip`): the legs keep the
//!   stool's pose or the standing idle, so the body neither floats off the stool nor
//!   slides (the InsertCoin and Loose files are made for the no-lean stool and would set
//!   their own seat height on the others).
//! - **The hook** (part B's `nvf_play::slots::SlotsSpin`): InsertCoin on `Started`,
//!   Loose on a losing `Result`, the cheer for a winning one's `celebrate` seconds.
//! - **Holding the seat**: while the menu is open the menu's keys (W spins, S bets the
//!   most) are not movement: they do not get the player up (`PlayerSeat::leave` cleared).
//! - **Co-op** [design: no wire change]: the stool goes out as any furniture
//!   (`PlayerState::furniture`), so the puppet sits on it with its getting-on and
//!   getting-off clips; the idle goes out in the emote's place (`PlayerState::emote`) with
//!   an id from [`SLOT_EMOTE`] on, which the emotes leave alone (`crate::emote_play`), and
//!   is played here on the puppet's upper body. The seated end view's reaction after the
//!   menu closes (`crate::match_end`, the cheer or the beaten look) goes out the same way,
//!   so the others see what this player's own screen shows [design: ship 14's laptop
//!   slots-coop exit, 2026-11-12: the puppet sat still through it, the last spin's Loose
//!   cut at the exit, while its player's screen showed the cheer].
//!
//! Log lines start `slots seat:`.

use crate::idles::{ActorIdle, PlayerIdles};
use crate::limbs::ActorRig;
use bevy::prelude::*;
use fnv_actor::idle_play::{IdleKeys, IdlePlay};
use fnv_game::idles::IdleSource;
use nvf_assets::actor::{bind_clip, ActorCtx, ActorParts};
use nvf_assets::anim_play::Clip;
use nvf_assets::model::ModelCtx;
use nvf_coop::net::{emote as phase, EmoteState};
use nvf_play::slots::seat::SeatPhase;
use nvf_play::slots::{SlotsScreen, SlotsSeat, SlotsSpin, SpinPhase};
use std::collections::HashMap;
use std::sync::Arc;

/// [observed: IDLE `NVSlotMachineInsertCoin` MODL].
pub const INSERT_COIN: &str = "meshes\\characters\\_male\\idleanims\\specialidle_nvinsertcoin.kf";
/// [observed: IDLE `NVSlotMachineLoose` MODL].
pub const LOOSE: &str = "meshes\\characters\\_male\\idleanims\\specialidle_nvloose.kf";

/// The first emote id the slot idles go out as (the catalogue's are 1 to its length,
/// `nvf_play::emotes::CATALOG`): [`SLOT_EMOTE`] + the [`Act`] number.
pub const SLOT_EMOTE: u8 = 240;

/// What the body does at the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Act {
    InsertCoin = 0,
    Loose = 1,
    Cheer = 2,
    /// The end view's beaten look (`crate::match_end::DEFEAT`), seated.
    Defeat = 3,
}

impl Act {
    pub fn path(self) -> &'static str {
        match self {
            Act::InsertCoin => INSERT_COIN,
            Act::Loose => LOOSE,
            Act::Cheer => crate::match_end::CHEER,
            Act::Defeat => crate::match_end::DEFEAT,
        }
    }

    pub fn id(self) -> u8 {
        SLOT_EMOTE + self as u8
    }

    pub fn from_id(id: u8) -> Option<Act> {
        match id.checked_sub(SLOT_EMOTE)? {
            0 => Some(Act::InsertCoin),
            1 => Some(Act::Loose),
            2 => Some(Act::Cheer),
            3 => Some(Act::Defeat),
            _ => None,
        }
    }
}

/// Whether an emote id is one of the slot idles (`crate::emote_play` leaves those alone).
pub fn is_slot_emote(id: u8) -> bool {
    Act::from_id(id).is_some()
}

/// This player's slot idle, as it goes to the others (`coop.rs`).
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct SlotIdle {
    /// The act, the idle's serial on the third-person body and when it began (real s).
    playing: Option<(Act, u32, f32)>,
    /// The celebration is faded out then (real s).
    cheer_until: Option<f32>,
    /// The clips are loaded for the machine now played.
    loaded: bool,
    /// What goes out: id 0 for none.
    pub state: EmoteState,
}

/// The upper-body clips, by path.
#[derive(Resource, Default)]
pub struct SlotClips {
    clips: HashMap<&'static str, Option<Arc<Clip>>>,
    /// Per puppet reference: the act it plays, its idle's serial and the act's time last
    /// heard.
    puppets: HashMap<u32, (u8, u32, f32)>,
}

/// How far back an act's time may go before it counts as played again from its start
/// (seconds): the same act begun anew, as the end view's cheer straight after a spin's
/// [design]. The time only grows while one act plays; a late packet is not this far back.
const RESTART_BACK: f32 = 0.5;

/// Whether a puppet's act, heard at `t` after `last` before, was begun again.
fn restarted(last: f32, t: f32) -> bool {
    t + RESTART_BACK < last
}

impl SlotClips {
    fn get(&mut self, ctx: &mut ActorCtx, act: Act) -> Option<Arc<Clip>> {
        let path = act.path();
        self.clips
            .entry(path)
            .or_insert_with(|| {
                let c = ctx.clip(path).map(|c| Arc::new(crate::pipboy_check::upper_clip(&c)));
                match &c {
                    Some(c) => info!("slots seat: {path}: {:.2}s, {} upper-body tracks", c.stop - c.start, c.tracks.len()),
                    None => warn!("slots seat: {path} does not load; that idle is left out"),
                }
                c
            })
            .clone()
    }
}

pub struct SlotsSeatPlugin;

impl Plugin for SlotsSeatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SlotIdle>()
            .init_resource::<SlotClips>()
            .add_systems(Update, (get_up, hold).chain().in_set(nvf_play::player::BodySet).before(crate::furniture::drive_player))
            .add_systems(Update, (own, puppets).chain().after(nvf_play::PlaySet::Ui));
    }
}

/// The menu's keys are not movement while it is open, nor while the end view plays on
/// the stool.
fn hold(screen: Res<SlotsScreen>, slots_seat: Res<SlotsSeat>, mut seat: ResMut<nvf_play::furniture::PlayerSeat>) {
    let at = screen.0.is_some() || matches!(slots_seat.phase, SeatPhase::Sitting | SeatPhase::Ending);
    if at && seat.leave && slots_seat.phase != SeatPhase::Leaving {
        seat.leave = false;
    }
}

/// How long the stool waits for an end view to begin after the menu closed: a visit with
/// nothing played has none [engineering choice: `match_end::ends` sees the menu closed
/// within a frame or two].
const END_VIEW_WAIT: f32 = 0.5;
/// The longest the player stays on the stool for the end view, whatever happens (real
/// seconds) [design: the timed view 3.5 s; it cuts back with no fade, `match_end::on_close`].
const END_VIEW_MAX: f32 = 8.0;

/// The menu closed on the stool (`SeatPhase::Ending`): the visit's end view plays seated
/// (`match_end.rs`); once it is over (cut straight back), or if none begins, the player gets
/// up by the getting-off clip [owner 2026-11-11: the end view "in the seat", the get-off
/// after it].
fn get_up(
    real: Res<Time<Real>>,
    time: nvf_play::pause::AidTime,
    match_end: Option<Res<crate::match_end::MatchEnd>>,
    mut slots_seat: ResMut<SlotsSeat>,
    mut seat: ResMut<nvf_play::furniture::PlayerSeat>,
    mut ending: Local<Option<(f32, bool)>>,
) {
    if slots_seat.phase != SeatPhase::Ending {
        *ending = None;
        return;
    }
    let now = real.elapsed_secs();
    let (since, seen) = ending.get_or_insert((now, false));
    let showing = match_end.as_deref().is_some_and(|m| m.phase != crate::match_end::Phase::Off);
    *seen |= showing;
    let over = if *seen { !showing } else { now - *since > END_VIEW_WAIT };
    if !over && now - *since < END_VIEW_MAX {
        return;
    }
    let stool = match slots_seat.stance {
        Some(nvf_play::slots::Stance::Seated { stool }) => stool,
        _ => 0,
    };
    info!("slots seat: the player gets up from stool {stool:08X}{}", if *seen { " after the end view" } else { " (no end view)" });
    seat.leave = true;
    *slots_seat = SlotsSeat { phase: SeatPhase::Leaving, since: time.elapsed_secs(), ..*slots_seat };
    *ending = None;
}

/// This player's body: the idle for each event, and what goes to the others.
#[allow(clippy::too_many_arguments)]
fn own(
    real: Res<Time<Real>>,
    mut spins: MessageReader<SlotsSpin>,
    slots_seat: Res<SlotsSeat>,
    match_end: Option<Res<crate::match_end::MatchEnd>>,
    mut idle: ResMut<SlotIdle>,
    mut clips: ResMut<SlotClips>,
    pi: Option<ResMut<PlayerIdles>>,
    (mut parts, mut models): (ResMut<ActorParts>, ModelCtx),
) {
    let now = real.elapsed_secs();
    let Some(mut pi) = pi else { return };
    // The slot machine menu holds the game (`idles::drive`), and with it the idles: the
    // cheer stood on its first frame, arms down [observed: slots-170-B camera-win-warm,
    // 43.25 to 46.2 s]. At the machine the body is seen, so its idles play on.
    let under = slots_seat.busy();
    if pi.under_menu != under {
        pi.under_menu = under;
    }
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    // At a machine: load them now, so the first spin does not wait for the files.
    if slots_seat.busy() && !idle.loaded {
        for a in [Act::InsertCoin, Act::Loose, Act::Cheer, Act::Defeat] {
            clips.get(&mut ctx, a);
        }
        idle.loaded = true;
    }
    idle.loaded &= slots_seat.busy();
    let fade = |idle: &mut SlotIdle, pi: &mut PlayerIdles| {
        idle.cheer_until = None;
        if let Some((a, serial, _)) = idle.playing.take() {
            if let Some(c) = pi.third.cur.as_mut().filter(|c| c.serial == serial && !c.play.ended()) {
                c.play.stop();
                info!("slots seat: {a:?} faded out");
            }
        }
    };
    if idle.cheer_until.is_some_and(|t| now >= t) {
        fade(&mut idle, &mut pi);
    }
    for ev in spins.read() {
        let (act, passes, cheer) = match ev.phase {
            SpinPhase::Started { .. } => (Act::InsertCoin, 1, None),
            SpinPhase::Result { multiple: 0, .. } => (Act::Loose, 1, None),
            // Two passes at most; it is cut at its length.
            SpinPhase::Result { celebrate, .. } if celebrate > 0.0 => (Act::Cheer, 2, Some(now + celebrate)),
            SpinPhase::Result { .. } => continue,
            SpinPhase::Opened => continue,
            SpinPhase::Closed => {
                fade(&mut idle, &mut pi);
                continue;
            }
        };
        let Some(clip) = clips.get(&mut ctx, act) else { continue };
        let play = IdlePlay::new(IdleKeys::read(clip.start, clip.stop, &clip.text_keys), Some(passes));
        let serial = pi.third.start_clip(IdleSource::Script, clip, play);
        idle.playing = Some((act, serial, now));
        idle.cheer_until = cheer;
        info!("slots seat: the player plays {act:?} ({})", act.path());
    }
    // What the others see: the act while its idle plays; on the stool after the menu,
    // the end view's reaction while it plays.
    let playing = |serial: u32| pi.third.cur.as_ref().is_some_and(|c| c.serial == serial && !c.play.ended());
    let end_view = match_end.as_deref().filter(|_| slots_seat.phase == SeatPhase::Ending).and_then(|m| m.own_reaction()).filter(|&(serial, ..)| playing(serial));
    let state = match (idle.playing, end_view) {
        (Some((act, serial, since)), _) if playing(serial) => EmoteState { id: act.id(), variant: 0, phase: phase::ONCE, step: 0, t: now - since },
        (_, Some((_, mood, since))) => {
            idle.playing = None;
            let act = match mood {
                crate::match_end::Mood::Cheer => Act::Cheer,
                crate::match_end::Mood::Defeat => Act::Defeat,
            };
            EmoteState { id: act.id(), variant: 0, phase: phase::ONCE, step: 0, t: now - since }
        }
        _ => {
            idle.playing = None;
            EmoteState::default()
        }
    };
    idle.state = state;
}

/// The others' slot idles on their puppets' upper bodies.
fn puppets(
    mut commands: Commands,
    peers: Option<Res<crate::emote_play::PeerEmotes>>,
    mut clips: ResMut<SlotClips>,
    mut rigs: Query<(Entity, &ActorRig, Option<&mut ActorIdle>)>,
    (mut parts, mut models): (ResMut<ActorParts>, ModelCtx),
) {
    let Some(peers) = peers else { return };
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    let clips = &mut *clips;
    let mut seen = Vec::new();
    for &(reference, e) in &peers.0 {
        let Some(act) = Act::from_id(e.id) else { continue };
        seen.push(reference);
        // The same act still under way: its time follows. Begun again (the end view's
        // reaction after the same act from the last spin): played again from where its
        // player is, else the puppet ran on a second behind the player's own body
        // [observed: ship15 slots-coop-2p-exit, the host's end-view cheer at 15:00:33.75
        // and the guest's puppet of it still on the spin's cheer from 32.67].
        if let Some(p) = clips.puppets.get_mut(&reference).filter(|p| p.0 == e.id) {
            if !restarted(p.2, e.t) {
                p.2 = p.2.max(e.t);
                continue;
            }
            info!("slots seat: puppet {reference:08X}'s {act:?} begun again ({:.2}s after {:.2}s)", e.t, p.2);
        }
        let Some((ent, rig, idle)) = rigs.iter_mut().find(|(_, r, _)| r.reference == reference) else { continue };
        let Some(clip) = clips.get(&mut ctx, act) else { continue };
        let bound = Arc::new(bind_clip(&rig.skel, clip.clone()));
        let mut play = IdlePlay::new(IdleKeys::read(clip.start, clip.stop, &clip.text_keys), Some(if matches!(act, Act::Cheer | Act::Defeat) { 2 } else { 1 }));
        // Seen first under way: from where its player is.
        if e.t > 0.3 {
            play.advance(e.t);
        }
        let serial = match idle {
            Some(mut i) => i.0.start_clip(IdleSource::Script, bound, play),
            None => {
                let mut i = ActorIdle::default();
                let s = i.0.start_clip(IdleSource::Script, bound, play);
                commands.entity(ent).insert(i);
                s
            }
        };
        clips.puppets.insert(reference, (e.id, serial, e.t));
        info!("slots seat: puppet {reference:08X} plays {act:?}");
    }
    // Those whose act ended (the celebration cut at its length): fade it out.
    let ended: Vec<u32> = clips.puppets.keys().copied().filter(|r| !seen.contains(r)).collect();
    for reference in ended {
        let Some((id, serial, _)) = clips.puppets.remove(&reference) else { continue };
        if let Some((_, _, Some(mut idle))) = rigs.iter_mut().find(|(_, r, _)| r.reference == reference) {
            if let Some(c) = idle.0.cur.as_mut().filter(|c| c.serial == serial && !c.play.ended()) {
                c.play.stop();
                info!("slots seat: puppet {reference:08X} ends {:?}", Act::from_id(id));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_acts_have_emote_ids_past_the_catalogue() {
        assert!(nvf_play::emotes::CATALOG.len() < SLOT_EMOTE as usize);
        for a in [Act::InsertCoin, Act::Loose, Act::Cheer, Act::Defeat] {
            assert_eq!(Act::from_id(a.id()), Some(a));
            assert!(is_slot_emote(a.id()));
        }
        assert!(!is_slot_emote(1) && !is_slot_emote(0) && Act::from_id(SLOT_EMOTE + 4).is_none());
    }

    #[test]
    fn an_act_begun_again_is_told_from_a_late_packet() {
        // ship15: the spin's cheer 1.08 s in, the end view's from its start.
        assert!(restarted(1.08, 0.0));
        assert!(!restarted(1.08, 1.0) && !restarted(1.08, 1.2) && !restarted(0.2, 0.0));
    }
}
