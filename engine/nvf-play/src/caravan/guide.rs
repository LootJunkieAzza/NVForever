//! The Caravan guide (owner, playtest 119, item 163): the right-hand message area of the
//! table, under the player's deck and above the buttons, tells what the card in focus does,
//! what it will do at the place shown, and why a card cannot be played. Tips T) turns it
//! on and off; the choice is kept with the settings (`caravan-tips`).
//!
//! The rules it states are the game's (docs/spec-notes/caravan.md §5, from the in-game help
//! `HelpCaravan*` and the owner's rulings): numbered cards on one's own caravans, keeping
//! the caravan's direction or its suit and never the last card's value; the Jack removes a
//! card and its faces, the Queen reverses the direction and sets the suit, the King
//! doubles, the Joker clears a suit (on an Ace) or a value (on 2 to 10). The words are
//! NVForever's own (LEGAL.md).

use fnv_game::caravan::{Caravan, Card, Dir, Game, Move, Phase, Rank, Suit, OPPONENT, PLAYER};

/// The tracks' names, as the table shows them.
const PLAYER_NAMES: [&str; 3] = ["DAYGLOW", "NEW RENO", "THE HUB"];
const OPPONENT_NAMES: [&str; 3] = ["BONEYARD", "REDDING", "SHADY SANDS"];

pub(crate) fn track_name(side: usize, track: usize) -> &'static str {
    let names = if side == OPPONENT { OPPONENT_NAMES } else { PLAYER_NAMES };
    names.get(track).copied().unwrap_or("the caravan")
}

fn suit_name(s: Suit) -> &'static str {
    match s {
        Suit::Hearts => "hearts",
        Suit::Spades => "spades",
        Suit::Diamonds => "diamonds",
        Suit::Clubs => "clubs",
        Suit::Joker => "jokers",
    }
}

/// A card's name: "7", "Ace", "King".
pub fn card_name(c: &Card) -> String {
    match c.rank {
        Rank::Num(1) => "Ace".into(),
        Rank::Num(n) => n.to_string(),
        Rank::Jack => "Jack".into(),
        Rank::Queen => "Queen".into(),
        Rank::King => "King".into(),
        Rank::Joker => "Joker".into(),
    }
}

/// What a card does, in general.
pub fn what(c: &Card, opening: bool) -> String {
    match c.rank {
        Rank::Num(n) if opening => format!("{}: the opening puts one number card on each of your caravans; this one adds {n}.", card_name(c)),
        Rank::Num(n) => format!("{}: adds {n} to one of your caravans. It must keep the caravan's direction or match its suit, and can't repeat its last value.", card_name(c)),
        Rank::Jack => "Jack: removes the card it is played on, with any face cards on it.".into(),
        Rank::Queen => "Queen: on a caravan's last card, reverses its direction and changes its suit to the Queen's.".into(),
        Rank::King => "King: doubles the card it is played on; each further King doubles it again.".into(),
        Rank::Joker => "Joker: on an Ace, removes every other card of that suit; on a 2 to 10, every other card of that value.".into(),
    }
}

/// A caravan's value and whether it sells (21 to 26).
fn worth(v: u32) -> String {
    match v {
        21..=26 => format!("{v}, sold"),
        27.. => format!("{v}, over 26"),
        _ => v.to_string(),
    }
}

/// What a move will do: "King: doubles this 7 to 14".
pub fn effect(g: &Game, m: &Move) -> Option<String> {
    let Move::Play { card, side, track, on } = *m else {
        return match m {
            Move::DiscardTrack { track } => Some(format!("Discard Track: throws away every card on {}.", track_name(PLAYER, *track))),
            _ => None,
        };
    };
    let c = g.sides[PLAYER].hand.get(card)?;
    let k: &Caravan = g.sides.get(side)?.caravans.get(track)?;
    let name = track_name(side, track);
    let target = on.and_then(|i| k.cards.get(i));
    Some(match (c.rank, target) {
        (Rank::Num(n), _) => format!("{}: {name} goes from {} to {}.", card_name(c), k.value(), worth(k.value() + n as u32)),
        (Rank::King, Some(t)) => format!("King: doubles this {} to {}; {name} goes to {}.", card_name(&t.card), t.value() * 2, worth(k.value() + t.value())),
        (Rank::Jack, Some(t)) => format!("Jack: removes this {} ({} points) from {name}, which goes to {}.", card_name(&t.card), t.value(), worth(k.value() - t.value())),
        (Rank::Queen, Some(_)) => {
            let dir = match k.direction() {
                Some(Dir::Up) => "down",
                Some(Dir::Down) => "up",
                None => "the other way",
            };
            format!("Queen: {name} now goes {dir}, and its suit is {}.", suit_name(c.suit))
        }
        (Rank::Joker, Some(t)) => match t.card.rank {
            Rank::Num(1) => format!("Joker: removes every other {} card from the table.", suit_name(t.card.suit).trim_end_matches('s')),
            _ => format!("Joker: removes every other {} from the table.", card_name(&t.card)),
        },
        _ => what(c, g.phase == Phase::Opening),
    })
}

/// Why a number card can't go on one of the player's caravans, or `None` if it can.
fn refusal(k: &Caravan, c: &Card) -> Option<String> {
    let n = c.rank.number()?;
    let last = k.cards.last()?;
    if n == last.base() {
        return Some(format!("its last card is a {}", card_name(&last.card)));
    }
    if k.suit() == Some(c.suit) {
        return None;
    }
    let suit = k.suit().map_or("its suit", suit_name);
    match k.direction() {
        Some(Dir::Up) if n < last.base() => Some(format!("it goes up and the {} is lower; play higher or {suit}", card_name(c))),
        Some(Dir::Down) if n > last.base() => Some(format!("it goes down and the {} is higher; play lower or {suit}", card_name(c))),
        _ => None,
    }
}

/// Why the card can't be played anywhere now.
pub fn why_not(g: &Game, c: &Card) -> String {
    let opening = g.phase == Phase::Opening;
    let name = card_name(c);
    if c.rank.is_face() {
        if opening {
            return format!("Can't play the {name} yet: face cards wait until each of your caravans has its first card.");
        }
        if c.rank == Rank::Queen {
            return "Can't play the Queen: it goes on a caravan's last card, and no caravan has one.".into();
        }
        return format!("Can't play the {name}: it goes on a card on the table, and there is none.");
    }
    if opening {
        return format!("Can't play the {name}: in the opening it goes on an empty caravan of yours, and none is empty.");
    }
    let reasons: Vec<String> = g.sides[PLAYER].caravans.iter().enumerate().filter_map(|(t, k)| refusal(k, c).map(|r| format!("{}: {r}", track_name(PLAYER, t)))).collect();
    if reasons.is_empty() {
        format!("Can't play the {name} now.")
    } else {
        format!("Can't play the {name}. {}.", reasons.join(". "))
    }
}

/// The tracks tied in the sold range, said plainly: a tie sells neither caravan until one
/// is higher, and the game goes on until all three tracks are sold [documented:
/// HelpCaravanContractWar]. The owner took a 23 to 23 tie, with the other two tracks won,
/// for a game that should have ended (PvP playtest 2026-11-11). `None` without a tie.
pub fn tie_note(g: &Game) -> Option<String> {
    if g.phase != Phase::War {
        return None;
    }
    let tied: Vec<String> = (0..3)
        .filter_map(|t| {
            let (mine, theirs) = (g.sides[PLAYER].caravans[t].value(), g.sides[OPPONENT].caravans[t].value());
            (mine == theirs && (21..=26).contains(&mine)).then(|| format!("{} and {} are tied at {mine}", track_name(PLAYER, t), track_name(OPPONENT, t)))
        })
        .collect();
    (!tied.is_empty()).then(|| format!("{}: a tie sells neither until one is higher. The game ends when all three are sold.", tied.join("; ")))
}

/// The guide's text for the table as it stands: the place shown, else the card in focus
/// (why it can't be played, when it can't), on the player's turn; empty otherwise.
pub fn text(g: &Game, hand: usize, shown: Option<&Move>, refused: bool) -> String {
    if g.phase == Phase::Over || g.turn != PLAYER {
        return String::new();
    }
    if let Some(e) = shown.and_then(|m| effect(g, m)) {
        return e;
    }
    let Some(c) = g.sides[PLAYER].hand.get(hand) else { return String::new() };
    if refused {
        return why_not(g, c);
    }
    what(c, g.phase == Phase::Opening)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_sim::FormId;

    fn card(rank: Rank, suit: Suit) -> Card {
        Card::new(FormId(1), suit, rank)
    }

    fn placed(n: u8, suit: Suit) -> fnv_game::caravan::Placed {
        fnv_game::caravan::Placed { card: card(Rank::Num(n), suit), faces: Vec::new() }
    }

    #[test]
    fn the_guide_says_what_a_card_does_and_why_not() {
        assert!(what(&card(Rank::King, Suit::Hearts), false).starts_with("King: doubles"));
        assert!(what(&card(Rank::Joker, Suit::Joker), false).contains("on an Ace"));
        // A caravan going down (9 then 4) in clubs: a 6 of hearts is higher and off suit.
        let k = Caravan { cards: vec![placed(9, Suit::Clubs), placed(4, Suit::Clubs)] };
        assert_eq!(refusal(&k, &card(Rank::Num(6), Suit::Hearts)).unwrap(), "it goes down and the 6 is higher; play lower or clubs");
        assert_eq!(refusal(&k, &card(Rank::Num(6), Suit::Clubs)), None, "the suit matches");
        assert_eq!(refusal(&k, &card(Rank::Num(2), Suit::Hearts)), None, "lower keeps the direction");
        assert_eq!(refusal(&k, &card(Rank::Num(4), Suit::Clubs)).unwrap(), "its last card is a 4");
        assert_eq!(worth(23), "23, sold");
        assert_eq!(worth(30), "30, over 26");
    }

    #[test]
    fn a_tie_in_the_sold_range_is_said() {
        // The owner's table: 22/23/25 against 20/23/14; the middle tracks tie at 23.
        let k = |cards: &[(u8, Suit)]| Caravan { cards: cards.iter().map(|&(n, s)| placed(n, s)).collect() };
        let mut g = Game::new(Vec::new(), Vec::new(), &mut <rand_chacha::ChaCha8Rng as rand_core::SeedableRng>::seed_from_u64(1));
        g.phase = Phase::War;
        g.sides[PLAYER].caravans = [k(&[(10, Suit::Hearts), (10, Suit::Hearts), (2, Suit::Hearts)]), k(&[(10, Suit::Clubs), (10, Suit::Clubs), (3, Suit::Clubs)]), k(&[(10, Suit::Spades), (10, Suit::Spades), (5, Suit::Spades)])];
        g.sides[OPPONENT].caravans = [k(&[(10, Suit::Hearts), (10, Suit::Hearts)]), k(&[(10, Suit::Diamonds), (10, Suit::Diamonds), (3, Suit::Diamonds)]), k(&[(10, Suit::Clubs), (4, Suit::Clubs)])];
        let note = tie_note(&g).expect("a tie");
        assert!(note.starts_with("NEW RENO and REDDING are tied at 23"), "{note}");
        g.sides[OPPONENT].caravans[1].cards.pop();
        assert_eq!(tie_note(&g), None, "20 against 23 is no tie");
    }
}
