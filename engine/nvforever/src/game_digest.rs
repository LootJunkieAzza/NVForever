//! Test aid `NVF_PLAY_TEST_GAME_DIGEST=<seconds>[,<seconds>...]`: at each time (the game's
//! clock since the start, as the other timed aids), one line of what the game's state is,
//! so two runs that must agree game-side (the full-detail ring off and on, #2701) can be
//! compared line for line:
//!
//! `game digest t=<s>: cells <hash> residents-high <hash> triggers <n> zone-levels <n> onload <n> end`
//!
//! - `cells`: an order-independent hash of the cells the game has loaded
//!   (`Game::loaded_cells`, the scripts' loaded area);
//! - `residents-high`: the same of the residents in the high process (residents.rs, with
//!   `--npc-packages`; 0 without);
//! - `triggers`, `zone-levels`, `onload`: actors that entered a trigger, encounter zones
//!   given their level (each zone's first entry, as a cell loads) and reference OnLoad
//!   blocks entered since the start (`Triggers::entered`, `Game::zone_levels_set`,
//!   `Game::onload_runs`);
//! - `end`: closes the line, so a check of the whole line anchors its last number.
//!
//! The hashes are [`crate::full_detail::set_hash`] of the IDs, in hex. It only logs: it is
//! one of the measuring aids (`nvf_play::MEASURING_AIDS`), so it drives nothing [design].

use bevy::prelude::*;

/// The environment variable of the aid.
pub const ENV: &str = "NVF_PLAY_TEST_GAME_DIGEST";

/// The times still to log, soonest first.
#[derive(Resource, Default)]
struct Due(Vec<f32>);

pub struct GameDigestPlugin;

impl Plugin for GameDigestPlugin {
    fn build(&self, app: &mut App) {
        let Ok(v) = std::env::var(ENV) else { return };
        match parse_times(&v) {
            Ok(times) => {
                info!("test aid {ENV}: a game digest at {} s", times.iter().map(f32::to_string).collect::<Vec<_>>().join(", "));
                app.insert_resource(Due(times)).add_systems(Last, log_digest);
            }
            Err(e) => warn!("{ENV}: {e}"),
        }
    }
}

/// `<seconds>[,<seconds>...]`, sorted.
pub fn parse_times(v: &str) -> Result<Vec<f32>, String> {
    let mut out = v
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<f32>().ok().filter(|t| t.is_finite() && *t >= 0.0).ok_or_else(|| format!("{s:?}: expected seconds")))
        .collect::<Result<Vec<f32>, String>>()?;
    out.sort_by(f32::total_cmp);
    Ok(out)
}

/// The digest line for time `t` from the game's state.
pub fn digest_line(t: f32, cells: impl Iterator<Item = u32>, high: impl Iterator<Item = u32>, triggers: u64, zone_levels: u64, onload: u64) -> String {
    let (cells, high) = (crate::full_detail::set_hash(cells.map(u64::from)), crate::full_detail::set_hash(high.map(u64::from)));
    format!("game digest t={t}: cells {cells:016x} residents-high {high:016x} triggers {triggers} zone-levels {zone_levels} onload {onload} end")
}

fn log_digest(time: Res<Time>, mut due: ResMut<Due>, script: Option<Res<nvf_play::script::ScriptState>>, residents: Option<Res<crate::residents::Residents>>) {
    let now = time.elapsed_secs();
    while due.0.first().is_some_and(|&t| t <= now) {
        let t = due.0.remove(0);
        let Some(rt) = script.as_deref().and_then(|s| s.rt.as_ref()) else {
            info!("game digest t={t}: no game running");
            continue;
        };
        let high = residents.as_deref().map(|r| r.high_process_ids()).unwrap_or_default();
        info!("{}", digest_line(t, rt.game.loaded_cells.iter().map(|c| c.0), high.into_iter(), rt.triggers.entered, rt.game.zone_levels_set, rt.game.onload_runs));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_parse_and_sort() {
        assert_eq!(parse_times("20, 8,14.5").unwrap(), vec![8.0, 14.5, 20.0]);
        assert!(parse_times("8,soon").is_err());
        assert!(parse_times("").unwrap().is_empty());
    }

    /// The same IDs in another order give the same line; another set another hash.
    #[test]
    fn the_digest_line_has_its_shape_and_ignores_order() {
        let a = digest_line(12.0, [3, 1, 2].into_iter(), [7].into_iter(), 4, 1, 30);
        let b = digest_line(12.0, [2, 3, 1].into_iter(), [7].into_iter(), 4, 1, 30);
        assert_eq!(a, b);
        assert!(a.starts_with("game digest t=12: cells "), "{a}");
        assert!(a.ends_with(" triggers 4 zone-levels 1 onload 30 end"), "{a}");
        assert!(!digest_line(12.0, [3, 1, 2].into_iter(), [7].into_iter(), 4, 1, 300).contains(" onload 30 end"), "the last number is anchored");
        assert_ne!(a, digest_line(12.0, [3, 1].into_iter(), [7].into_iter(), 4, 1, 30));
        assert!(digest_line(8.0, std::iter::empty(), std::iter::empty(), 0, 0, 0).contains("cells 0000000000000000 residents-high 0000000000000000"));
    }
}
