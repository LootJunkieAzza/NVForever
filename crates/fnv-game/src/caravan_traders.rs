//! Caravan with the traveling merchants: NVForever's own Caravan entry for the wandering
//! traders, which the original does not have (docs/spec-notes/caravan.md §1.1) [owner
//! request 2026-11-12, ship17: "you should be able to play caravan with all traveling
//! merchants"].
//!
//! - In the original, Caravan is the topic `CaravanOffer` ("Want to play a hand of
//!   Caravan?"), whose quest `vCaravanQuest` lets only members of `CaravanPlayerFaction`
//!   say it. Its lines name their players, and its two lines for anyone else start a game
//!   only for Johnson Nash, Little Buster, Dale Barton and Isaac [observed: FalloutNV.esm
//!   QUST 0x00157F76's one condition, INFO 0x00157F6D's result script]. The traveling
//!   merchants are not in the faction, so they never offer it [observed: NVMerchantMale's
//!   factions].
//! - A traveling merchant is the original's own: an actor of class `VendorTraveler`
//!   ("Traveler Merchant"), whose barter line "I'd like to see your wares." asks for that
//!   class, or a member of `vMojaveMerchantFaction` ("Mojave Traveler Merchants")
//!   [observed: FalloutNV.esm INFO 0x00163CA6's conditions, CLAS 0x0015C4D6, FACT
//!   0x0016ACA6]. No names are listed, so a merchant of another plugin made from these
//!   records counts too.
//! - Such a merchant, not already a Caravan player, is offered `CaravanOffer` with the
//!   data's line for anyone (0x00157F6D, "All right, let's see what you got."). Choosing it
//!   asks for a game against the merchant as ShowCaravanMenu would, with Ringo's deck and
//!   a quarter of its money at most, Ringo's own terms (he is a trader too) [design]. The
//!   line's own result script runs as written and starts nothing for them.

use crate::dialogue::Choice;
use crate::game::Game;
use fnv_sim::FormId;

/// The original's Caravan topic, "Want to play a hand of Caravan?" [observed: DIAL].
pub const CARAVAN_OFFER: FormId = FormId(0x0015_7F69);
/// Its line for a speaker it does not name, "All right, let's see what you got."
/// [observed: INFO, Goodbye].
pub const ANYONE_LINE: FormId = FormId(0x0015_7F6D);
/// The faction whose members may say the Caravan lines: `vCaravanQuest`'s condition.
pub const CARAVAN_PLAYERS: FormId = FormId(0x0015_7F77);
/// `VendorTraveler`, "Traveler Merchant": the class the barter line asks for.
pub const TRAVELER_CLASS: FormId = FormId(0x0015_C4D6);
/// `vMojaveMerchantFaction`, "Mojave Traveler Merchants".
pub const TRAVELER_FACTION: FormId = FormId(0x0016_ACA6);
/// The deck a merchant plays: Ringo's (`CaravanDeckRingo`) [design].
pub const TRAVELER_DECK: FormId = FormId(0x0015_D7A2);
/// The most of its money a merchant bets, as Ringo's line asks [observed: 0.25].
pub const TRAVELER_SHARE: f32 = 0.25;

impl Game<'_> {
    /// Whether `actor` is a traveling merchant: of the Traveler Merchant class, or in the
    /// Mojave Traveler Merchants faction.
    pub fn traveling_merchant(&self, actor: FormId) -> bool {
        self.traits_def(actor).and_then(|d| d.class) == Some(TRAVELER_CLASS) || crate::functions::actor::faction_rank(self, actor, TRAVELER_FACTION) >= 0
    }

    /// Whether NVForever offers `actor` its own Caravan entry: a traveling merchant that is
    /// not a Caravan player of the original's.
    pub fn merchant_plays_caravan(&self, actor: FormId) -> bool {
        self.traveling_merchant(actor) && crate::functions::actor::faction_rank(self, actor, CARAVAN_PLAYERS) < 0
    }

    /// The Caravan entry for a traveling merchant, unless the list has one already or the
    /// data has no such topic and line.
    pub(crate) fn merchant_caravan_choice(&self, actor: FormId, listed: &[Choice]) -> Option<Choice> {
        if !self.merchant_plays_caravan(actor) || listed.iter().any(|c| c.topic == CARAVAN_OFFER) {
            return None;
        }
        let dv = self.dialogue_view();
        dv.topic(&CARAVAN_OFFER)?;
        let def = dv.info(&ANYONE_LINE)?;
        Some(Choice {
            topic: CARAVAN_OFFER,
            info: ANYONE_LINE,
            chars: dv.choice_text(CARAVAN_OFFER, ANYONE_LINE).chars().count(),
            prompt: def.prompt.is_some(),
            goodbye: def.goodbye,
            darkened: false,
            challenge: None,
            dumb: false,
        })
    }

    /// A greeting with a choice list of its own (the merchants' "Hey there. Need
    /// anything?" lists "I'd like to see your wares." and "Goodbye." [observed: INFO
    /// 0x00163CA9 in a run]): the Caravan entry goes before the list's closing goodbye
    /// choice, else last. Lines after the greeting are left as written.
    pub(crate) fn merchant_caravan_in_list(&self, actor: FormId, topic: FormId, out: &mut Vec<Choice>) {
        if self.data.greeting != Some(topic) {
            return;
        }
        if let Some(c) = self.merchant_caravan_choice(actor, out) {
            let at = if out.last().is_some_and(|l| l.goodbye) { out.len() - 1 } else { out.len() };
            out.insert(at, c);
        }
    }

    /// The merchant's entry was chosen: a game against it, opened when the conversation
    /// is over (`caravan_requests`).
    pub(crate) fn merchant_caravan_chosen(&mut self, actor: FormId, choice: &Choice) {
        if choice.topic == CARAVAN_OFFER && choice.info == ANYONE_LINE && self.merchant_plays_caravan(actor) {
            self.caravan_requests.push(crate::caravan::CaravanRequest { opponent: actor, deck: TRAVELER_DECK, difficulty: 0, max_bet_share: TRAVELER_SHARE });
        }
    }
}
