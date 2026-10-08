//! Game time, kept in the engine's time globals.
//!
//! The masters define the clock as globals [observed, FalloutNV.esm]: GameYear 0x35
//! (2281), GameMonth 0x36 (9), GameDay 0x37 (18), GameHour 0x38 (12), GameDaysPassed 0x39
//! (5.0), TimeScale 0x3A (30). Scripts read and set them (Layer 5), so the simulation
//! keeps time *in* those globals rather than in a separate counter.
//!
//! Rules:
//! - TimeScale is game seconds per real second (30 → one real minute is 30 game
//!   minutes) [hypothesis; GECK wiki "TimeScale" describes it as the game-time ratio].
//! - Advancing by h game hours adds h to GameHour and h / 24 to GameDaysPassed; each
//!   wrap of GameHour past 24 advances GameDay, rolling the month and year over with
//!   Gregorian month lengths, GameMonth counting from 0 (January). [observed in game for
//!   the day and month: day 18 + 30 h → day 19; day 31 of month 9 at 20:00 + 6 h → day
//!   1 of month 10 (xNVSE capture, `L4.sim.trace.goodsprings_weight_and_time`). The
//!   GameHour/GameDaysPassed arithmetic and the year rollover are still hypothesis: the
//!   in-game clock keeps running during menus, so those were not captured.]

/// Editor IDs of the time globals.
pub const GAME_YEAR: &str = "GameYear";
pub const GAME_MONTH: &str = "GameMonth";
pub const GAME_DAY: &str = "GameDay";
pub const GAME_HOUR: &str = "GameHour";
pub const GAME_DAYS_PASSED: &str = "GameDaysPassed";
pub const TIME_SCALE: &str = "TimeScale";

/// Calendar values (all as stored in the globals).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calendar {
    pub year: f32,
    pub month: f32,
    pub day: f32,
    pub hour: f32,
    pub days_passed: f32,
}

fn days_in_month(year: i64, month0: i64) -> i64 {
    match month0 {
        1 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        3 | 5 | 8 | 10 => 30,
        _ => 31,
    }
}

impl Calendar {
    /// Advance by `hours` game hours (negative values are ignored).
    pub fn advance(mut self, hours: f64) -> Calendar {
        if hours <= 0.0 || !hours.is_finite() {
            return self;
        }
        self.days_passed = (self.days_passed as f64 + hours / 24.0) as f32;
        let mut hour = self.hour as f64 + hours;
        let (mut year, mut month, mut day) = (self.year as i64, self.month as i64, self.day as i64);
        while hour >= 24.0 {
            hour -= 24.0;
            day += 1;
            if day > days_in_month(year, month) {
                day = 1;
                month += 1;
                if month > 11 {
                    month = 0;
                    year += 1;
                }
            }
        }
        self.hour = hour as f32;
        (self.year, self.month, self.day) = (year as f32, month as f32, day as f32);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollover() {
        let c = Calendar { year: 2281.0, month: 9.0, day: 31.0, hour: 23.0, days_passed: 5.0 };
        let n = c.advance(2.0);
        assert_eq!((n.year, n.month, n.day, n.hour), (2281.0, 10.0, 1.0, 1.0));
        assert!((n.days_passed - (5.0 + 2.0 / 24.0)).abs() < 1e-6);
        let ny = Calendar { year: 2281.0, month: 11.0, day: 31.0, hour: 12.0, days_passed: 0.0 }.advance(24.0);
        assert_eq!((ny.year, ny.month, ny.day, ny.hour), (2282.0, 0.0, 1.0, 12.0));
        let leap = Calendar { year: 2284.0, month: 1.0, day: 28.0, hour: 0.0, days_passed: 0.0 }.advance(24.0);
        assert_eq!((leap.month, leap.day), (1.0, 29.0));
        assert_eq!(c.advance(-5.0), c);
        assert_eq!(c.advance(f64::NAN), c);
        let long = c.advance(24.0 * 400.0);
        assert!((long.days_passed - 405.0).abs() < 1e-3);
    }
}
