//! The sounds animation clips name in their text keys, and the footstep and armour
//! sounds their `Enum:` keys stand for (docs/spec-notes/audio.md §10). Headless: the
//! engine says which keys a playing clip passed and what the actor stands on and wears;
//! this module says which SOUN to play.
//!
//! - `Sound: <SOUN editor ID> [node]` (any case) names the sound [observed: 3,530 keys
//!   in `Fallout - Meshes.bsa`; 663 of 842 distinct names are SOUN editor IDs in
//!   FalloutNV.esm, the rest add-on content's].
//! - `Enum: Left` / `Enum: Right` are footfalls [hypothesis, actors.md §8.3]; the sound
//!   is the footstep record `FST<material><gait><L|R>` of the ground's Havok material
//!   [hypothesis from the record names; the table [`footstep_family`] is ours].
//! - `Enum: ArmorWalk|Run|Sneak` and `Enum: FoleyWalk|Run|Sneak` are the worn armour's
//!   rustle: the ARMO's animation sounds (SNAM, its own or its template's, TNAM)
//!   [hypothesis: armour (`… (Armor)`) types answer `Armor…` keys, the others `Foley…`].

use fnv_esm::{LoadOrder, Schema};
use fnv_world::fields::{formid_of, global, winner, Fields};
use serde_json::Value;

/// How an actor moves when a footfall or rustle sounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gait {
    Walk,
    Run,
    Sneak,
}

impl Gait {
    pub fn name(self) -> &'static str {
        match self {
            Gait::Walk => "Walk",
            Gait::Run => "Run",
            Gait::Sneak => "Sneak",
        }
    }

    /// The gait a word names (`walk`, `Run`, …).
    fn of_word(w: &str) -> Option<Gait> {
        [Gait::Walk, Gait::Run, Gait::Sneak].into_iter().find(|g| w.eq_ignore_ascii_case(g.name()))
    }

    /// The gait a name contains (`FOLClothRun` → Run).
    fn in_name(n: &str) -> Option<Gait> {
        let l = n.to_ascii_lowercase();
        [Gait::Sneak, Gait::Run, Gait::Walk].into_iter().find(|g| l.contains(&g.name().to_ascii_lowercase()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

/// What a text key asks to be heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySound<'a> {
    /// A SOUN by editor ID.
    Sound(&'a str),
    /// A footfall.
    Foot(Side),
    /// The armour's rustle: armour (`Armor…`) or cloth (`Foley…`) sounds, at a gait.
    Foley { armor: bool, gait: Gait },
}

/// Read one text key; `None` for keys that ask for no sound.
pub fn parse(key: &str) -> Option<KeySound<'_>> {
    let key = key.trim();
    let (head, rest) = key.split_once(':')?;
    let rest = rest.trim();
    if head.trim().eq_ignore_ascii_case("sound") {
        // `Sound: DRSVaultGearOpenSteam01 SoundNodeSteam`: the name, then the node.
        let name = rest.split_whitespace().next()?;
        return Some(KeySound::Sound(name));
    }
    if !head.trim().eq_ignore_ascii_case("enum") {
        return None;
    }
    if rest.eq_ignore_ascii_case("left") {
        return Some(KeySound::Foot(Side::Left));
    }
    if rest.eq_ignore_ascii_case("right") {
        return Some(KeySound::Foot(Side::Right));
    }
    for (prefix, armor) in [("armor", true), ("foley", false)] {
        if rest.len() > prefix.len() && rest[..prefix.len()].eq_ignore_ascii_case(prefix) {
            return Gait::of_word(&rest[prefix.len()..]).map(|gait| KeySound::Foley { armor, gait });
        }
    }
    None
}

/// The text keys a clip passed going from clip time `from` to `to` (both within
/// `start..=stop`). A clip that went round (`to < from`) passed the keys after `from` and
/// those up to `to`; `from` = `None` starts at the clip's start (keys there included).
pub fn passed<'a>(keys: &'a [(f32, String)], start: f32, from: Option<f32>, to: f32) -> impl Iterator<Item = &'a str> + 'a {
    let from = from.unwrap_or(start - 1e-4);
    keys.iter()
        .filter(move |(t, _)| if to >= from { *t > from && *t <= to } else { *t > from || (*t >= start - 1e-4 && *t <= to) })
        .map(|(_, k)| k.as_str())
}

/// The gait of a movement clip: its `Enum: Armor…`/`Foley…` keys say it; else its name
/// (`FastForward` runs, `sneak…` sneaks) [hypothesis].
pub fn clip_gait(keys: &[(f32, String)], name: &str) -> Gait {
    if let Some(g) = keys.iter().find_map(|(_, k)| match parse(k) {
        Some(KeySound::Foley { gait, .. }) => Some(gait),
        _ => None,
    }) {
        return g;
    }
    let l = name.to_ascii_lowercase();
    if l.contains("sneak") {
        Gait::Sneak
    } else if l.contains("fast") || l.contains("run") {
        Gait::Run
    } else {
        Gait::Walk
    }
}

/// The footstep family (`FST<family>…`) of a Havok material code (nif.xml's
/// `Fallout3HavokMaterial`, the same numbering as LTEX HNAM): the code's material, and
/// whether it is stairs (codes 64–127) [documented: the numbering; hypothesis: which
/// family each material takes, from the names].
pub fn footstep_family(code: u32) -> (&'static str, bool) {
    let stairs = matches!(code / 32, 2 | 3);
    let family = match code % 32 {
        // Stone, glass, heavy stone, bottles.
        0 | 3 | 10 | 24 => "ConcSolid",
        19 => "ConcBroken",
        2 | 1 | 6 | 7 | 31 => "Dirt",
        4 => "Grass",
        8 => "Water",
        9 | 12 => "Wood",
        14 => "Snow",
        18 => "Sand",
        // Hollow metal, vehicle bodies and hollow parts, barrels, shopping carts.
        16 | 20 | 22 | 23 | 28 => "MetalHollow",
        // Sheet metal, cans, lunchboxes.
        17 | 25 | 29 => "MetalSheet",
        // Metal, heavy metal, chain, elevators, solid vehicle parts, guns, the rest.
        _ => "MetalSolid",
    };
    (family, stairs)
}

/// Footstep SOUN editor IDs to try, best first: the family's stairs sound on stairs, its
/// gait and side, its walk, its sound without a side (`FSTSnowWalk`), then dirt's
/// [design: the fallbacks].
pub fn footstep_candidates(family: &str, stairs: bool, gait: Gait, side: Side) -> Vec<String> {
    let s = if side == Side::Left { "L" } else { "R" };
    let g = gait.name();
    let mut v = Vec::new();
    if stairs {
        v.push(format!("FST{family}Stairs{s}"));
    }
    v.push(format!("FST{family}{g}{s}"));
    v.push(format!("FST{family}Walk{s}"));
    v.push(format!("FST{family}{g}"));
    v.push(format!("FST{family}Walk"));
    if family != "Dirt" {
        v.push(format!("FSTDirt{g}{s}"));
    }
    v
}

/// One of an armour's animation sounds (ARMO SNAM).
#[derive(Debug, Clone, PartialEq)]
pub struct FoleySound {
    pub sound: u32,
    pub chance: u8,
    /// An armour type (`… (Armor)`).
    pub armor: bool,
    /// The gait the sound's own editor ID names, else the type's [observed: in
    /// FalloutNV.esm the two disagree for walk and sneak (cloth) and walk and run
    /// (armour), audio.md §10; the name is taken].
    pub gait: Option<Gait>,
}

/// An armour's animation sounds: its own SNAM, else its template's (TNAM, followed up to
/// four times).
pub fn armour_foley(lo: &LoadOrder, schema: &Schema, armo: u32) -> Vec<FoleySound> {
    let mut id = armo;
    for _ in 0..4 {
        let Some(f) = winner(lo, id).and_then(|(pi, r)| Fields::new(r, pi, schema).ok()) else { return Vec::new() };
        if &f.sig != b"ARMO" {
            return Vec::new();
        }
        let own: Vec<FoleySound> = f
            .subs
            .iter()
            .filter(|s| &s.sig == b"SNAM")
            .filter_map(|s| f.decode(s))
            .filter_map(|v| {
                let sound = v.get("sound").and_then(formid_of).and_then(|l| global(lo, f.plugin, l))?;
                let label = v.get("type").and_then(Value::as_str).unwrap_or_default().to_string();
                let edid = winner(lo, sound).and_then(|(pi, r)| Fields::new(r, pi, schema).ok()).and_then(|f| f.string(b"EDID")).unwrap_or_default();
                Some(FoleySound {
                    sound,
                    chance: v.get("chance").and_then(Value::as_u64).unwrap_or(100).min(255) as u8,
                    armor: label.contains("Armor"),
                    gait: Gait::in_name(&edid).or_else(|| Gait::in_name(&label)),
                })
            })
            .collect();
        if !own.is_empty() {
            return own;
        }
        match f.formid(b"TNAM", lo) {
            Some(t) => id = t,
            None => return Vec::new(),
        }
    }
    Vec::new()
}

/// The sound of an armour's for a rustle key, if it has one.
pub fn foley_for(sounds: &[FoleySound], armor: bool, gait: Gait) -> Option<&FoleySound> {
    sounds.iter().find(|s| s.armor == armor && s.gait == Some(gait))
}

/// Of the sounds of several worn pieces, the ones that rustle: the first piece with
/// armour sounds, else the first with any [design].
pub fn worn_foley(pieces: Vec<Vec<FoleySound>>) -> Vec<FoleySound> {
    let mut first = None;
    for p in pieces {
        if p.iter().any(|s| s.armor) {
            return p;
        }
        if first.is_none() && !p.is_empty() {
            first = Some(p);
        }
    }
    first.unwrap_or_default()
}

/// The Havok material of a landscape texture (LTEX HNAM's first byte) [documented:
/// xEdit wbDefinitionsFNV.pas LTEX HNAM, `schemas/fnv/world.json`].
pub fn land_texture_material(lo: &LoadOrder, schema: &Schema, ltex: u32) -> Option<u32> {
    let f = winner(lo, ltex).and_then(|(pi, r)| Fields::new(r, pi, schema).ok())?;
    f.raw(b"HNAM").and_then(|s| s.data.first()).map(|&b| u32::from(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(v: &[(f32, &str)]) -> Vec<(f32, String)> {
        v.iter().map(|&(t, k)| (t, k.to_string())).collect()
    }

    #[test]
    fn keys_are_read() {
        assert_eq!(parse("Sound: WPNPistol10mmReloadOut"), Some(KeySound::Sound("WPNPistol10mmReloadOut")));
        assert_eq!(parse("sound: DRSOpen"), Some(KeySound::Sound("DRSOpen")));
        assert_eq!(parse("Sound: OBJVertibirdBladesALP SoundNodeProps"), Some(KeySound::Sound("OBJVertibirdBladesALP")));
        assert_eq!(parse("Enum: Left  "), Some(KeySound::Foot(Side::Left)));
        assert_eq!(parse("Enum: right"), Some(KeySound::Foot(Side::Right)));
        assert_eq!(parse("Enum: ArmorWalk"), Some(KeySound::Foley { armor: true, gait: Gait::Walk }));
        assert_eq!(parse("Enum: FoleyRun"), Some(KeySound::Foley { armor: false, gait: Gait::Run }));
        assert_eq!(parse("Enum: Equip"), None);
        assert_eq!(parse("Blend: 9"), None);
        assert_eq!(parse("start"), None);
        assert_eq!(parse("Sound:"), None);
    }

    #[test]
    fn keys_passed_in_a_step_and_round_the_end() {
        let k = keys(&[(0.0, "a"), (0.3, "b"), (0.9, "c"), (1.0, "d")]);
        assert_eq!(passed(&k, 0.0, None, 0.1).collect::<Vec<_>>(), ["a"]);
        assert_eq!(passed(&k, 0.0, Some(0.1), 0.3).collect::<Vec<_>>(), ["b"]);
        assert!(passed(&k, 0.0, Some(0.3), 0.5).next().is_none());
        // Round the end: after 0.8, then from the start to 0.05.
        assert_eq!(passed(&k, 0.0, Some(0.8), 0.05).collect::<Vec<_>>(), ["a", "c", "d"]);
    }

    #[test]
    fn gaits_and_footstep_names() {
        let walk = keys(&[(0.33, "Enum: Left"), (0.33, "Enum: ArmorWalk")]);
        assert_eq!(clip_gait(&walk, "Forward"), Gait::Walk);
        assert_eq!(clip_gait(&[], "FastForward"), Gait::Run);
        assert_eq!(clip_gait(&[], "sneakmtforward"), Gait::Sneak);
        assert_eq!(footstep_family(2), ("Dirt", false));
        assert_eq!(footstep_family(9 + 64), ("Wood", true));
        assert_eq!(footstep_family(16), ("MetalHollow", false));
        let c = footstep_candidates("Wood", true, Gait::Run, Side::Left);
        assert_eq!(c[..3], ["FSTWoodStairsL", "FSTWoodRunL", "FSTWoodWalkL"]);
        assert_eq!(footstep_candidates("Snow", false, Gait::Walk, Side::Right)[2], "FSTSnowWalk");
        assert_eq!(footstep_candidates("Dirt", false, Gait::Sneak, Side::Right).last().unwrap(), "FSTDirtWalk");
    }

    #[test]
    fn rustle_by_kind_and_gait() {
        let s = |armor, gait| FoleySound { sound: 1, chance: 100, armor, gait: Some(gait) };
        let heavy = vec![s(true, Gait::Walk), s(true, Gait::Run)];
        let cloth = vec![s(false, Gait::Walk)];
        assert!(foley_for(&heavy, true, Gait::Run).is_some());
        assert!(foley_for(&heavy, false, Gait::Run).is_none());
        assert_eq!(worn_foley(vec![vec![], cloth.clone(), heavy.clone()]), heavy);
        assert_eq!(worn_foley(vec![cloth.clone()]), cloth);
    }
}
