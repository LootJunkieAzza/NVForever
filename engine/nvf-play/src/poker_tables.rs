//! Poker tables in the world (docs/design/poker.md): the original has no poker, so
//! NVForever turns some of the Strip casinos' blackjack tables into poker tables [design].
//!
//! - A swapped table ([`swapped`]) is a placed blackjack table: its script still asks for
//!   blackjack (ShowBlackJackMenuParams), and the engine opens poker at its casino instead
//!   (`crate::blackjack`'s `scripts_open`); it goes by "Poker Table" ([`name`]), and the
//!   engine draws a poker table's top over its blackjack felt (nvforever `poker_view.rs`).
//! - Poker plays for caps at every table; the casino gives the cards and the felt.
//! - **Off by default** [design]: the setting `poker-tables` (Gameplay > Game) turns the
//!   swap on; off, every table is the original's blackjack table.

use crate::poker::Casino;
use bevy::prelude::*;
use crate::sim::SimState;
use fnv_sim::FormId;

/// What a poker table is called at the crosshair, in the chosen language.
pub fn name() -> &'static str {
    fnv_util::words::tr("Poker Table")
}

/// What the crosshair offers at a poker table.
pub fn prompt() -> &'static str {
    fnv_util::words::tr("Play Poker")
}

/// Which of a casino's placed blackjack tables become poker tables [design]: the tables are
/// the references to the base object named `base_edid` (one per casino in the base game);
/// `keep` of them, the ones lying furthest toward `toward` along `axis` (0 x, 1 y), swap.
/// So the Ultra-Luxe's two northmost tables, Gomorrah's westmost and the Tops' southmost.
struct Rule {
    base_edid: &'static str,
    casino: Casino,
    keep: usize,
    axis: usize,
    toward: f32,
}

const RULES: [Rule; 3] = [
    Rule { base_edid: "BlackjackTableUltraLuxe", casino: Casino::UltraLuxe, keep: 2, axis: 1, toward: 1.0 },
    Rule { base_edid: "BlackjackTableGomorrah", casino: Casino::Gomorrah, keep: 1, axis: 0, toward: -1.0 },
    Rule { base_edid: "BlackjackTableTheTops", casino: Casino::Tops, keep: 1, axis: 1, toward: -1.0 },
];

/// The poker tables among placed blackjack tables, given as (reference, index into
/// [`RULES`], position): each rule keeps its tables furthest toward its side.
fn choose(placed: impl IntoIterator<Item = (u32, usize, [f32; 3])>) -> Vec<(u32, Casino)> {
    let mut by_rule: Vec<Vec<(u32, [f32; 3])>> = vec![Vec::new(); RULES.len()];
    for (r, rule, pos) in placed {
        by_rule[rule].push((r, pos));
    }
    let mut out = Vec::new();
    for (rule, mut tables) in RULES.iter().zip(by_rule) {
        tables.sort_by(|a, b| (b.1[rule.axis] * rule.toward).total_cmp(&(a.1[rule.axis] * rule.toward)).then(a.0.cmp(&b.0)));
        out.extend(tables.into_iter().take(rule.keep).map(|(r, _)| (r, rule.casino)));
    }
    out
}

/// The loaded world's poker tables (found once per world: the cache is keyed by the
/// reference table's address).
pub fn swapped(sim: &SimState) -> std::sync::Arc<Vec<(u32, Casino)>> {
    type Found = std::sync::Arc<Vec<(u32, Casino)>>;
    static CACHE: std::sync::Mutex<Option<(usize, Found)>> = std::sync::Mutex::new(None);
    let key = sim.refs as *const _ as usize;
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((k, v)) = cache.as_ref() {
        if *k == key {
            return v.clone();
        }
    }
    let bases: Vec<Option<FormId>> = RULES.iter().map(|r| sim.forms.edids.get(&r.base_edid.to_ascii_lowercase()).copied()).collect();
    let found = choose(sim.refs.refs.values().filter_map(|p| bases.iter().position(|b| *b == Some(p.base)).map(|i| (p.id.0, i, p.pos))));
    let v = std::sync::Arc::new(found);
    *cache = Some((key, v.clone()));
    v
}

static ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the swapped tables are poker tables (the `poker-tables` setting).
pub fn on() -> bool {
    ON.load(std::sync::atomic::Ordering::Relaxed)
}

/// Switch the poker tables on or off.
pub fn set_on(on: bool) {
    ON.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The poker casino of a table reference, if it is a poker table (none while the setting
/// is off).
pub fn casino_of(sim: &SimState, table: FormId) -> Option<Casino> {
    if !on() {
        return None;
    }
    swapped(sim).iter().find(|(r, _)| *r == table.0).map(|(_, c)| *c)
}

pub fn is_poker_table(sim: &SimState, table: FormId) -> bool {
    casino_of(sim, table).is_some()
}

pub struct PokerTablesPlugin;

impl Plugin for PokerTablesPlugin {
    fn build(&self, app: &mut App) {
        crate::settings::add_setting(
            app,
            crate::settings::Setting {
                key: "poker-tables",
                group: crate::pause::GROUP_MENUS,
                label: "Poker tables",
                note: "On: a few of the Strip casinos' blackjack tables become poker tables. Off: every table plays blackjack, as in the original.",
                switches: "",
                get: |_| on(),
                set: |_, on| set_on(on),
                slider: None,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_casino_keeps_blackjack_tables_of_its_own() {
        // Six Ultra-Luxe tables at y 10..60, four of Gomorrah's at x 5..8, three of the
        // Tops' at y 1..3.
        let ul = (0..6u32).map(|i| (100 + i, 0usize, [0.0, 10.0 * (i as f32 + 1.0), 0.0]));
        let gom = (0..4u32).map(|i| (200 + i, 1usize, [8.0 - i as f32, 0.0, 0.0]));
        let tops = (0..3u32).map(|i| (300 + i, 2usize, [0.0, 3.0 - i as f32, 0.0]));
        let got = choose(ul.chain(gom).chain(tops));
        assert_eq!(got, vec![(105, Casino::UltraLuxe), (104, Casino::UltraLuxe), (203, Casino::Gomorrah), (302, Casino::Tops)]);
        for (c, of) in [(Casino::UltraLuxe, 6), (Casino::Gomorrah, 4), (Casino::Tops, 3)] {
            let n = got.iter().filter(|(_, k)| *k == c).count();
            assert!(n >= 1 && n * 2 < of + 1, "{c:?}: {n} of {of}");
        }
        assert!(choose([]).is_empty(), "no blackjack tables, no poker tables");
    }
}
