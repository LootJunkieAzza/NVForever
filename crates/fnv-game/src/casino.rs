//! The casinos (CSNO) and the player's standing at each: the record the casino games read
//! (docs/spec-notes/blackjack.md §2), the net chips won per casino, the winnings levels the
//! casinos' scripts read and the ban at the max winnings (§4), and whether a casino's game
//! opens for the player. Shared by the casino games (`crate::blackjack`, roulette, `crate::slots`).

use crate::blackjack::Card;
use crate::game::Game;
use fnv_sim::FormId;

/// A casino's record (CSNO; blackjack.md §2).
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
    /// DATA 8: the slot machine reel stops, symbols 1 to 6 and W (slots.md §2).
    pub reel_stops: [u32; 7],
    /// MOD2: the slot machine's model, under `meshes\` (slots.md §2).
    pub slot_model: Option<String>,
    /// ICON × 7: the slot symbols' pictures, under `textures\`, in the reel stops' order.
    pub symbols: Vec<String>,
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
                    for (i, n) in c.reel_stops.iter_mut().enumerate() {
                        *n = u32_at(d, 8 + 4 * i).unwrap_or(0);
                    }
                    c.decks = u32_at(d, 36).unwrap_or(0);
                    c.max_winnings = u32_at(d, 40).unwrap_or(0);
                    c.chip = fid(u32_at(d, 44));
                    c.quest = fid(u32_at(d, 48));
                    c.stay_soft17 = u32_at(d, 52).is_some_and(|f| f & 1 != 0);
                }
                b"MOD3" => c.table_model = Some(zstring(&s.data)).filter(|p| !p.is_empty()),
                b"MOD4" => c.roulette_model = Some(zstring(&s.data)).filter(|p| !p.is_empty()),
                b"MOD2" => c.slot_model = Some(zstring(&s.data)).filter(|p| !p.is_empty()),
                b"ICON" => c.symbols.push(zstring(&s.data)),
                b"ICO2" => c.back_texture = Some(zstring(&s.data)).filter(|p| !p.is_empty()),
                _ => {}
            }
        }
        c
    }

    /// A card's face picture: the back's folder and deck letter with the card's code
    /// (`terminals\NV_PlayingCards\DeckG\cardG_Back.dds` → `…\cardG_h01.dds`) [observed:
    /// the archives' names, blackjack.md §2].
    pub fn face_texture(&self, card: Card) -> Option<String> {
        let back = self.back_texture.as_deref()?;
        let lower = back.to_ascii_lowercase();
        let cut = lower.rfind("_back")?;
        Some(format!("{}_{}{}", &back[..cut], card.texture_code(), &back[cut + 5..]))
    }
}

fn zstring(b: &[u8]) -> String {
    // In this run's code page (fnv_util::text, #1439).
    fnv_util::text::decode_z(b).trim().to_string()
}

/// The winnings level (0 to 4) of `earnings` at a casino whose max winnings are `max`: 4
/// from the max on, else a quarter of the max per level (blackjack.md §4) [documented: 4
/// at the max; hypothesis: the quarters].
pub fn winnings_level(earnings: i64, max: u32) -> i32 {
    if max == 0 || earnings <= 0 {
        return 0;
    }
    if earnings >= max as i64 {
        return 4;
    }
    ((earnings * 4) / max as i64).clamp(0, 3) as i32
}

/// The shared script/dialogue view of saved standing. No Casino Bans exposes the
/// last reward instead of the ban without erasing earned standing (blackjack.md §4).
pub fn script_winnings_level(g: &Game<'_>, casino: FormId) -> i32 {
    let level = g.player.casino_levels.get(&casino).copied().unwrap_or(0);
    if g.tweaks.no_casino_bans && level == 4 { 3 } else { level }
}

/// Source-name `SetCasinoWinningsLevel` (blackjack.md §4): a no-op on an untouched
/// casino, and on a tracked casino sets net earnings to `level * max_winnings / 4`
/// alongside the saved standing [observed: 2026-11-22 live probe, #3474].
pub(crate) fn set_winnings_level(g: &mut Game, args: &[fnv_script::vm::XVal]) -> Result<fnv_script::vm::XVal, String> {
    use fnv_script::vm::XVal;
    let casino = args.first().and_then(|v| v.value().ok()?.as_form()).map(FormId).ok_or("casino is not a form")?;
    let level = args.get(1).ok_or("winnings level is missing")?.num().map_err(|e| e.to_string())?;
    if !g.data.casinos.contains_key(&casino) {
        return Err("not a casino".into());
    }
    if !level.is_finite() || level.fract() != 0.0 || !(0.0..=4.0).contains(&level) {
        return Err("winnings level must be an integer from 0 to 4".into());
    }
    if !g.player.casino_earnings.contains_key(&casino) && !g.player.casino_levels.contains_key(&casino) {
        return Ok(XVal::Num(0.0));
    }
    let level = level as i32;
    let max = max_winnings(g, casino, 0);
    let target_earnings = level_earnings(level, max);
    g.player.casino_earnings.insert(casino, target_earnings);
    g.player.casino_levels.insert(casino, level);
    Ok(XVal::Num(0.0))
}

/// Net winnings corresponding to `SetCasinoWinningsLevel casino level`: `level * max / 4`
/// [observed: 2026-11-22 live probe, #3474].
pub fn level_earnings(level: i32, max: u32) -> i64 {
    i64::from(level.clamp(0, 4)) * i64::from(max) / 4
}

/// Add a round's net to the player's earnings at a casino and set the winnings level the
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

/// The casino's chip: SetCasinoChip's for the session (`crate::nvse_jg3`), else the
/// record's (DATA currency).
pub fn chip(g: &Game, casino: FormId) -> Option<FormId> {
    g.other.casino_chips.get(&casino).copied().or_else(|| g.data.casinos.get(&casino)?.chip)
}

/// The max winnings a table plays to: the menu call's, else (0) the casino's own
/// [hypothesis: every table's script passes 0].
pub fn max_winnings(g: &Game, casino: FormId, asked: u32) -> u32 {
    if asked > 0 {
        return asked;
    }
    crate::nvse_lutana::casino(g, casino).map_or(0, |c| c.max_winnings)
}

/// Why the menu does not open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closed {
    /// Not a casino record.
    NoCasino,
    /// No chips of the casino carried.
    NoChips,
    /// Banned: the earnings reached the max winnings.
    Banned,
}

/// Whether the menu opens for the player: the casino known, its chips carried, not banned
/// (blackjack.md §1). `chips` the casino's chips the player carries; `max` the max winnings.
pub fn opening(g: &Game, casino: FormId, chips: u32, max: u32) -> Result<(), Closed> {
    if !g.data.casinos.contains_key(&casino) {
        return Err(Closed::NoCasino);
    }
    let earnings = g.player.casino_earnings.get(&casino).copied().unwrap_or(0);
    // The player's tweak: no ban [design: docs/design/tweaks.md].
    if !g.tweaks.no_casino_bans && (winnings_level(earnings, max) >= 4 || g.player.casino_levels.get(&casino).is_some_and(|&l| l >= 4)) {
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
    fn winnings_levels_climb_by_quarters_to_the_ban() {
        assert_eq!(winnings_level(0, 9000), 0);
        assert_eq!(winnings_level(-500, 9000), 0);
        assert_eq!(winnings_level(2249, 9000), 0);
        assert_eq!(winnings_level(2250, 9000), 1);
        assert_eq!(winnings_level(4500, 9000), 2);
        assert_eq!(winnings_level(8999, 9000), 3);
        assert_eq!(winnings_level(9000, 9000), 4);
        assert_eq!(winnings_level(50, 0), 0);
    }

    #[test]
    fn level_earnings_matches_observed_quarter_thresholds() {
        for (max, expected) in [
            (2500u32, [0i64, 625, 1250, 1875, 2500]),
            (5000, [0, 1250, 2500, 3750, 5000]),
            (10000, [0, 2500, 5000, 7500, 10000]),
        ] {
            for (level, &want) in expected.iter().enumerate() {
                let earnings = level_earnings(level as i32, max);
                assert_eq!(earnings, want);
                assert_eq!(winnings_level(earnings, max), level as i32);
            }
        }
        assert_eq!(level_earnings(4, 0), 0);
    }

    #[test]
    fn the_casino_record_reads() {
        let mut data = Vec::new();
        for v in [0.2f32.to_bits(), 1.5f32.to_bits(), 2, 2, 2, 2, 2, 2, 2, 3, 9000, 0x0013_9FE0, 0x0016_07FF, 0] {
            data.extend(v.to_le_bytes());
        }
        let sub = |sig: &[u8; 4], d: Vec<u8>| fnv_esm::Subrecord { sig: *sig, data: d, extended: false };
        let subs = vec![
            sub(b"FULL", b"Gommorah\0".to_vec()),
            sub(b"DATA", data),
            sub(b"MOD3", b"terminals\\NV_Blackjack\\NV_Blackjack-TableG.NIF\0".to_vec()),
            sub(b"MOD4", b"terminals\\NV_Roulette\\NV_Roulette-TableG.NIF\0".to_vec()),
            sub(b"ICO2", b"terminals\\NV_PlayingCards\\DeckG\\cardG_Back.dds\0".to_vec()),
            sub(b"MOD2", b"terminals\\NV_SlotMachine\\NV_SlotMachine-Minigame-Gom.NIF\0".to_vec()),
            sub(b"ICON", b"terminals\\NV_SlotMachine\\NV_SlotMachine-Symbol1.dds\0".to_vec()),
            sub(b"ICON", b"terminals\\NV_SlotMachine\\NV_SlotMachine-Symbol2.dds\0".to_vec()),
        ];
        let c = CasinoDef::from_raw(&subs, &Some);
        assert_eq!(c.name, "Gommorah");
        assert_eq!(c.roulette_model.as_deref(), Some("terminals\\NV_Roulette\\NV_Roulette-TableG.NIF"));
        assert_eq!((c.decks, c.max_winnings, c.stay_soft17), (3, 9000, false));
        assert!((c.shuffle_share - 0.2).abs() < 1e-6 && (c.payout - 1.5).abs() < 1e-6);
        assert_eq!(c.chip, Some(FormId(0x0013_9FE0)));
        assert_eq!(c.reel_stops, [2; 7]);
        assert_eq!(c.slot_model.as_deref(), Some("terminals\\NV_SlotMachine\\NV_SlotMachine-Minigame-Gom.NIF"));
        assert_eq!(c.symbols.len(), 2);
        assert_eq!(c.face_texture(Card::parse("Ah").unwrap()).as_deref(), Some("terminals\\NV_PlayingCards\\DeckG\\cardG_h01.dds"));
    }
}
