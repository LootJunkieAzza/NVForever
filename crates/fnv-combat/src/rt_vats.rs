//! Real-time VATS: the rules of the co-op lock-on (docs/spec-notes/realtime-vats.md).
//!
//! In a co-op game the clock cannot stop for one player, so the VATS key locks on to a
//! hostile instead: shots at the locked target land by the VATS hit chance of the limb
//! aimed at, Action Points drain while the lock is held and with every shot, a kill hands
//! the lock on to the next hostile, and hits charge a critical meter. The behaviour and
//! every default are those of the settings file of the RealtimeVatsNV mod the owner
//! asked for (queue item 196) [documented: its `RealtimeVatsNV.toml`]; nothing else of
//! the mod was read. What the settings leave open is decided here and tagged.
//!
//! Headless: the engine (`nvf-play`'s `rt_vats`) finds the targets, the chances and the
//! Action Points and asks these rules what to do with them.

/// The settings, named as the mod's file names them (without the type prefix).
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// `bEnabled`.
    pub enabled: bool,
    /// `iKeyLock`, `iKeyNext`, `iKeyPrevious`: DirectInput scan codes, 0 for none.
    pub key_lock: u8,
    pub key_next: u8,
    pub key_previous: u8,
    /// `fMaxRange` (game units) and `fMaxAngle` (degrees off the view) of a new lock.
    pub max_range: f32,
    pub max_angle: f32,
    /// `fMinChanceToLock`, 0 to 1: the lowest hit chance a target can be locked, cycled
    /// to or chained to at.
    pub min_chance_to_lock: f32,
    /// `iLimb`: the limb aimed at when a lock starts.
    pub limb: Limb,
    /// `bFreeGameKeys`: while locked, the game's own bindings of the keys are held back.
    pub free_game_keys: bool,
    /// `bLimbHighlight`.
    pub limb_highlight: bool,
    /// `fPanelGap`: interface units between the limb's mark and the panel.
    pub panel_gap: f32,
    /// `iKeyLimbNext`, `iKeyLimbPrevious`.
    pub key_limb_next: u8,
    pub key_limb_previous: u8,
    /// `bLineOfSight`, `fLosLostSeconds`, `fLosInterval`.
    pub line_of_sight: bool,
    pub los_lost_seconds: f32,
    pub los_interval: f32,
    /// `bChain`, `fChainMaxAngle`, `fChainMaxRange`.
    pub chain: bool,
    pub chain_max_angle: f32,
    pub chain_max_range: f32,
    /// `bApDrain`, `fLockDrainPerSecond`, `fShotCostMult`, `fMinApToLock`.
    pub ap_drain: bool,
    pub lock_drain_per_second: f32,
    pub shot_cost_mult: f32,
    pub min_ap_to_lock: f32,
    /// `bNoRegenWhileLocked`.
    pub no_regen_while_locked: bool,
    /// `fMaxChance`, 0 to 1.
    pub max_chance: f32,
    /// `iKeyCrit`, `fCritChargePerHit`, `fCritDamageMult`.
    pub key_crit: u8,
    pub crit_charge_per_hit: f32,
    pub crit_damage_mult: f32,
    /// `bNoNaturalCrits`.
    pub no_natural_crits: bool,
}

impl Default for Config {
    /// The mod's own defaults [documented: `RealtimeVatsNV.toml` 1.0.0].
    fn default() -> Self {
        Config {
            enabled: true,
            key_lock: 0x2D,
            key_next: 0x2E,
            key_previous: 0,
            max_range: 2500.0,
            max_angle: 45.0,
            min_chance_to_lock: 0.10,
            limb: Limb::Torso,
            free_game_keys: true,
            limb_highlight: true,
            panel_gap: 60.0,
            key_limb_next: 0x2C,
            key_limb_previous: 0,
            line_of_sight: true,
            los_lost_seconds: 1.5,
            los_interval: 0.2,
            chain: true,
            chain_max_angle: 90.0,
            chain_max_range: 1500.0,
            ap_drain: true,
            lock_drain_per_second: 1.5,
            shot_cost_mult: 0.4,
            min_ap_to_lock: 5.0,
            no_regen_while_locked: true,
            max_chance: 0.95,
            key_crit: 0x21,
            crit_charge_per_hit: 0.2,
            crit_damage_mult: 3.0,
            no_natural_crits: true,
        }
    }
}

impl Config {
    /// The settings with every value `get` has under the mod's names (`fMaxRange`,
    /// `bChain`, `iKeyLock` …) put over the defaults; the names whose text is not a value
    /// of their kind are returned and keep their default. Numbers may be decimal or
    /// `0x` hexadecimal, switches `true`/`false`/`1`/`0`.
    pub fn read(get: impl Fn(&str) -> Option<String>) -> (Config, Vec<String>) {
        let mut c = Config::default();
        let mut bad = Vec::new();
        let mut float = |name: &str, to: &mut f32, least: f32| {
            if let Some(t) = get(name) {
                match t.trim().parse::<f32>() {
                    Ok(v) if v.is_finite() && v >= least => *to = v,
                    _ => bad.push(name.to_string()),
                }
            }
        };
        float("fMaxRange", &mut c.max_range, 0.0);
        float("fMaxAngle", &mut c.max_angle, 0.0);
        float("fMinChanceToLock", &mut c.min_chance_to_lock, 0.0);
        float("fPanelGap", &mut c.panel_gap, 0.0);
        float("fLosLostSeconds", &mut c.los_lost_seconds, 0.0);
        float("fLosInterval", &mut c.los_interval, 0.0);
        float("fChainMaxAngle", &mut c.chain_max_angle, 0.0);
        float("fChainMaxRange", &mut c.chain_max_range, 0.0);
        float("fLockDrainPerSecond", &mut c.lock_drain_per_second, 0.0);
        float("fShotCostMult", &mut c.shot_cost_mult, 0.0);
        float("fMinApToLock", &mut c.min_ap_to_lock, 0.0);
        float("fMaxChance", &mut c.max_chance, 0.0);
        float("fCritChargePerHit", &mut c.crit_charge_per_hit, 0.0);
        float("fCritDamageMult", &mut c.crit_damage_mult, 0.0);
        let mut switch = |name: &str, to: &mut bool| {
            if let Some(t) = get(name) {
                match t.trim().to_ascii_lowercase().as_str() {
                    "true" | "1" => *to = true,
                    "false" | "0" => *to = false,
                    _ => bad.push(name.to_string()),
                }
            }
        };
        switch("bEnabled", &mut c.enabled);
        switch("bFreeGameKeys", &mut c.free_game_keys);
        switch("bLimbHighlight", &mut c.limb_highlight);
        switch("bLineOfSight", &mut c.line_of_sight);
        switch("bChain", &mut c.chain);
        switch("bApDrain", &mut c.ap_drain);
        switch("bNoRegenWhileLocked", &mut c.no_regen_while_locked);
        switch("bNoNaturalCrits", &mut c.no_natural_crits);
        let whole = |t: &str| {
            let t = t.trim();
            match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                Some(h) => u32::from_str_radix(h, 16).ok(),
                None => t.parse::<u32>().ok(),
            }
        };
        let mut key = |name: &str, to: &mut u8| {
            if let Some(t) = get(name) {
                match whole(&t).and_then(|v| u8::try_from(v).ok()) {
                    Some(v) => *to = v,
                    None => bad.push(name.to_string()),
                }
            }
        };
        key("iKeyLock", &mut c.key_lock);
        key("iKeyNext", &mut c.key_next);
        key("iKeyPrevious", &mut c.key_previous);
        key("iKeyLimbNext", &mut c.key_limb_next);
        key("iKeyLimbPrevious", &mut c.key_limb_previous);
        key("iKeyCrit", &mut c.key_crit);
        if let Some(t) = get("iLimb") {
            match whole(&t).and_then(Limb::from_code) {
                Some(l) => c.limb = l,
                None => bad.push("iLimb".to_string()),
            }
        }
        (c, bad)
    }

    /// The mod's names and this configuration's values, for writing the settings out.
    pub fn entries(&self) -> Vec<(&'static str, String)> {
        let b = |v: bool| v.to_string();
        let k = |v: u8| format!("0x{v:02X}");
        vec![
            ("bEnabled", b(self.enabled)),
            ("iKeyLock", k(self.key_lock)),
            ("iKeyNext", k(self.key_next)),
            ("iKeyPrevious", k(self.key_previous)),
            ("fMaxRange", self.max_range.to_string()),
            ("fMaxAngle", self.max_angle.to_string()),
            ("fMinChanceToLock", self.min_chance_to_lock.to_string()),
            ("iLimb", self.limb.code().to_string()),
            ("bFreeGameKeys", b(self.free_game_keys)),
            ("bLimbHighlight", b(self.limb_highlight)),
            ("fPanelGap", self.panel_gap.to_string()),
            ("iKeyLimbNext", k(self.key_limb_next)),
            ("iKeyLimbPrevious", k(self.key_limb_previous)),
            ("bLineOfSight", b(self.line_of_sight)),
            ("fLosLostSeconds", self.los_lost_seconds.to_string()),
            ("fLosInterval", self.los_interval.to_string()),
            ("bChain", b(self.chain)),
            ("fChainMaxAngle", self.chain_max_angle.to_string()),
            ("fChainMaxRange", self.chain_max_range.to_string()),
            ("bApDrain", b(self.ap_drain)),
            ("fLockDrainPerSecond", self.lock_drain_per_second.to_string()),
            ("fShotCostMult", self.shot_cost_mult.to_string()),
            ("fMinApToLock", self.min_ap_to_lock.to_string()),
            ("bNoRegenWhileLocked", b(self.no_regen_while_locked)),
            ("fMaxChance", self.max_chance.to_string()),
            ("iKeyCrit", k(self.key_crit)),
            ("fCritChargePerHit", self.crit_charge_per_hit.to_string()),
            ("fCritDamageMult", self.crit_damage_mult.to_string()),
            ("bNoNaturalCrits", b(self.no_natural_crits)),
        ]
    }

    /// The chance a lock-on shot is rolled on, percent: the VATS chance of the limb, no
    /// higher than `fMaxChance`.
    pub fn shot_chance(&self, vats_percent: f32) -> f32 {
        vats_percent.clamp(0.0, (self.max_chance * 100.0).max(0.0))
    }

    /// Whether a target with this chance (percent, [`Config::shot_chance`]) can be locked,
    /// cycled to or chained to.
    pub fn lockable(&self, chance_percent: f32) -> bool {
        chance_percent >= self.min_chance_to_lock * 100.0
    }

    /// The Action Points one shot at the locked target costs: the weapon's VATS attack
    /// cost times `fShotCostMult`; nothing with `bApDrain` off.
    pub fn shot_cost(&self, weapon_ap: f32) -> f32 {
        if self.ap_drain {
            (weapon_ap * self.shot_cost_mult).max(0.0)
        } else {
            0.0
        }
    }

    /// The Action Points holding the lock takes over `dt` seconds.
    pub fn hold_cost(&self, dt: f32) -> f32 {
        if self.ap_drain {
            (self.lock_drain_per_second * dt).max(0.0)
        } else {
            0.0
        }
    }

    /// Whether a lock may start with `ap` Action Points: at least `fMinApToLock`
    /// (always with `bApDrain` off).
    pub fn can_start(&self, ap: f32) -> bool {
        !self.ap_drain || ap >= self.min_ap_to_lock
    }

    /// Whether a held lock drops for want of Action Points: at zero.
    pub fn out_of_ap(&self, ap: f32) -> bool {
        self.ap_drain && ap <= 0.0
    }
}

/// The limb a lock aims at, in the order the limb key goes through them [documented: the
/// settings file's comment on `iKeyLimbNext`: "torso, head, arms, legs"].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Limb {
    #[default]
    Torso,
    Head,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
}

impl Limb {
    pub const ALL: [Limb; 6] = [Limb::Torso, Limb::Head, Limb::LeftArm, Limb::RightArm, Limb::LeftLeg, Limb::RightLeg];

    /// The settings file's number of the limb [documented: its comment on `iLimb`].
    pub fn code(self) -> u32 {
        match self {
            Limb::Torso => 0,
            Limb::Head => 1,
            Limb::LeftArm => 3,
            Limb::RightArm => 5,
            Limb::LeftLeg => 7,
            Limb::RightLeg => 10,
        }
    }

    pub fn from_code(code: u32) -> Option<Limb> {
        Limb::ALL.into_iter().find(|l| l.code() == code)
    }

    /// The limb-condition actor value the limb lowers (Perception Condition 25 … Right
    /// Mobility Condition 30) [documented: GECK wiki Actor Value list].
    pub fn condition(self) -> u16 {
        match self {
            Limb::Head => 25,
            Limb::Torso => 26,
            Limb::LeftArm => 27,
            Limb::RightArm => 28,
            Limb::LeftLeg => 29,
            Limb::RightLeg => 30,
        }
    }

    /// The panel's name for the limb when the target's body part data has none.
    pub fn name(self) -> &'static str {
        match self {
            Limb::Torso => "Torso",
            Limb::Head => "Head",
            Limb::LeftArm => "Left Arm",
            Limb::RightArm => "Right Arm",
            Limb::LeftLeg => "Left Leg",
            Limb::RightLeg => "Right Leg",
        }
    }

    /// The limb `steps` on from this one among `has` (the limbs the target has), going
    /// round; this limb when the target has no other.
    pub fn cycled(self, steps: i32, has: impl Fn(Limb) -> bool) -> Limb {
        let at = Limb::ALL.iter().position(|l| *l == self).unwrap_or(0) as i32;
        let n = Limb::ALL.len() as i32;
        let step = if steps < 0 { -1 } else { 1 };
        (1..=n).map(|k| Limb::ALL[(at + step * k).rem_euclid(n) as usize]).find(|l| has(*l)).unwrap_or(self)
    }
}

/// A hostile the player could lock on to, as seen this moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seen {
    pub id: u32,
    /// From the player's eye, game units.
    pub distance: f32,
    /// Off the view direction, degrees.
    pub angle: f32,
    /// The chance to hit it on the aimed limb, percent ([`Config::shot_chance`]).
    pub chance: f32,
}

/// The target a new lock takes among the visible hostiles: within `fMaxRange` and
/// `fMaxAngle` with a chance of at least `fMinChanceToLock`, the one nearest the view
/// direction [hypothesis: the settings give the limits, not the choice among several].
pub fn pick_lock(seen: &[Seen], c: &Config) -> Option<Seen> {
    in_reach(seen, c).min_by(|a, b| a.angle.total_cmp(&b.angle).then(a.distance.total_cmp(&b.distance)))
}

fn in_reach<'a>(seen: &'a [Seen], c: &'a Config) -> impl Iterator<Item = Seen> + 'a {
    seen.iter().copied().filter(|s| s.distance <= c.max_range && s.angle <= c.max_angle && c.lockable(s.chance))
}

/// The target the cycle key moves to from `current`: the lockable targets ordered by
/// their angle off the view, the next one after (or before, `forward` false) the current
/// one, going round [hypothesis: the order]. `None` when there is no other.
pub fn pick_cycle(seen: &[Seen], current: u32, forward: bool, c: &Config) -> Option<Seen> {
    let mut all: Vec<Seen> = in_reach(seen, c).filter(|s| s.id != current).collect();
    all.sort_by(|a, b| a.angle.total_cmp(&b.angle).then(a.distance.total_cmp(&b.distance)).then(a.id.cmp(&b.id)));
    let angle = seen.iter().find(|s| s.id == current).map(|s| s.angle);
    match (angle, forward) {
        // The current one is still seen: the next farther off the view, else round to the first.
        (Some(a), true) => all.iter().find(|s| s.angle >= a).or(all.first()).copied(),
        (Some(a), false) => all.iter().rev().find(|s| s.angle <= a).or(all.last()).copied(),
        (None, true) => all.first().copied(),
        (None, false) => all.last().copied(),
    }
}

/// The target a lock chains to when its target dies: with `bChain` on, the nearest
/// visible hostile within `fChainMaxRange` and `fChainMaxAngle` of the view with a
/// lockable chance [documented: "the next closest hostile in view"].
pub fn pick_chain(seen: &[Seen], dead: u32, c: &Config) -> Option<Seen> {
    if !c.chain {
        return None;
    }
    seen.iter()
        .copied()
        .filter(|s| s.id != dead && s.distance <= c.chain_max_range && s.angle <= c.chain_max_angle && c.lockable(s.chance))
        .min_by(|a, b| a.distance.total_cmp(&b.distance).then(a.angle.total_cmp(&b.angle)))
}

/// The critical meter: hits on the locked target charge it, and once full the critical
/// key arms the next hit.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CritMeter {
    /// 0 to 1 (full).
    pub charge: f32,
    pub armed: bool,
}

impl CritMeter {
    pub fn full(&self) -> bool {
        self.charge >= 1.0 - 1e-4
    }

    /// The critical key: arms the next hit when the meter is full. Whether it armed.
    pub fn arm(&mut self) -> bool {
        if self.full() && !self.armed {
            self.armed = true;
            return true;
        }
        false
    }

    /// A hit on the locked target: an armed one is the critical (the damage multiplier
    /// returned; the meter empties), any other charges the meter by `fCritChargePerHit`
    /// [hypothesis: the critical hit itself charges nothing].
    pub fn hit(&mut self, c: &Config) -> Option<f32> {
        if self.armed {
            *self = CritMeter::default();
            return Some(c.crit_damage_mult);
        }
        self.charge = (self.charge + c.crit_charge_per_hit).clamp(0.0, 1.0);
        None
    }
}

/// How long a locked target has been out of sight: the lock is let go after
/// `fLosLostSeconds`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Sight {
    pub unseen: f32,
}

impl Sight {
    /// A sight check `dt` seconds after the last: whether the lock is lost.
    pub fn check(&mut self, seen: bool, dt: f32, c: &Config) -> bool {
        if seen || !c.line_of_sight {
            self.unseen = 0.0;
            return false;
        }
        self.unseen += dt.max(0.0);
        self.unseen >= c.los_lost_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(id: u32, distance: f32, angle: f32, chance: f32) -> Seen {
        Seen { id, distance, angle, chance }
    }

    #[test]
    fn the_defaults_are_the_mods() {
        let c = Config::default();
        assert_eq!((c.key_lock, c.key_next, c.key_limb_next, c.key_crit), (0x2D, 0x2E, 0x2C, 0x21));
        assert_eq!((c.max_range, c.max_angle, c.min_chance_to_lock), (2500.0, 45.0, 0.10));
        assert_eq!((c.los_lost_seconds, c.los_interval), (1.5, 0.2));
        assert_eq!((c.chain_max_angle, c.chain_max_range), (90.0, 1500.0));
        assert_eq!((c.lock_drain_per_second, c.shot_cost_mult, c.min_ap_to_lock), (1.5, 0.4, 5.0));
        assert_eq!((c.max_chance, c.crit_charge_per_hit, c.crit_damage_mult), (0.95, 0.2, 3.0));
        assert!(c.enabled && c.chain && c.ap_drain && c.no_regen_while_locked && c.no_natural_crits && c.line_of_sight && c.free_game_keys);
    }

    #[test]
    fn a_shots_chance_is_the_vats_chance_capped_at_95() {
        let c = Config::default();
        assert_eq!(c.shot_chance(40.0), 40.0);
        assert_eq!(c.shot_chance(95.0), 95.0);
        assert_eq!(c.shot_chance(100.0), 95.0);
        assert_eq!(c.shot_chance(-3.0), 0.0);
        let low = Config { max_chance: 0.5, ..Config::default() };
        assert_eq!(low.shot_chance(80.0), 50.0);
    }

    #[test]
    fn a_target_under_ten_percent_cannot_be_locked() {
        let c = Config::default();
        assert!(c.lockable(10.0));
        assert!(!c.lockable(9.9));
    }

    #[test]
    fn a_shot_costs_four_tenths_of_the_weapons_ap_and_the_lock_drains() {
        let c = Config::default();
        assert!((c.shot_cost(20.0) - 8.0).abs() < 1e-5);
        assert!((c.shot_cost(55.0) - 22.0).abs() < 1e-5);
        assert!((c.hold_cost(2.0) - 3.0).abs() < 1e-5);
        assert!(c.can_start(5.0) && !c.can_start(4.9));
        assert!(c.out_of_ap(0.0) && !c.out_of_ap(0.1));
        let free = Config { ap_drain: false, ..Config::default() };
        assert_eq!((free.shot_cost(20.0), free.hold_cost(2.0)), (0.0, 0.0));
        assert!(free.can_start(0.0) && !free.out_of_ap(0.0));
    }

    #[test]
    fn a_lock_takes_the_target_nearest_the_view_within_range_and_angle() {
        let c = Config::default();
        let all = [seen(1, 400.0, 30.0, 80.0), seen(2, 2000.0, 5.0, 40.0), seen(3, 2600.0, 1.0, 90.0), seen(4, 300.0, 50.0, 90.0), seen(5, 500.0, 2.0, 5.0)];
        // 3 is out of range, 4 out of the angle, 5 under 10 %.
        assert_eq!(pick_lock(&all, &c).map(|s| s.id), Some(2));
        assert_eq!(pick_lock(&all[2..], &c), None);
    }

    #[test]
    fn a_kill_chains_to_the_nearest_within_1500_units_and_90_degrees() {
        let c = Config::default();
        let all = [seen(1, 100.0, 0.0, 90.0), seen(2, 1400.0, 80.0, 30.0), seen(3, 900.0, 95.0, 90.0), seen(4, 1600.0, 2.0, 90.0), seen(5, 1200.0, 10.0, 60.0), seen(6, 200.0, 10.0, 4.0)];
        // 1 is the dead one, 3 is behind 90 degrees, 4 beyond 1500 units, 6 under 10 %: 5
        // is nearer than 2.
        assert_eq!(pick_chain(&all, 1, &c).map(|s| s.id), Some(5));
        assert_eq!(pick_chain(&all[..1], 1, &c), None);
        assert_eq!(pick_chain(&all, 1, &Config { chain: false, ..c }), None);
    }

    #[test]
    fn the_cycle_key_goes_round_the_targets_by_angle() {
        let c = Config::default();
        let all = [seen(1, 400.0, 2.0, 80.0), seen(2, 500.0, 10.0, 80.0), seen(3, 600.0, 30.0, 80.0), seen(4, 600.0, 60.0, 80.0)];
        assert_eq!(pick_cycle(&all, 1, true, &c).map(|s| s.id), Some(2));
        assert_eq!(pick_cycle(&all, 2, true, &c).map(|s| s.id), Some(3));
        assert_eq!(pick_cycle(&all, 3, true, &c).map(|s| s.id), Some(1), "4 is out of the angle: round to the first");
        assert_eq!(pick_cycle(&all, 1, false, &c).map(|s| s.id), Some(3));
        assert_eq!(pick_cycle(&all[..1], 1, true, &c), None);
    }

    #[test]
    fn five_hits_fill_the_meter_and_the_armed_hit_is_the_critical() {
        let c = Config::default();
        let mut m = CritMeter::default();
        assert!(!m.arm(), "an empty meter does not arm");
        for k in 1..=4 {
            assert_eq!(m.hit(&c), None);
            assert!(!m.full(), "hit {k}");
        }
        assert_eq!(m.hit(&c), None);
        assert!(m.full());
        assert_eq!(m.hit(&c), None, "a full meter stays full until armed");
        assert!(m.full());
        assert!(m.arm());
        assert!(!m.arm(), "already armed");
        assert_eq!(m.hit(&c), Some(3.0));
        assert_eq!(m, CritMeter::default());
    }

    #[test]
    fn a_lock_is_lost_after_a_second_and_a_half_out_of_sight() {
        let c = Config::default();
        let mut s = Sight::default();
        for _ in 0..7 {
            assert!(!s.check(false, 0.2, &c));
        }
        assert!(s.check(false, 0.2, &c), "1.6 s unseen");
        let mut s = Sight::default();
        assert!(!s.check(false, 1.0, &c));
        assert!(!s.check(true, 0.2, &c), "seen again: the count starts over");
        assert!(!s.check(false, 1.0, &c));
        assert!(!Sight::default().check(false, 10.0, &Config { line_of_sight: false, ..c }));
    }

    #[test]
    fn the_limb_key_goes_torso_head_arms_legs_over_the_limbs_the_target_has() {
        let all = |_| true;
        assert_eq!(Limb::Torso.cycled(1, all), Limb::Head);
        assert_eq!(Limb::Head.cycled(1, all), Limb::LeftArm);
        assert_eq!(Limb::RightLeg.cycled(1, all), Limb::Torso);
        assert_eq!(Limb::Torso.cycled(-1, all), Limb::RightLeg);
        let no_arms = |l| !matches!(l, Limb::LeftArm | Limb::RightArm);
        assert_eq!(Limb::Head.cycled(1, no_arms), Limb::LeftLeg);
        assert_eq!(Limb::Torso.cycled(1, |l| l == Limb::Torso), Limb::Torso);
        assert_eq!(Limb::from_code(10), Some(Limb::RightLeg));
        assert_eq!(Limb::from_code(2), None);
    }

    #[test]
    fn settings_are_read_by_the_mods_names() {
        let text = [("fMaxRange", "1800"), ("bChain", "false"), ("iKeyLock", "0x2F"), ("iLimb", "1"), ("fMaxChance", "nope"), ("iKeyCrit", "999")];
        let (c, bad) = Config::read(|n| text.iter().find(|(k, _)| *k == n).map(|(_, v)| v.to_string()));
        assert_eq!((c.max_range, c.chain, c.key_lock, c.limb), (1800.0, false, 0x2F, Limb::Head));
        assert_eq!((c.max_chance, c.key_crit), (0.95, 0x21), "bad values keep the default");
        assert_eq!(bad, vec!["fMaxChance".to_string(), "iKeyCrit".to_string()]);
        // What is written out reads back the same.
        let out = c.entries();
        let (again, bad) = Config::read(|n| out.iter().find(|(k, _)| *k == n).map(|(_, v)| v.clone()));
        assert_eq!(again, c);
        assert!(bad.is_empty());
    }
}
