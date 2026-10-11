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

/// The sound types a creature record lists (CREA `CSDT`) [documented: xEdit
/// wbDefinitionsFNV.pas wbCSDT, `schemas/fnv/gameplay.json`; xEdit names 19 and 20 both
/// "Auxiliary 8", so 20 and 21 are read as Jump and PlayRandom/Loop there].
pub const CREATURE_SOUND_TYPES: [&str; 23] = [
    "Left Foot",
    "Right Foot",
    "Left Back Foot",
    "Right Back Foot",
    "Idle",
    "Aware",
    "Attack",
    "Hit",
    "Death",
    "Weapon",
    "Movement Loop",
    "Conscious Loop",
    "Auxiliary 1",
    "Auxiliary 2",
    "Auxiliary 3",
    "Auxiliary 4",
    "Auxiliary 5",
    "Auxiliary 6",
    "Auxiliary 7",
    "Auxiliary 8",
    "Auxiliary 8 (second)",
    "Jump",
    "PlayRandom/Loop",
];

/// Creature sound types by number.
pub mod creature_sound {
    pub const LEFT_FOOT: u32 = 0;
    pub const RIGHT_FOOT: u32 = 1;
    pub const LEFT_BACK_FOOT: u32 = 2;
    pub const RIGHT_BACK_FOOT: u32 = 3;
    pub const IDLE: u32 = 4;
    pub const AWARE: u32 = 5;
    pub const ATTACK: u32 = 6;
    pub const HIT: u32 = 7;
    pub const DEATH: u32 = 8;
    pub const WEAPON: u32 = 9;
    pub const MOVEMENT_LOOP: u32 = 10;
    pub const CONSCIOUS_LOOP: u32 = 11;
    pub const AUXILIARY_1: u32 = 12;
}

/// The name of a creature sound type.
pub fn creature_sound_type_name(kind: u32) -> &'static str {
    CREATURE_SOUND_TYPES.get(kind as usize).copied().unwrap_or("?")
}

/// One sound a creature lists under a type: `CSDT` the type, then pairs of `CSDI` the
/// sound and `CSDC` its chance [documented: xEdit wbSoundTypeSounds].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreatureSound {
    pub kind: u32,
    pub sound: u32,
    /// Percent.
    pub chance: u8,
}

/// The sounds a creature record lists itself, in order.
pub fn own_creature_sounds(lo: &LoadOrder, f: &Fields) -> Vec<CreatureSound> {
    let mut out = Vec::new();
    let mut kind = None;
    for s in &f.subs {
        let u32_of = |d: &[u8]| d.get(..4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        match &s.sig {
            b"CSDT" => kind = u32_of(&s.data),
            b"CSDI" => {
                if let (Some(kind), Some(sound)) = (kind, u32_of(&s.data).and_then(|l| global(lo, f.plugin, l))) {
                    out.push(CreatureSound { kind, sound, chance: 100 });
                }
            }
            b"CSDC" => {
                if let (Some(last), Some(&c)) = (out.last_mut(), s.data.first()) {
                    last.chance = c;
                }
            }
            _ => {}
        }
    }
    out
}

/// Where a creature's sounds come from: its own list, else the creature it inherits
/// sounds from (`CSCR`, followed up to four times) [documented: xEdit "Inherits Sounds
/// from"; hypothesis: an own list wins]. The sounds and the record they came from.
pub fn creature_sounds(lo: &LoadOrder, schema: &Schema, crea: u32) -> (Vec<CreatureSound>, u32) {
    let mut id = crea;
    for _ in 0..5 {
        let Some(f) = winner(lo, id).and_then(|(pi, r)| Fields::new(r, pi, schema).ok()) else { break };
        if &f.sig != b"CREA" {
            break;
        }
        let own = own_creature_sounds(lo, &f);
        if !own.is_empty() {
            return (own, id);
        }
        match f.formid(b"CSCR", lo) {
            Some(next) if next != id => id = next,
            _ => break,
        }
    }
    (Vec::new(), id)
}

/// The creature sound type an `Enum:` key stands for [hypothesis: by the names; xEdit's
/// type names and the keys match one to one]: `Left`, `Right`, `BackLeft`, `BackRight`,
/// `Idle`, `Aware`, `Attack`, `Hit`, `Auxiliary1`–`8` (also `Auxiliary 3`).
pub fn creature_key_type(key: &str) -> Option<u32> {
    use creature_sound::*;
    let key = key.trim();
    let (head, rest) = key.split_once(':')?;
    if !head.trim().eq_ignore_ascii_case("enum") {
        return None;
    }
    let w: String = rest.trim().chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_lowercase();
    Some(match w.as_str() {
        "left" => LEFT_FOOT,
        "right" => RIGHT_FOOT,
        "backleft" => LEFT_BACK_FOOT,
        "backright" => RIGHT_BACK_FOOT,
        "idle" => IDLE,
        "aware" => AWARE,
        "attack" => ATTACK,
        "hit" => HIT,
        _ => {
            let n: u32 = w.strip_prefix("auxiliary")?.parse().ok()?;
            if !(1..=8).contains(&n) {
                return None;
            }
            AUXILIARY_1 + n - 1
        }
    })
}

/// Of a creature's sounds of one type, the one to play this time: one of them at random
/// (`pick` in 0..1), then its chance (`roll` in 0..1) [hypothesis: a type with several
/// sounds holds takes of one sound].
pub fn pick_creature_sound(sounds: &[CreatureSound], kind: u32, pick: f32, roll: f32) -> Option<u32> {
    let of: Vec<&CreatureSound> = sounds.iter().filter(|s| s.kind == kind).collect();
    if of.is_empty() {
        return None;
    }
    let s = of[((pick * of.len() as f32) as usize).min(of.len() - 1)];
    (roll * 100.0 < f32::from(s.chance)).then_some(s.sound)
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
    fn creature_keys_and_sounds() {
        use creature_sound::*;
        assert_eq!(creature_key_type("Enum: Left"), Some(LEFT_FOOT));
        assert_eq!(creature_key_type("Enum: BackRight"), Some(RIGHT_BACK_FOOT));
        assert_eq!(creature_key_type("enum: backLeft"), Some(LEFT_BACK_FOOT));
        assert_eq!(creature_key_type("Enum: Auxiliary1"), Some(AUXILIARY_1));
        assert_eq!(creature_key_type("Enum: Auxiliary 3"), Some(AUXILIARY_1 + 2));
        assert_eq!(creature_key_type("Enum: Auxiliary9"), None);
        assert_eq!(creature_key_type("Enum: attack"), Some(ATTACK));
        assert_eq!(creature_key_type("Enum: ArmorWalk"), None);
        assert_eq!(creature_key_type("Sound: NPCGeckoSnarl"), None);
        assert_eq!(creature_sound_type_name(IDLE), "Idle");
        let list = [
            CreatureSound { kind: LEFT_FOOT, sound: 1, chance: 100 },
            CreatureSound { kind: ATTACK, sound: 2, chance: 50 },
            CreatureSound { kind: ATTACK, sound: 3, chance: 50 },
        ];
        assert_eq!(pick_creature_sound(&list, LEFT_FOOT, 0.99, 0.99), Some(1));
        // One of the type's sounds, then its chance.
        assert_eq!(pick_creature_sound(&list, ATTACK, 0.2, 0.4), Some(2));
        assert_eq!(pick_creature_sound(&list, ATTACK, 0.7, 0.4), Some(3));
        assert_eq!(pick_creature_sound(&list, ATTACK, 0.7, 0.6), None);
        assert_eq!(pick_creature_sound(&list, IDLE, 0.0, 0.0), None);
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
