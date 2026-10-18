//! What the STATS page shows, as plain data ([`Snapshot`]), and the rules that turn the
//! game's numbers into the page's words (docs/spec-notes/pipboy.md §5.1). Nothing here
//! reads the game; `read.rs` fills a snapshot.

use fnv_sim::FormId;

/// The actor values of the six body parts the Vault Boy shows, in the order head, torso,
/// left arm, right arm, left leg, right leg: Perception, Endurance, Left Attack, Right
/// Attack, Left Mobility and Right Mobility Condition [documented: xEdit `ActorValue`
/// 25–30; the AVIF records name them Head, Chest, Left Arm, Right Arm, Left Leg,
/// RightLeg (observed)].
pub const LIMBS: [u16; 6] = [25, 26, 27, 28, 29, 30];

/// The AVIF record of an actor value code in `FalloutNV.esm` [observed: the masters'
/// AVIF FormIDs follow the codes in five runs: 0–4 from 0x514, 5–11 from 0x3E8, 12–31
/// from 0x44C, 32–45 from 0x4B0, 46–76 from 0x5DC (`AVStrength` 0x3E8, `AVBarter`
/// 0x4B0, `AVDamageThreshold` 0x5FA)].
pub fn avif_form(code: u16) -> Option<u32> {
    let c = code as u32;
    Some(match code {
        0..=4 => 0x514 + c,
        5..=11 => 0x3E8 + c - 5,
        12..=31 => 0x44C + c - 12,
        32..=45 => 0x4B0 + c - 32,
        46..=76 => 0x5DC + c - 46,
        _ => return None,
    })
}

/// The experience a level needs: (n − 1) × ((n − 2) / 2 × `iXPBumpBase` + `iXPBase`)
/// [documented: GECK wiki Settings, "Experience required to level up"; defaults 200 and
/// 150].
pub fn xp_for_level(level: f64, base: f64, bump: f64) -> f64 {
    (level - 1.0) * ((level - 2.0) / 2.0 * bump + base)
}

/// "(+)" or "(-)" after a value raised or lowered from its permanent value (base and
/// permanent modifier) [observed: "34 (+)" for the owner's Melee Weapons with the Vault
/// Jumpsuit's +2 worn; hypothesis: a lowered value shows "(-)"].
pub fn modified_mark(shown: f64, permanent: f64) -> Option<&'static str> {
    if shown.round() > permanent.round() {
        Some("(+)")
    } else if shown.round() < permanent.round() {
        Some("(-)")
    } else {
        None
    }
}

/// A whole number as the page writes it.
pub fn whole(v: f64) -> String {
    format!("{}", v.round() as i64)
}

/// One change an effect makes: the value's short name, the amount, and whether the base
/// effect is detrimental (MGEF flag "Detrimental").
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub value: String,
    pub amount: f64,
    pub detrimental: bool,
}

/// The changes of one source, as the EFF list writes them: "Speech +2, Melee Weap. +2"
/// [observed: the owner's EFF screenshot of the Vault Jumpsuit, whose enchantment adds 2
/// to Speech and 2 to Melee Weapons; the short names are the AVIF records' ANAM;
/// hypothesis: a detrimental effect is written with a minus].
pub fn changes_text(changes: &[Change]) -> String {
    changes
        .iter()
        .map(|c| {
            let sign = if c.detrimental { '-' } else { '+' };
            format!("{} {sign}{}", c.value, whole(c.amount.abs()))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The amount a list of changes adds to one value, by the value's code.
pub fn added(changes: &[(u16, f64, bool)], code: u16) -> f64 {
    changes.iter().filter(|c| c.0 == code).map(|c| if c.2 { -c.1 } else { c.1 }).sum()
}

/// A hint at the status page's top right: "(9) Stimpak", or just the name when the player
/// has none [observed: "(9) Stimpak", "(3) Doctor's Bag", and "RadAway", "Rad-X" without
/// a count on the owner's screenshots; hypothesis: the count is left out at 0].
pub fn hint_text(name: &str, count: i32) -> String {
    if count > 0 {
        format!("({count}) {name}")
    } else {
        name.to_string()
    }
}

/// The karma's alignment and its picture from the four `Karma*` globals (very evil, evil,
/// good, very good) [documented: the globals `KarmaVeryEvil`, `KarmaEvil`, `KarmaGood`,
/// `KarmaVeryGood` in FalloutNV.esm; hypothesis: a karma at or beyond a threshold has
/// that alignment, the words are the globals' and the pictures are
/// `karma icons\karma_evil|bad|neutral|good|saintly.dds` in that order].
pub fn karma_alignment(karma: f64, very_evil: f64, evil: f64, good: f64, very_good: f64) -> (&'static str, &'static str) {
    if karma >= very_good {
        ("Very Good", "karma_saintly")
    } else if karma >= good {
        ("Good", "karma_good")
    } else if karma <= very_evil {
        ("Very Evil", "karma_evil")
    } else if karma <= evil {
        ("Evil", "karma_bad")
    } else {
        ("Neutral", "karma_neutral")
    }
}

/// The karma titles by level (1 to 30, 30 standing for 30 and over) and karma (good,
/// neutral, evil), as the owner's table gives them (`todo 5.txt`, 2026-10-16) [observed:
/// the owner's list]. Only a fallback: FalloutNV.esm holds most of them as the game
/// settings `sKarmicTitle<Good|Neutral|Evil><level>` [observed: 50 of the 90; the others
/// are presumably the executable's defaults], which win where present.
pub const KARMA_TITLES: [[&str; 3]; 30] = [
    ["Samaritan", "Drifter", "Grifter"],
    ["Martyr", "Renegade", "Outlaw"],
    ["Sentinel", "Seeker", "Opportunist"],
    ["Defender", "Wanderer", "Plunderer"],
    ["Dignitary", "Citizen", "Fat Cat"],
    ["Peacekeeper", "Adventurer", "Marauder"],
    ["Ranger of the Wastes", "Vagabond of the Wastes", "Pirate of the Wastes"],
    ["Protector", "Mercenary", "Betrayer"],
    ["Desert Avenger", "Desert Scavenger", "Desert Terror"],
    ["Exemplar", "Observer", "Ne'er-do-well"],
    ["Vegas Crusader", "Vegas Councilor", "Vegas Crime lord"],
    ["Paladin", "Keeper", "Defiler"],
    ["Mojave Legend", "Mojave Myth", "Mojave Boogeyman"],
    ["Shield of Hope", "Pinnacle of Survival", "Sword of Despair"],
    ["Vegas Legend", "Vegas Myth", "Vegas Boogeyman"],
    ["Hero of the Wastes", "Strider of the Wastes", "Villain of the Wastes"],
    ["Paragon", "Beholder", "Fiend"],
    ["Wasteland Savior", "Wasteland Watcher", "Wasteland Destroyer"],
    ["Saint", "Super-Human", "Evil Incarnate"],
    ["Guardian of the Wastes", "Renegade of the Wastes", "Scourge of the Wastes"],
    ["Restorer of Faith", "Soldier of Fortune", "Architect of Doom"],
    ["Model of Selflessness", "Profiteer", "Bringer of Sorrow"],
    ["Shepherd", "Egocentric", "Deceiver"],
    ["Friend of the People", "Loner", "Consort of Discord"],
    ["Champion of Justice", "Hero for Hire", "Stuff of Nightmares"],
    ["Symbol of Order", "Model of Apathy", "Agent of Chaos"],
    ["Herald of Tranquility", "Person of Refinement", "Instrument of Ruin"],
    ["Last, Best Hope of Humanity", "Moneygrubber", "Soultaker"],
    ["Savior of the Damned", "Gray Stranger", "Demon's Spawn"],
    ["Messiah", "True Mortal", "Devil"],
];

/// The karma title's column for an alignment ([`karma_alignment`]'s word): good for Good
/// and Very Good, evil for Evil and Very Evil [hypothesis: three columns over five
/// alignments].
pub fn karma_column(alignment: &str) -> usize {
    match alignment {
        "Good" | "Very Good" => 0,
        "Evil" | "Very Evil" => 2,
        _ => 1,
    }
}

/// The karma title at a level for an alignment: the game setting
/// `sKarmicTitle<Good|Neutral|Evil><level>` where the plugins hold it (`setting`), else
/// [`KARMA_TITLES`]. Levels above 30 take 30's title, below 1 level 1's.
pub fn karma_title(level: i64, alignment: &str, setting: impl Fn(&str) -> Option<String>) -> String {
    let level = level.clamp(1, 30) as usize;
    let column = karma_column(alignment);
    let name = format!("sKarmicTitle{}{level}", ["Good", "Neutral", "Evil"][column]);
    setting(&name).filter(|s| !s.is_empty()).unwrap_or_else(|| KARMA_TITLES[level - 1][column].to_string())
}

/// The title of a reputation from the levels (0–3) of fame and infamy
/// (`crate::reputation::TITLES`).
pub fn reputation_title(fame: usize, infamy: usize) -> &'static str {
    crate::reputation::title(fame, infamy)
}

/// The New Vegas statistics the player's General list names, in the order of their codes
/// [documented: GECK wiki GetPCMiscStat names them; observed: the original's list starts
/// Quests Completed, Locations Discovered, People Killed, Creatures Killed, Locks Picked,
/// Computers Hacked, Stimpaks Taken, Rad-X Taken, RadAway Taken, Chems Taken, owner's
/// screenshot in the private references, "pipboy stat tracking", 2026-10-14] (`fnv_game::misc_stats`).
pub const MISC_STATS: [&str; 43] = fnv_game::misc_stats::NAMES;

/// One row of a list (S.P.E.C.I.A.L., Skills, Perks, General): its name, the number at
/// the right, the mark after it, and what the display beside the list shows of it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Row {
    pub name: String,
    pub value: Option<String>,
    pub mark: Option<&'static str>,
    pub description: String,
    /// The picture, a path under `textures\`.
    pub icon: Option<String>,
}

/// One line of the EFF list: the source's name and its changes.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectLine {
    pub name: String,
    pub text: String,
}

/// A reputation the player has a standing with.
#[derive(Clone, Debug, PartialEq)]
pub struct Standing {
    pub form: FormId,
    pub name: String,
    pub title: &'static str,
    pub fame: f64,
    pub infamy: f64,
    pub icon: Option<String>,
}

/// The status page's hints at the top right.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Hints {
    /// Stimpak, Doctor's Bag, RadAway, Rad-X: the text without the key.
    pub texts: [String; 4],
}

/// Everything the page draws, compared frame to frame to know when to draw it again.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Snapshot {
    /// Whether the scripts' game is running (actor values, experience and perks come from
    /// it; without it the figures are "--").
    pub game: bool,
    pub level: Option<i64>,
    /// Current and maximum.
    pub hp: Option<(f64, f64)>,
    pub ap: Option<(f64, f64)>,
    /// Experience and what the next level needs.
    pub xp: Option<(f64, f64)>,
    pub name: String,
    /// Each body part's condition, 0 to 1 ([`LIMBS`]).
    pub limbs: [f64; 6],
    pub hints: Hints,
    pub rad_resist: Option<f64>,
    pub rads: f64,
    pub effects: Vec<EffectLine>,
    pub special: Vec<Row>,
    pub skills: Vec<Row>,
    pub perks: Vec<Row>,
    pub general: Vec<Row>,
    pub standings: Vec<Standing>,
    pub karma: Option<(f64, &'static str, &'static str)>,
    /// The karma title for the player's level and alignment ([`karma_title`]).
    pub karma_title: Option<String>,
}

impl Snapshot {
    /// The rows of a list tab (1 S.P.E.C.I.A.L., 2 Skills, 3 Perks, 4 General in the
    /// mode given).
    pub fn rows(&self, tab: usize, reputation: bool) -> Vec<Row> {
        match tab {
            1 => self.special.clone(),
            2 => self.skills.clone(),
            3 => self.perks.clone(),
            4 if reputation => self.standings.iter().map(|s| Row { name: s.name.clone(), icon: s.icon.clone(), ..Row::default() }).collect(),
            4 => self.general.clone(),
            _ => Vec::new(),
        }
    }
}

/// The first row shown so that `selected` is among `visible` rows from it, moved as
/// little as possible from `top`.
pub fn keep_in_view(top: usize, selected: usize, visible: usize, len: usize) -> usize {
    let visible = visible.max(1);
    let top = if selected < top {
        selected
    } else if selected >= top + visible {
        selected + 1 - visible
    } else {
        top
    };
    top.min(len.saturating_sub(visible))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_values_map_to_their_records() {
        assert_eq!(avif_form(5), Some(0x3E8));
        assert_eq!(avif_form(11), Some(0x3EE));
        assert_eq!(avif_form(16), Some(0x450));
        assert_eq!(avif_form(20), Some(0x454));
        assert_eq!(avif_form(25), Some(0x459));
        assert_eq!(avif_form(32), Some(0x4B0));
        assert_eq!(avif_form(41), Some(0x4B9));
        assert_eq!(avif_form(54), Some(0x5E4));
        assert_eq!(avif_form(76), Some(0x5FA));
        assert_eq!(avif_form(77), None);
    }

    #[test]
    fn levels_need_more_experience_each_time() {
        assert_eq!(xp_for_level(2.0, 200.0, 150.0), 200.0);
        assert_eq!(xp_for_level(3.0, 200.0, 150.0), 550.0);
        assert_eq!(xp_for_level(4.0, 200.0, 150.0), 1050.0);
    }

    #[test]
    fn effects_are_written_as_the_original_writes_them() {
        let c = [
            Change { value: "Speech".into(), amount: 2.0, detrimental: false },
            Change { value: "Melee Weap.".into(), amount: 2.0, detrimental: false },
        ];
        assert_eq!(changes_text(&c), "Speech +2, Melee Weap. +2");
        let d = [Change { value: "AGL".into(), amount: 1.0, detrimental: true }];
        assert_eq!(changes_text(&d), "AGL -1");
        assert_eq!(added(&[(38, 2.0, false), (43, 2.0, false), (38, 1.0, true)], 38), 1.0);
    }

    #[test]
    fn marks_and_hints() {
        assert_eq!(modified_mark(34.0, 32.0), Some("(+)"));
        assert_eq!(modified_mark(30.0, 32.0), Some("(-)"));
        assert_eq!(modified_mark(32.2, 32.0), None);
        assert_eq!(hint_text("Stimpak", 9), "(9) Stimpak");
        assert_eq!(hint_text("RadAway", 0), "RadAway");
    }

    #[test]
    fn karma_and_reputation_words() {
        assert_eq!(karma_alignment(0.0, -750.0, -250.0, 250.0, 750.0).0, "Neutral");
        assert_eq!(karma_alignment(250.0, -750.0, -250.0, 250.0, 750.0).0, "Good");
        assert_eq!(karma_alignment(-800.0, -750.0, -250.0, 250.0, 750.0), ("Very Evil", "karma_evil"));
        // The plugins' setting wins; the owner's table fills the rest; 30 and over share.
        let esm = |n: &str| (n == "sKarmicTitleEvil29").then(|| "Demon Spawn".to_string());
        assert_eq!(karma_title(29, "Very Evil", esm), "Demon Spawn");
        assert_eq!(karma_title(29, "Neutral", esm), "Gray Stranger");
        assert_eq!(karma_title(1, "Good", |_| None), "Samaritan");
        assert_eq!(karma_title(45, "Evil", |_| None), "Devil");
        assert_eq!(karma_title(7, "Neutral", |_| None), "Vagabond of the Wastes");
        assert_eq!(reputation_title(0, 0), "Neutral");
        assert_eq!(reputation_title(3, 0), "Idolized");
        assert_eq!(reputation_title(1, 1), "Mixed");
    }

    #[test]
    fn the_selection_stays_in_view() {
        assert_eq!(keep_in_view(0, 3, 9, 13), 0);
        assert_eq!(keep_in_view(0, 9, 9, 13), 1);
        assert_eq!(keep_in_view(4, 12, 9, 13), 4);
        assert_eq!(keep_in_view(4, 2, 9, 13), 2);
        assert_eq!(keep_in_view(6, 6, 9, 13), 4);
        assert_eq!(keep_in_view(0, 0, 9, 3), 0);
    }
}
