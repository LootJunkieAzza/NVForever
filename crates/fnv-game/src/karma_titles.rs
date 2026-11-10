//! The karma's alignment and title (docs/spec-notes/pipboy-stats.md): for the Pip-Boy's
//! stats page (`nvf-play` pipboy/stats) and GetPlayerKarmaTitle (`crate::nvse_jg9`).

use crate::game::Game;

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

impl Game<'_> {
    /// The player's karma alignment word ([`karma_alignment`]) and karma title
    /// ([`karma_title`]) now, from the player's Karma, level and the four `Karma*`
    /// globals; `None` without those globals.
    pub fn player_karma(&mut self) -> Option<(&'static str, String)> {
        let limits = ["KarmaVeryEvil", "KarmaEvil", "KarmaGood", "KarmaVeryGood"].map(|n| self.world.forms.global_by_edid(n).map(|v| f64::from(v.1)));
        let [Some(ve), Some(e), Some(go), Some(vg)] = limits else { return None };
        let karma = crate::functions::actor::actor_value(self, fnv_sim::FormId::PLAYER_REF, crate::actor_values::KARMA).ok()?;
        let (word, _) = karma_alignment(karma, ve, e, go, vg);
        let level = self.player_level();
        let title = karma_title(level as i64, word, |n| match self.world.setting_value(n)? {
            fnv_sim::settings::Setting::Str(s) => Some(s.clone()),
            _ => None,
        });
        Some((word, title))
    }
}
