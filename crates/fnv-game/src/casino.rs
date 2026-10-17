//! The casinos (CSNO) and what their games share: the record, the player's earnings at
//! each casino, the winnings levels the casinos' scripts read and the ban
//! (docs/spec-notes/roulette.md §2 and §4; blackjack.md §2 and §4). The games themselves:
//! `crate::roulette` (and blackjack's).

use crate::game::Game;
use fnv_sim::FormId;

/// A casino's record (CSNO).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CasinoDef {
    pub name: String,
    /// The share of the shoe dealt before it is shuffled again (1.0: all of it).
    pub shuffle_share: f32,
    pub payout: f32,
    pub decks: u32,
    pub max_winnings: u32,
    /// The casino's chip (CHIP), the currency of its tables.
    pub chip: Option<FormId>,
    pub quest: Option<FormId>,
    /// The dealer stays on a soft 17 (else hits it).
    pub stay_soft17: bool,
    /// MOD3: the blackjack table's model, under `meshes\`.
    pub table_model: Option<String>,
    /// MOD4: the roulette table's model, under `meshes\` (roulette.md §2) [observed:
    /// `terminals\NV_Roulette\NV_Roulette-Table<x>.NIF` in all five casinos].
    pub roulette_model: Option<String>,
    /// ICO2: the deck's back, under `textures\`.
    pub back_texture: Option<String>,
}

impl CasinoDef {
    /// From the record's subrecords [documented: schemas/fnv/objects.json CSNO, from xEdit
    /// wbDefinitionsFNV.pas]; `map` turns a file's FormID into the load order's.
    pub fn from_raw(subs: &[fnv_esm::Subrecord], map: &dyn Fn(u32) -> Option<u32>) -> CasinoDef {
        let mut c = CasinoDef::default();
        let u32_at = |b: &[u8], o: usize| b.get(o..o + 4).and_then(|x| <[u8; 4]>::try_from(x).ok()).map(u32::from_le_bytes);
        let fid = |v: Option<u32>| v.filter(|&x| x != 0).and_then(map).map(FormId);
        for s in subs {
            match &s.sig {
                b"FULL" => c.name = zstring(&s.data),
                b"DATA" => {
                    let d = &s.data;
                    c.shuffle_share = u32_at(d, 0).map_or(0.0, f32::from_bits);
                    c.payout = u32_at(d, 4).map_or(0.0, f32::from_bits);
                    c.decks = u32_at(d, 36).unwrap_or(0);
                    c.max_winnings = u32_at(d, 40).unwrap_or(0);
                    c.chip = fid(u32_at(d, 44));
                    c.quest = fid(u32_at(d, 48));
                    c.stay_soft17 = u32_at(d, 52).is_some_and(|f| f & 1 != 0);
                }
                b"MOD3" => c.table_model = Some(zstring(&s.data)).filter(|p| !p.is_empty()),
                b"MOD4" => c.roulette_model = Some(zstring(&s.data)).filter(|p| !p.is_empty()),
                // The first ICO2 (the record repeats it).
                b"ICO2" if c.back_texture.is_none() => c.back_texture = Some(zstring(&s.data)).filter(|p| !p.is_empty()),
                _ => {}
            }
        }
        c
    }
}

fn zstring(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).trim().to_string()
}

/// The winnings level (0 to 4) of `earnings` at a casino whose max winnings are `max`: 4
/// from the max on, else a quarter of the max per level [documented: 4 at the max, GECK
/// wiki GetCasinoWinningsLevel; hypothesis: the quarters].
pub fn winnings_level(earnings: i64, max: u32) -> i32 {
    if max == 0 || earnings <= 0 {
        return 0;
    }
    if earnings >= max as i64 {
        return 4;
    }
    ((earnings * 4) / max as i64).clamp(0, 3) as i32
}

/// Add a game's net to the player's earnings at a casino and set the winnings level the
/// scripts read (GetCasinoWinningsLevel). The new earnings.
pub fn add_earnings(g: &mut Game, casino: FormId, net: i64, max: u32) -> i64 {
    let e = g.player.casino_earnings.entry(casino).or_insert(0);
    *e += net;
    let earnings = *e;
    let level = winnings_level(earnings, max);
    // A level once reached stays (the comps and the ban are not taken back) [hypothesis].
    let l = g.player.casino_levels.entry(casino).or_insert(0);
    *l = (*l).max(level);
    earnings
}

/// The max winnings a table plays to: the menu call's, else (0) the casino's own
/// [hypothesis: every table's script passes 0].
pub fn max_winnings(g: &Game, casino: FormId, asked: u32) -> u32 {
    if asked > 0 {
        return asked;
    }
    g.data.casinos.get(&casino).map_or(0, |c| c.max_winnings)
}

/// Why a casino's menu does not open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closed {
    /// Not a casino record.
    NoCasino,
    /// No chips of the casino carried.
    NoChips,
    /// Banned: the earnings reached the max winnings.
    Banned,
}

/// Whether a casino's menu opens for the player: the casino known, its chips carried, not
/// banned [documented: GECK wiki ShowBlackJackMenuParams, "Player must meet Casino
/// requirements … such as having Casino Chips in the inventory and not being banned"].
/// `chips` the casino's chips the player carries; `max` the max winnings.
pub fn opening(g: &Game, casino: FormId, chips: u32, max: u32) -> Result<(), Closed> {
    if !g.data.casinos.contains_key(&casino) {
        return Err(Closed::NoCasino);
    }
    let earnings = g.player.casino_earnings.get(&casino).copied().unwrap_or(0);
    if winnings_level(earnings, max) >= 4 || g.player.casino_levels.get(&casino).is_some_and(|&l| l >= 4) {
        return Err(Closed::Banned);
    }
    if chips == 0 {
        return Err(Closed::NoChips);
    }
    Ok(())
}

/// The line shown when a casino's menu does not open [hypothesis: the original's words are
/// not in the plugins; these are NVForever's].
pub fn closed_line(c: Closed, casino: &str) -> String {
    match c {
        Closed::NoCasino => "This table is closed.".into(),
        Closed::NoChips => format!("You need {casino} chips to play."),
        Closed::Banned => format!("You have been banned from playing at {casino}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_climb_by_quarters_to_the_ban() {
        assert_eq!(winnings_level(0, 10000), 0);
        assert_eq!(winnings_level(-500, 10000), 0);
        assert_eq!(winnings_level(2499, 10000), 0);
        assert_eq!(winnings_level(2500, 10000), 1);
        assert_eq!(winnings_level(7600, 10000), 3);
        assert_eq!(winnings_level(10000, 10000), 4);
        assert_eq!(winnings_level(5000, 0), 0);
    }

    #[test]
    fn the_record_reads_both_tables() {
        let sub = |sig: &[u8; 4], data: &[u8]| fnv_esm::Subrecord { sig: *sig, data: data.to_vec(), extended: false };
        let mut data = vec![0u8; 56];
        data[40..44].copy_from_slice(&10000u32.to_le_bytes());
        data[44..48].copy_from_slice(&0x0014_4C3Bu32.to_le_bytes());
        let subs = [
            sub(b"FULL", b"The Tops\0"),
            sub(b"DATA", &data),
            sub(b"MOD3", b"terminals\\NV_Blackjack\\NV_Blackjack-Table.NIF\0"),
            sub(b"MOD4", b"terminals\\NV_Roulette\\NV_Roulette-Table.NIF\0"),
        ];
        let c = CasinoDef::from_raw(&subs, &|x| Some(x));
        assert_eq!(c.name, "The Tops");
        assert_eq!(c.max_winnings, 10000);
        assert_eq!(c.chip, Some(FormId(0x0014_4C3B)));
        assert_eq!(c.roulette_model.as_deref(), Some("terminals\\NV_Roulette\\NV_Roulette-Table.NIF"));
    }
}
