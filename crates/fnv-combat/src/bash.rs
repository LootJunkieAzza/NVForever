//! Weapon bash (the fork): a gun drawn, the Bash key strikes with it as a club, from the
//! mod B42 Melee Bash [observed: its `B42BashS`, `B42BashAnimS` and `B42BashOnHitEvent`
//! scripts in `B42Bash.esp`, 0.6b]. Headless; nvf-play `bash` starts it and deals the blow
//! through the melee strike, nvforever `consume_anims` plays the mod's clip.
//!
//! - **Which guns** ([`clip_type`]): weapon types 3 to 9 (pistols, rifles, automatics,
//!   two-hand handles and launchers); an energy pistol uses the pistol clips (4 → 3) and an
//!   energy rifle the rifle ones (7 → 5).
//! - **The cost**: [`AP_COST`] Action Points. Without them, the first-person fail clip
//!   (`BashFail.kf`) plays and nothing is struck.
//! - **The blow** ([`BashBlow`]): struck as the melee weapon of its kind ([`base_weapon`]:
//!   the Police Baton for pistols, the Baseball Bat for rifles and automatics, the
//!   Sledgehammer for the heavier ones), its damage × (1 + the gun's weight / 100) × 0.7,
//!   + 10 % a rank of the mod's Basher perks.
//! - **Perks** (`B42Bash.esp`'s, by EditorID): Basher (four ranks: damage, a chance to
//!   cripple a limb of [`cripple_chance`], at rank 4 a double-damage Critical Bash on twice
//!   the crit chance), Piercing Bash (the target's DT counts half, [`pierced_dt`]).
//! - **Knockdown** ([`knockdown_chance`]): every blow on someone still standing rolls
//!   1 + Melee Weapons^0.695 + 15 with Super Slam − the target's Endurance, percent.
//! - **Timing**: the clip's `HitFrameDetect` sound key strikes; the next bash may start
//!   [`COOLDOWN`] seconds after the clip ends.
//!
//! Not done (the mod's): Brutal Beatdown's streak, Executor, Knock-Out Bash, Wild Swings,
//! Forced Entry's lock bashing, Judicious Whipper's wear, the hit-reaction idles of its own
//! (the game's hit reactions play), the F4-like aim at the nearest head.

use crate::data::WeaponStats;

/// The Bash key's default: X (DIK 45) [observed: `iBashKey`].
pub const KEY_DIK: u32 = 45;
/// Action Points a bash takes [observed: `fCostAP = 17 * fAPMult`].
pub const AP_COST: f32 = 17.0;
/// Seconds after a bash before the next [observed: `fCD = 0.65 - 0.2 * Rapid - fCutoffTimer`,
/// 0.45, so 0.2 (0 with Brutal Beatdown)].
pub const COOLDOWN: f32 = 0.2;
/// A bash whose clip never strikes (no clip, no key) strikes this long after it starts
/// [design].
pub const STRIKE_FALLBACK: f32 = 0.25;
/// And is over this long after [design: the clips run 0.6 to 0.9 s].
pub const LONGEST: f32 = 1.5;

/// The mod's perks, by EditorID.
pub const PERK_BASHER: [&str; 4] = ["B42BashPerkBasher", "B42BashPerkBasher2", "B42BashPerkBasher3", "B42BashPerkBasher4"];
pub const PERK_PIERCING: &str = "B42BashPerkDT";
/// The mod's quests, whose scripts the native bash replaces.
pub const MOD_QUESTS: [&str; 2] = ["B42BashQ", "B42BashMCMQ"];
/// Super Slam (FalloutNV.esm `SuperSlam`).
pub const SUPER_SLAM: &str = "SuperSlam";

/// The clip folder for a weapon type (`GetWeapType`), `None` for one that does not bash.
pub fn clip_type(weapon_type: i64) -> Option<i64> {
    match weapon_type {
        4 => Some(3),
        7 => Some(5),
        3..=9 => Some(weapon_type),
        _ => None,
    }
}

/// The melee weapon a bash strikes as (its EditorID), for a clip type ([`clip_type`]).
pub fn base_weapon(clip_type: i64) -> &'static str {
    match clip_type {
        3 => "WeapPoliceBaton",
        t if t > 6 => "WeapSledgehammer",
        _ => "WeapBaseballBat",
    }
}

/// The mod's clip folder: `meshes\characters\_male\idleanims\b42bash\<alt>\<1 first | 0
/// third>\<clip type>\`.
pub fn clip_folder(first_person: bool, clip_type: i64) -> String {
    format!("meshes\\characters\\_male\\idleanims\\b42bash\\0\\{}\\{clip_type}\\", u8::from(first_person))
}

/// The fail clip (no Action Points), first person only.
pub const FAIL_CLIP: &str = "meshes\\characters\\_male\\idleanims\\b42bash\\0\\bashfail.kf";

/// What a bash strikes with.
#[derive(Debug, Clone)]
pub struct BashBlow {
    /// The base melee weapon's stats, its damage the bash's.
    pub stats: WeaponStats,
    /// Basher ranks (0 to 4).
    pub basher: u8,
    pub piercing: bool,
}

/// The bash's damage before skill and perks: the base weapon's × (1 + the gun's weight /
/// 100) × 0.7 × (1 + 0.1 × Basher ranks).
pub fn damage(base_damage: f32, gun_weight: f32, basher: u8) -> f32 {
    base_damage * (1.0 + gun_weight / 100.0) * 0.7 * (1.0 + 0.1 * f32::from(basher))
}

impl BashBlow {
    /// The blow of a gun of `gun_weight`, struck as `base`.
    pub fn of(base: &WeaponStats, gun_weight: f32, basher: u8, piercing: bool) -> BashBlow {
        let mut stats = base.clone();
        stats.base_damage = damage(base.base_damage as f32, gun_weight, basher).round() as i32;
        BashBlow { stats, basher, piercing }
    }
}

/// The target's DT against a bash: half with Piercing Bash [observed: `fDT / iPerkDT`].
pub fn pierced_dt(dt: f32, piercing: bool) -> f32 {
    if piercing { dt / 2.0 } else { dt }
}

/// The chance (0..1) a bash cripples a limb: 5 % a Basher rank [observed: `iPerkBasher * 5`].
pub fn cripple_chance(basher: u8) -> f32 {
    f32::from(basher) * 0.05
}

/// The chance (0..1) a bash knocks its target down [observed: `fMeleeAVCombo + iForceKO -
/// Endurance >= rand 0 100`, `fMeleeAVCombo = 1 + Melee^0.695 + SuperSlam * 15`].
pub fn knockdown_chance(melee_skill: f32, super_slam: bool, endurance: f32) -> f32 {
    ((1.0 + melee_skill.max(0.0).powf(0.695) + if super_slam { 15.0 } else { 0.0 } - endurance) / 100.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guns_bash_with_their_kind_of_club() {
        assert_eq!(clip_type(0), None);
        assert_eq!(clip_type(2), None);
        assert_eq!(clip_type(4), Some(3));
        assert_eq!(clip_type(7), Some(5));
        assert_eq!(clip_type(9), Some(9));
        assert_eq!(clip_type(10), None);
        assert_eq!(base_weapon(3), "WeapPoliceBaton");
        assert_eq!(base_weapon(5), "WeapBaseballBat");
        assert_eq!(base_weapon(6), "WeapBaseballBat");
        assert_eq!(base_weapon(8), "WeapSledgehammer");
        assert_eq!(clip_folder(true, 5), "meshes\\characters\\_male\\idleanims\\b42bash\\0\\1\\5\\");
    }

    #[test]
    fn the_blow_follows_the_mods_numbers() {
        // A 10 lb rifle, struck as a bat of 18: 18 × 1.1 × 0.7 = 13.86; Basher 2 + 20 %.
        assert!((damage(18.0, 10.0, 0) - 13.86).abs() < 1e-3);
        assert!((damage(18.0, 10.0, 2) - 16.632).abs() < 1e-3);
        let base = WeaponStats { base_damage: 18, edid: Some("WeapBaseballBat".into()), ..Default::default() };
        let b = BashBlow::of(&base, 10.0, 0, true);
        assert_eq!(b.stats.base_damage, 14);
        assert_eq!(pierced_dt(10.0, b.piercing), 5.0);
        assert_eq!(pierced_dt(10.0, false), 10.0);
        assert!((cripple_chance(4) - 0.2).abs() < 1e-6);
        // Melee 50, no Super Slam, Endurance 5: 1 + 50^0.695 (15.18) - 5 = 11.2 %.
        let k = knockdown_chance(50.0, false, 5.0);
        assert!((k - 0.1118).abs() < 1e-3, "{k}");
        assert!(knockdown_chance(50.0, true, 5.0) > k + 0.149);
        assert_eq!(knockdown_chance(0.0, false, 10.0), 0.0);
    }
}
