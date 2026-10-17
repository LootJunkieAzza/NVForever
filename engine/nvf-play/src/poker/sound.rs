//! The poker table's sound (docs/design/poker.md §4): while it is open, the casino's floor
//! as the game sounds it — its background loop, now and then a murmur, a laugh, the card
//! tables and the slot machines — and its music, the casino's own radio station
//! (`crate::radio::TableStation`). The world's music, loops and radios are quiet meanwhile
//! (`crate::music`, `crate::ambient`, `crate::radio`).
//!
//! - **The table's own sounds**: the cards and chips of the blackjack and roulette
//!   minigames, played as the table's events come (`super::Session::cues`): a shuffle and a
//!   card dealt at a time as a hand starts, the board's cards dealt and turned over, chips
//!   for blinds, calls and raises, a tap on the felt for a check, the cards pushed in for a
//!   fold, the pot gathered, the blackjack win and lose stings for the player, chips up or
//!   down as the raise changes, the roulette's refusal for a move not allowed [design: which
//!   sound goes with which moment].
//!
//! The sounds are the game's records, by Editor ID [observed: `fnvtool esm decode`,
//! 2026-10-17: the `AMB…LP` loops have the Loop flag; the `…OS`, `EMTGameCards…` and
//! `…Slots` records play a file of their folder at random]; how often each plays is
//! NVForever's [design]: the game rolls a placed sound's Random Chance once a second
//! where its marker stands, and the table has no markers.

use super::{Casino, PokerScreen};
use crate::pause::AidTime;
use crate::radio::TableStation;
use crate::sfx::{LoopSound, PlaySound, Sfx};
use bevy::prelude::*;
use fnv_game::poker::unit;
use fnv_sim::FormId;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The casinos' radio stations (TACT) [observed: the RNAM of the casinos' loudspeakers
/// `vCasinoTopsSpeaker`, `vCasinoGomorrahSpeaker`, `vCasinoUltraLuxeSpeaker`].
const TOPS_RADIO: u32 = 0x0014_E8DE;
const GOMORRAH_RADIO: u32 = 0x0016_B46D;
const ULTRA_LUXE_RADIO: u32 = 0x0016_B46E;

/// The table's own sounds [observed: the records' folders under `sound\fx\fx\games\`, and
/// `sound\fx\fol\the_gambler\` for the felt].
pub const SHUFFLE: &str = "GAMEBlackJackCardShuffle";
pub const DEAL: &str = "GAMEBlackJackCardDeal";
pub const FLIP: &str = "GAMEBlackJackCardFlip";
pub const FOLD: &str = "GAMEBlackJackCardCollect";
pub const COLLECT_DEALER: &str = "GAMEBlackJackCardCollectDealer";
pub const CHIPS: &str = "GAMEBlackJackIncreaseBet";
pub const CHECK: &str = "FolGmbleChipFelt";
pub const COLLECT_CHIPS: &str = "GAMEMiscCollectChips";
pub const WIN: &str = "GAMEBlackJackWin";
pub const LOSE: &str = "GAMEBlackJackLose";
pub const BET_UP: &str = "GAMERouletteIncreaseBet";
pub const BET_DOWN: &str = "GAMEBlackJackDecreaseBet";
pub const INVALID: &str = "GAMERouletteInvalidBet";

/// What a casino's floor sounds like at the table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ambience {
    /// Looping sounds, played all the while.
    pub loops: &'static [&'static str],
    /// Sounds played now and then: the record, and the least and most seconds between two.
    pub now_and_then: &'static [(&'static str, f32, f32)],
    /// The radio station playing the music, if the casino has one.
    pub station: Option<u32>,
}

impl Casino {
    /// The floor's sounds [observed: the records named; design: which go with which table,
    /// the Lucky 38's empty floor has no station, the plain deck sounds like Vikki and
    /// Vance's floor with the Tops' music].
    pub fn ambience(self) -> Ambience {
        match self {
            Casino::Tops => Ambience {
                loops: &["AMBTopsCasinoALP"],
                now_and_then: &[("AMBTopsCasinoAWallaOS", 5.0, 12.0), ("AMBTopsCasinoALaughOS", 10.0, 25.0), ("EMTGameCardsTopsA", 3.0, 7.0), ("AMBTopsSlots", 6.0, 14.0)],
                station: Some(TOPS_RADIO),
            },
            Casino::Gomorrah => Ambience {
                loops: &["AMBGomorrahCasinoALP"],
                now_and_then: &[("AMBGomorrahLaughsA", 10.0, 25.0), ("EMTGameCardsGomorrahMain", 3.0, 7.0), ("AMBGomorrahSlots", 6.0, 14.0)],
                station: Some(GOMORRAH_RADIO),
            },
            Casino::UltraLuxe => Ambience { loops: &["AMBULCasinoLP", "AMBULWallaCasinoLP"], now_and_then: &[("EMTGameCardsUltraluxe", 4.0, 9.0)], station: Some(ULTRA_LUXE_RADIO) },
            Casino::Lucky38 => Ambience { loops: &["AMBLucky38CasinoLP"], now_and_then: &[], station: None },
            Casino::Plain => Ambience {
                loops: &["AMBVikkiAndVanceWallaLP"],
                now_and_then: &[("EMTGameCardsVikkiAndVance", 3.0, 7.0), ("AMBVikkiAndVanceSlots", 6.0, 14.0)],
                station: Some(TOPS_RADIO),
            },
        }
    }
}

/// The sounds playing: the loops started, each now-and-then sound and when it next plays.
struct Playing {
    loops: Vec<FormId>,
    next: Vec<(FormId, f32, f32, f32)>,
}

#[derive(Default)]
pub struct State {
    playing: Option<Playing>,
    rng: Option<ChaCha8Rng>,
    /// Records found by Editor ID (`None`: not in the plugins, reported once).
    ids: std::collections::HashMap<&'static str, Option<FormId>>,
}

fn find(sfx: &Sfx, edid: &str) -> Option<FormId> {
    sfx.forms.sounds.iter().find(|(_, s)| s.edid.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(edid))).map(|(id, _)| *id)
}

pub fn sound(
    mut screen: ResMut<PokerScreen>,
    sfx: Option<Res<Sfx>>,
    time: AidTime,
    mut table: ResMut<TableStation>,
    mut loops: MessageWriter<LoopSound>,
    mut play: MessageWriter<PlaySound>,
    mut st: Local<State>,
) {
    let open = screen.0.as_ref().filter(|s| s.leaving.is_none());
    let now = time.elapsed_secs();
    match (open, st.playing.is_some()) {
        (Some(s), false) => {
            let Some(sfx) = sfx else { return };
            let a = s.casino.ambience();
            let mut missing = Vec::new();
            let mut found = |e: &str| {
                let id = find(&sfx, e);
                if id.is_none() {
                    missing.push(e.to_string());
                }
                id
            };
            let ids: Vec<FormId> = a.loops.iter().filter_map(|e| found(e)).collect();
            let random: Vec<(FormId, f32, f32)> = a.now_and_then.iter().filter_map(|&(e, lo, hi)| Some((found(e)?, lo, hi))).collect();
            if !missing.is_empty() {
                warn!("poker sound: not in the plugins: {}", missing.join(", "));
            }
            for &id in &ids {
                loops.write(LoopSound { sound: id, on: true });
            }
            let rng = st.rng.get_or_insert_with(|| ChaCha8Rng::seed_from_u64(now.to_bits() as u64));
            let next = random.iter().map(|&(id, lo, hi)| (id, lo, hi, now + lo * unit(rng))).collect();
            table.0 = a.station;
            info!(
                "poker sound: {}: loops {}, now and then {}, station {}",
                s.casino.title(),
                a.loops.join(", "),
                a.now_and_then.iter().map(|x| x.0).collect::<Vec<_>>().join(", "),
                a.station.map_or("none".to_string(), |b| format!("{b:08X}"))
            );
            st.playing = Some(Playing { loops: ids, next });
        }
        (Some(_), true) => {
            // The table's own sounds that are due.
            if let (Some(s), Some(sfx)) = (screen.0.as_mut(), sfx.as_deref()) {
                let (due, later): (Vec<_>, Vec<_>) = s.cues.drain(..).partition(|c| c.0 <= now);
                s.cues = later;
                for (_, edid) in due {
                    let id = *st.ids.entry(edid).or_insert_with(|| {
                        let id = find(sfx, edid);
                        if id.is_none() {
                            warn!("poker sound: {edid} is not in the plugins");
                        }
                        id
                    });
                    if let Some(id) = id {
                        play.write(PlaySound { sound: id, at: None });
                    }
                }
            }
            let State { playing, rng, .. } = &mut *st;
            let (Some(p), Some(rng)) = (playing.as_mut(), rng.as_mut()) else { return };
            for (id, lo, hi, at) in &mut p.next {
                if now >= *at {
                    play.write(PlaySound { sound: *id, at: None });
                    *at = now + *lo + (*hi - *lo) * unit(rng);
                }
            }
        }
        (None, true) => {
            if let Some(p) = st.playing.take() {
                for id in p.loops {
                    loops.write(LoopSound { sound: id, on: false });
                }
            }
            table.0 = None;
            info!("poker sound: stopped");
        }
        (None, false) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_casino_sounds_like_somewhere() {
        for c in [Casino::Tops, Casino::Gomorrah, Casino::UltraLuxe, Casino::Lucky38, Casino::Plain] {
            let a = c.ambience();
            assert!(!a.loops.is_empty(), "{c:?}");
            assert!(a.now_and_then.iter().all(|&(_, lo, hi)| lo > 0.0 && hi >= lo));
        }
        assert_eq!(Casino::Tops.ambience().station, Some(TOPS_RADIO));
        assert_eq!(Casino::Lucky38.ambience().station, None);
    }
}
