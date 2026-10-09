//! Which `.kf` files a humanoid under direct control plays: standing, moving in four
//! directions at walking or running pace, sneaking, swimming, jumping, turning on the
//! spot, and drawing, holding and holstering a weapon (docs/spec-notes/animation.md §8).
//! Headless: the engine passes what the actor is doing and gets candidate files, in order
//! of preference, relative to the skeleton's folder; it plays the first that exists. The
//! weapon draw/holster sequence is the small state machine [`WeaponHands`].
//!
//! File names follow the human skeleton folder's layout [observed: the file list of
//! `meshes\characters\_male` in `Fallout - Meshes.bsa`, §8.1]; how the game chooses among
//! them is [hypothesis] throughout (Layer 6 will replace this with the game's own rules
//! once captured).

/// What a weapon's `WEAP` DNAM "Animation Type" selects (0–13) [documented: xEdit
/// wbWeaponAnimTypeEnum], as the file-name prefix of its clips [hypothesis: the prefixes
/// of §3.2 of the spec note; energy pistols share `1hp`; energy rifles take `2ha`, whose
/// folder holds `2hahandgrip1_plasmarifle.kf`; thrown weapons share `1gt`].
pub fn weapon_prefix(animation_type: u32) -> Option<&'static str> {
    Some(match animation_type {
        0 => "h2h",
        1 => "1hm",
        2 => "2hm",
        3 | 4 => "1hp",
        5 => "2hr",
        6 | 7 => "2ha",
        8 => "2hh",
        9 => "2hl",
        10 | 13 => "1gt",
        11 => "1lm",
        12 => "1md",
        _ => return None,
    })
}

/// The prefix used with no weapon equipped: the fists [hypothesis: the game's unarmed
/// player raises their fists, `h2hequip.kf`].
pub const UNARMED: &str = "h2h";

/// A movement direction relative to the actor's facing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dir {
    Forward,
    Backward,
    Left,
    Right,
}

impl Dir {
    /// As in the file names (`mtforward.kf`, `sneakmtleft.kf`).
    pub fn name(self) -> &'static str {
        match self {
            Dir::Forward => "forward",
            Dir::Backward => "backward",
            Dir::Left => "left",
            Dir::Right => "right",
        }
    }
}

/// Clip weights for a movement input (`forward` +1 ahead / −1 back, `right` +1 right /
/// −1 left): a diagonal blends the forward or backward clip with the sideways one in
/// proportion to the two inputs; `None` when standing still [hypothesis: the game may
/// pick the nearer of the two instead of blending].
pub fn direction_weights(forward: f32, right: f32) -> Option<[(Dir, f32); 2]> {
    let (f, r) = (forward.abs(), right.abs());
    if f + r < 1e-3 {
        return None;
    }
    let along = if forward >= 0.0 { Dir::Forward } else { Dir::Backward };
    let side = if right >= 0.0 { Dir::Right } else { Dir::Left };
    Some([(along, f / (f + r)), (side, r / (f + r))])
}

/// Posture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Stance {
    #[default]
    Stand,
    /// Crouched (sneaking).
    Sneak,
    Swim,
}

impl Stance {
    /// The prefix of the posture's own clips (`sneakmtidle.kf`, `swimmtforward.kf`).
    fn prefix(self) -> &'static str {
        match self {
            Stance::Stand => "",
            Stance::Sneak => "sneak",
            Stance::Swim => "swim",
        }
    }
}

/// The posture's idle, the base every other clip is laid over: `locomotion\mtidle.kf`
/// standing (actors.md §7), `sneakmtidle.kf` and `swimmtidle.kf` otherwise.
pub fn idle_clips(stance: Stance) -> Vec<String> {
    match stance {
        Stance::Stand => vec![r"locomotion\mtidle.kf".into()],
        s => vec![format!("{}mtidle.kf", s.prefix()), r"locomotion\mtidle.kf".into()],
    }
}

/// Moving in `dir`, running when `fast`, with the weapon of prefix `drawn` in hand.
/// Standing: the weapon's own movement clip (`locomotion\1hpforward.kf`), else the
/// unarmed one for the sex (`locomotion\female\mtforward.kf`, male as the fallback).
/// Sneaking and swimming: the weapon's (`sneak2hhforward.kf`), else the posture's
/// (`sneakmtforward.kf`). Where no weapon clip exists the weapon is held by the aim clip
/// laid over the movement ([`aim_clips`]).
pub fn move_clips(stance: Stance, female: bool, fast: bool, dir: Dir, drawn: Option<&str>) -> Vec<String> {
    let f = if fast { "fast" } else { "" };
    let d = dir.name();
    let mut out = Vec::new();
    match stance {
        Stance::Stand => {
            if let Some(p) = drawn {
                out.push(format!(r"locomotion\{p}{f}{d}.kf"));
            }
            if female {
                out.push(format!(r"locomotion\female\mt{f}{d}.kf"));
            }
            out.push(format!(r"locomotion\male\mt{f}{d}.kf"));
        }
        s => {
            let s = s.prefix();
            if let Some(p) = drawn {
                out.push(format!("{s}{p}{f}{d}.kf"));
            }
            out.push(format!("{s}mt{f}{d}.kf"));
            if fast {
                out.push(format!("{s}mt{d}.kf"));
            }
        }
    }
    out
}

/// How the weapon is pointed: level, or towards the aim-up or aim-down extreme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Aim {
    Level,
    Up,
    Down,
}

/// The pose of holding weapon `prefix` out (its `Aim` group: `1hpaim.kf`, `aimup`,
/// `aimdown`), sneaking first when crouched [hypothesis: the game's weapon-out stance is
/// the Aim group; the clip loops]. With `sights`, aiming down the sights (`AimIS`:
/// `2hraimis.kf`, `aimisup`, `aimisdown`; pistols, rifles and automatics have them
/// [observed: file list]), falling back to the plain aim.
pub fn aim_clips(stance: Stance, prefix: &str, aim: Aim, sights: bool) -> Vec<String> {
    let a = match aim {
        Aim::Level => "",
        Aim::Up => "up",
        Aim::Down => "down",
    };
    let mut out = Vec::new();
    for is in if sights { &["is", ""][..] } else { &[""][..] } {
        if stance == Stance::Sneak {
            out.push(format!("sneak{prefix}aim{is}{a}.kf"));
        }
        out.push(format!("{prefix}aim{is}{a}.kf"));
    }
    out
}

/// Weights of the level, up and down aim clips for a view pitch (radians, negative looks
/// down): linear between level and the extremes at ±90° [hypothesis: the angle the
/// extreme clips stand for is not known].
pub fn aim_weights(pitch: f32) -> [(Aim, f32); 3] {
    let t = (pitch.abs() / std::f32::consts::FRAC_PI_2).clamp(0.0, 1.0);
    let (up, down) = if pitch >= 0.0 { (t, 0.0) } else { (0.0, t) };
    [(Aim::Level, 1.0 - t), (Aim::Up, up), (Aim::Down, down)]
}

/// The weapon in its holster: a short clamped clip whose `Weapon` track places the weapon
/// on the bone its `prn:` key names (`1hpholster.kf`: `Bip01 Pelvis`; `2hrholster.kf`:
/// `Bip01 Spine2`) [observed: tracks and keys, §8.3; hypothesis: it is held while the
/// weapon is holstered].
pub fn holster_clips(stance: Stance, prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    if stance == Stance::Sneak {
        out.push(format!("sneak{prefix}holster.kf"));
    }
    out.push(format!("{prefix}holster.kf"));
    out
}

/// Drawing (`equip`) or holstering (`unequip`) weapon `prefix`, played once.
pub fn equip_clips(stance: Stance, prefix: &str, equip: bool) -> Vec<String> {
    let e = if equip { "equip" } else { "unequip" };
    let mut out = Vec::new();
    if stance == Stance::Sneak {
        out.push(format!("sneak{prefix}{e}.kf"));
    }
    out.push(format!("{prefix}{e}.kf"));
    out
}

/// In the air (`land` false: looped) or landing (`land` true: once), moving in `dir`.
/// The weapon's clips first (`1hpjumploopforward.kf`), then the posture's
/// (`sneakmtjumplandleft.kf`), then the plain unarmed ones (`mtjumploop.kf`).
pub fn jump_clips(stance: Stance, drawn: Option<&str>, dir: Option<Dir>, land: bool) -> Vec<String> {
    let j = if land { "jumpland" } else { "jumploop" };
    let d = dir.map_or("", Dir::name);
    let s = if stance == Stance::Sneak { "sneak" } else { "" };
    let mut out = Vec::new();
    for p in drawn.into_iter().chain(["mt"]) {
        if !s.is_empty() && p == "mt" {
            out.push(format!("{s}mt{j}{d}.kf"));
            out.push(format!("{s}mt{j}.kf"));
        }
        out.push(format!("{p}{j}{d}.kf"));
        out.push(format!("{p}{j}.kf"));
    }
    out.dedup();
    out
}

/// Turning on the spot (left: counter-clockwise seen from above).
pub fn turn_clips(stance: Stance, drawn: Option<&str>, left: bool) -> Vec<String> {
    let t = if left { "turnleft" } else { "turnright" };
    let mut out = Vec::new();
    match stance {
        Stance::Stand => {
            if let Some(p) = drawn {
                out.push(format!("{p}{t}.kf"));
                out.push(format!(r"locomotion\{p}{t}.kf"));
            }
            out.push(format!(r"locomotion\mt{t}.kf"));
        }
        s => {
            out.push(format!("{}mt{t}.kf", s.prefix()));
            out.push(format!(r"locomotion\mt{t}.kf"));
        }
    }
    out
}

/// What a clip's text keys say about blending and the weapon [observed: key texts of the
/// human clips, §8.3]: keys hold one or more lines (`Blend:6`, `prn: Bip01 R Hand`,
/// `Attach`, `Detach`, `Enum: …`, `m:L`, …); case and spaces vary.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClipKeys {
    /// `Blend:N`: N frames [hypothesis: the time to blend the clip in, at 30 frames per
    /// second — key times sit on a 1/30 s grid].
    pub blend_frames: Option<f32>,
    /// `prn: <bone>` (the first): the bone the weapon hangs from from then on.
    pub prn: Option<(f32, String)>,
    /// `Attach`: the weapon moves into the hand.
    pub attach: Option<f32>,
    /// `Detach`: the weapon leaves the hand for the holster.
    pub detach: Option<f32>,
}

impl ClipKeys {
    pub fn parse(keys: &[(f32, String)]) -> ClipKeys {
        let mut out = ClipKeys::default();
        for (t, text) in keys {
            for line in text.split(['\r', '\n']).map(str::trim).filter(|l| !l.is_empty()) {
                let lower = line.to_ascii_lowercase();
                if let Some(v) = lower.strip_prefix("blend:") {
                    if out.blend_frames.is_none() {
                        out.blend_frames = v.trim().parse().ok();
                    }
                } else if lower.starts_with("prn:") {
                    if out.prn.is_none() {
                        out.prn = Some((*t, line[4..].trim().to_string()));
                    }
                } else if lower == "attach" {
                    out.attach.get_or_insert(*t);
                } else if lower == "detach" {
                    out.detach.get_or_insert(*t);
                }
            }
        }
        out
    }

    /// The blend-in time in seconds ([`Self::blend_frames`] at 30 per second), else
    /// `default`.
    pub fn blend_seconds(&self, default: f32) -> f32 {
        self.blend_frames.map_or(default, |f| f / 30.0)
    }
}

/// Where the weapon is in its draw/holster cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WeaponPhase {
    #[default]
    Holstered,
    /// Playing the equip clip.
    Drawing,
    Drawn,
    /// Playing the unequip clip.
    Holstering,
}

/// A draw or holster clip's timing: its length and when the weapon changes hands
/// (`Attach` / `Detach` key; half way when the clip has none [engineering choice]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition {
    pub length: f32,
    pub switch_at: f32,
}

impl Transition {
    pub fn new(length: f32, switch_at: Option<f32>) -> Transition {
        let length = length.max(0.0);
        Transition { length, switch_at: switch_at.unwrap_or(length / 2.0).clamp(0.0, length) }
    }
}

/// What changed in one [`WeaponHands::update`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandEvent {
    /// Drawing started (play the equip clip from its start).
    StartDraw,
    /// Holstering started (play the unequip clip).
    StartHolster,
    /// The weapon moved into the hand.
    ToHand,
    /// The weapon moved back to its holster.
    ToHolster,
    Drawn,
    Holstered,
}

/// The weapon draw/holster sequence: asking for the weapon starts the equip clip; at its
/// `Attach` key the weapon moves to the hand, and at its end the weapon is out. Putting
/// it away runs the unequip clip the same way (`Detach`). A request made during a
/// transition waits for it to finish [hypothesis: the game may cut it short].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WeaponHands {
    pub phase: WeaponPhase,
    /// Time into the current transition clip.
    pub time: f32,
    /// Whether the weapon hangs from the hand (else from its holster bone).
    pub in_hand: bool,
}

impl WeaponHands {
    /// Advance by `dt` seconds wanting the weapon out (`want_drawn`) or away; at most a few
    /// events per call, written to `events` (cleared first).
    pub fn update(&mut self, want_drawn: bool, dt: f32, draw: Transition, holster: Transition, events: &mut Vec<HandEvent>) {
        events.clear();
        match self.phase {
            WeaponPhase::Holstered if want_drawn => {
                (self.phase, self.time) = (WeaponPhase::Drawing, 0.0);
                events.push(HandEvent::StartDraw);
            }
            WeaponPhase::Drawn if !want_drawn => {
                (self.phase, self.time) = (WeaponPhase::Holstering, 0.0);
                events.push(HandEvent::StartHolster);
            }
            WeaponPhase::Drawing => {
                self.time += dt;
                if !self.in_hand && self.time >= draw.switch_at {
                    self.in_hand = true;
                    events.push(HandEvent::ToHand);
                }
                if self.time >= draw.length {
                    self.phase = WeaponPhase::Drawn;
                    events.push(HandEvent::Drawn);
                }
            }
            WeaponPhase::Holstering => {
                self.time += dt;
                if self.in_hand && self.time >= holster.switch_at {
                    self.in_hand = false;
                    events.push(HandEvent::ToHolster);
                }
                if self.time >= holster.length {
                    self.phase = WeaponPhase::Holstered;
                    events.push(HandEvent::Holstered);
                }
            }
            _ => {}
        }
    }

    /// Whether the weapon is out (drawn and no longer changing hands).
    pub fn out(&self) -> bool {
        self.phase == WeaponPhase::Drawn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_cover_every_animation_type() {
        let p: Vec<_> = (0..14).map(|t| weapon_prefix(t).unwrap()).collect();
        assert_eq!(p, ["h2h", "1hm", "2hm", "1hp", "1hp", "2hr", "2ha", "2ha", "2hh", "2hl", "1gt", "1lm", "1md", "1gt"]);
        assert_eq!(weapon_prefix(14), None);
    }

    #[test]
    fn diagonals_blend_and_still_is_none() {
        assert_eq!(direction_weights(0.0, 0.0), None);
        assert_eq!(direction_weights(1.0, 0.0), Some([(Dir::Forward, 1.0), (Dir::Right, 0.0)]));
        assert_eq!(direction_weights(-1.0, -1.0), Some([(Dir::Backward, 0.5), (Dir::Left, 0.5)]));
        assert_eq!(direction_weights(0.0, -1.0), Some([(Dir::Forward, 0.0), (Dir::Left, 1.0)]));
    }

    #[test]
    fn movement_files() {
        assert_eq!(move_clips(Stance::Stand, false, true, Dir::Forward, None), [r"locomotion\male\mtfastforward.kf"]);
        assert_eq!(
            move_clips(Stance::Stand, true, false, Dir::Left, Some("1hp")),
            [r"locomotion\1hpleft.kf", r"locomotion\female\mtleft.kf", r"locomotion\male\mtleft.kf"]
        );
        assert_eq!(move_clips(Stance::Sneak, false, false, Dir::Backward, Some("2hh")), ["sneak2hhbackward.kf", "sneakmtbackward.kf"]);
        assert_eq!(move_clips(Stance::Swim, false, true, Dir::Right, None), ["swimmtfastright.kf", "swimmtright.kf"]);
        assert_eq!(idle_clips(Stance::Sneak), ["sneakmtidle.kf", r"locomotion\mtidle.kf"]);
    }

    #[test]
    fn weapon_files() {
        assert_eq!(aim_clips(Stance::Sneak, "1hp", Aim::Up, false), ["sneak1hpaimup.kf", "1hpaimup.kf"]);
        assert_eq!(aim_clips(Stance::Stand, "2hr", Aim::Level, true), ["2hraimis.kf", "2hraim.kf"]);
        assert_eq!(holster_clips(Stance::Stand, "2hr"), ["2hrholster.kf"]);
        assert_eq!(equip_clips(Stance::Sneak, "h2h", false), ["sneakh2hunequip.kf", "h2hunequip.kf"]);
        assert_eq!(
            jump_clips(Stance::Stand, Some("1hp"), Some(Dir::Forward), false),
            ["1hpjumploopforward.kf", "1hpjumploop.kf", "mtjumploopforward.kf", "mtjumploop.kf"]
        );
        assert_eq!(jump_clips(Stance::Sneak, None, None, true), ["sneakmtjumpland.kf", "mtjumpland.kf"]);
        assert_eq!(turn_clips(Stance::Stand, Some("2hh"), true), ["2hhturnleft.kf", r"locomotion\2hhturnleft.kf", r"locomotion\mtturnleft.kf"]);
    }

    #[test]
    fn aim_weights_follow_the_pitch() {
        assert_eq!(aim_weights(0.0), [(Aim::Level, 1.0), (Aim::Up, 0.0), (Aim::Down, 0.0)]);
        let w = aim_weights(-std::f32::consts::FRAC_PI_4);
        assert!((w[0].1 - 0.5).abs() < 1e-6 && w[1].1 == 0.0 && (w[2].1 - 0.5).abs() < 1e-6);
        assert_eq!(aim_weights(3.0)[1].1, 1.0);
    }

    #[test]
    fn keys_of_the_human_clips() {
        // 1hpequip.kf and mtforward.kf [observed].
        let equip = [
            (0.0, "start".to_string()),
            (0.033, "Enum: Equip\r\n".to_string()),
            (0.133, "prn: Bip01 R Hand".to_string()),
            (0.167, "Attach".to_string()),
            (0.367, "end".to_string()),
        ];
        let k = ClipKeys::parse(&equip);
        assert_eq!(k.prn, Some((0.133, "Bip01 R Hand".to_string())));
        assert_eq!((k.attach, k.detach, k.blend_frames), (Some(0.167), None, None));
        let walk = [(0.033, "Blend:6\r\n".to_string()), (0.367, "Enum: ArmorWalk\r\n".to_string())];
        let k = ClipKeys::parse(&walk);
        assert_eq!(k.blend_frames, Some(6.0));
        assert!((k.blend_seconds(0.3) - 0.2).abs() < 1e-6);
        assert_eq!(ClipKeys::parse(&[(0.1, "Blend: 12".into()), (0.2, "Prn: Bip01 Spine2".into())]).blend_frames, Some(12.0));
        assert_eq!(ClipKeys::default().blend_seconds(0.25), 0.25);
    }

    #[test]
    fn draw_and_holster() {
        let draw = Transition::new(0.367, Some(0.167));
        let holster = Transition::new(0.467, Some(0.233));
        let mut h = WeaponHands::default();
        let mut ev = Vec::new();
        h.update(false, 0.1, draw, holster, &mut ev);
        assert!(ev.is_empty() && h.phase == WeaponPhase::Holstered);
        h.update(true, 0.1, draw, holster, &mut ev);
        assert_eq!((ev.as_slice(), h.phase), ([HandEvent::StartDraw].as_slice(), WeaponPhase::Drawing));
        h.update(true, 0.1, draw, holster, &mut ev);
        assert!(ev.is_empty() && !h.in_hand);
        h.update(false, 0.1, draw, holster, &mut ev); // asked away while drawing: finishes first
        assert_eq!(ev, [HandEvent::ToHand]);
        h.update(false, 0.2, draw, holster, &mut ev);
        assert_eq!((ev.as_slice(), h.out()), ([HandEvent::Drawn].as_slice(), true));
        h.update(false, 0.1, draw, holster, &mut ev);
        assert_eq!(ev, [HandEvent::StartHolster]);
        h.update(false, 1.0, draw, holster, &mut ev);
        assert_eq!(ev, [HandEvent::ToHolster, HandEvent::Holstered]);
        assert!(!h.in_hand && h.phase == WeaponPhase::Holstered);
        // No switch key: half way.
        assert_eq!(Transition::new(0.4, None).switch_at, 0.2);
        assert_eq!(Transition::new(0.4, Some(9.0)).switch_at, 0.4);
    }
}
