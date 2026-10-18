//! Which `.kf` files the player's body plays under direct control (docs/design/
//! player-body.md; NPC locomotion is [`crate::locomotion`]): standing, moving in four
//! directions at walking or running pace, sneaking, swimming, jumping, turning on the
//! spot, and drawing, holding and holstering a weapon (docs/spec-notes/animation.md §9).
//! Headless: the engine passes what the actor is doing and gets candidate files, in order
//! of preference, relative to the skeleton's folder; it plays the first that exists. The
//! weapon draw/holster sequence is the small state machine [`WeaponHands`]. Weapon clip
//! prefixes come from the weapon's animation type (`crate::weapon::AnimType::prefix`).
//!
//! File names follow the human skeleton folders' layouts [observed: the file lists of
//! `meshes\characters\_male` and `meshes\characters\_1stperson` in `Fallout -
//! Meshes.bsa`, §9.1 and §9.5]: candidates cover both, and the engine takes the first
//! that exists under the rig's folder. How the game chooses among them is [hypothesis]
//! throughout (Layer 6 will replace this with the game's own rules once captured).

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
/// standing (actors.md §7; `mtidle.kf` in the first-person folder), `sneakmtidle.kf`
/// and `swimmtidle.kf` otherwise.
pub fn idle_clips(stance: Stance) -> Vec<String> {
    let mut out = match stance {
        Stance::Stand => Vec::new(),
        s => vec![format!("{}mtidle.kf", s.prefix())],
    };
    out.extend([r"locomotion\mtidle.kf".to_string(), "mtidle.kf".to_string()]);
    out
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
            // The first-person folder keeps them directly in `locomotion\`.
            out.push(format!(r"locomotion\mt{f}{d}.kf"));
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

/// Standing with a crippled leg: the limping clips of `locomotion\hurt\` first
/// (`mtforward_hurt.kf`; the weapon's own, `1hpforward_hurt.kf`, before them), then
/// [`move_clips`] [observed: both human folders have the four directions walking and
/// running for `mt`, `1hm` and `1hp`, `_male` also `h2hforward_hurt.kf`; design: a
/// weapon without limping clips limps with the unarmed legs and holds the weapon by its
/// aim clip, as it does where it has no movement clips; docs/spec-notes/actors.md §8.11].
pub fn limping_move_clips(female: bool, fast: bool, dir: Dir, drawn: Option<&str>) -> Vec<String> {
    let f = if fast { "fast" } else { "" };
    let d = dir.name();
    let mut out = Vec::new();
    if let Some(p) = drawn {
        out.push(format!(r"locomotion\hurt\{p}{f}{d}_hurt.kf"));
    }
    out.push(format!(r"locomotion\hurt\mt{f}{d}_hurt.kf"));
    out.extend(move_clips(Stance::Stand, female, fast, dir, drawn));
    out
}

/// The standing idle with a crippled leg: `locomotion\hurt\mtidle.kf` (`_male` only; it
/// is not the same file as `locomotion\mtidle.kf` [observed]), then [`idle_clips`].
pub fn limping_idle_clips() -> Vec<String> {
    let mut out = vec![r"locomotion\hurt\mtidle.kf".to_string()];
    out.extend(idle_clips(Stance::Stand));
    out
}

/// Turning on the spot with a crippled leg: the weapon's own turn still first (it moves
/// the upper body), then `locomotion\hurt\mtturnleft.kf`, then the unarmed turns of
/// [`turn_clips`].
pub fn limping_turn_clips(drawn: Option<&str>, left: bool) -> Vec<String> {
    let t = if left { "turnleft" } else { "turnright" };
    let mut out = turn_clips(Stance::Stand, drawn, left);
    let at = out.iter().position(|c| c.ends_with(&format!("mt{t}.kf"))).unwrap_or(out.len());
    out.insert(at, format!(r"locomotion\hurt\mt{t}.kf"));
    out
}

/// How the weapon is pointed: level, or towards the aim-up or aim-down extreme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Aim {
    Level,
    Up,
    Down,
}

impl Aim {
    /// What a clip's file name ends in for this aim (`2hraim.kf`, `2hraimup.kf`,
    /// `2hraimdown.kf`).
    fn ending(self) -> &'static str {
        match self {
            Aim::Level => "",
            Aim::Up => "up",
            Aim::Down => "down",
        }
    }
}

/// The up or down form of the clip at `path`: the same name with `up` or `down` added
/// (`2hrattackleft.kf` → `2hrattackleftup.kf`, `2hrattackleftis.kf` →
/// `2hrattackleftisdown.kf`) [observed: file names, animation.md §9.6]. Aims and attacks
/// have them; whether a given clip does is for the caller to look up.
pub fn aim_variant(path: &str, aim: Aim) -> String {
    let stem = path.strip_suffix(".kf").unwrap_or(path);
    format!("{stem}{}.kf", aim.ending())
}

/// The pose of holding weapon `prefix` out (its `Aim` group: `1hpaim.kf`, `aimup`,
/// `aimdown`), sneaking first when crouched [hypothesis: the game's weapon-out stance is
/// the Aim group; the clip loops]. With `sights`, aiming down the sights (`AimIS`:
/// `2hraimis.kf`, `aimisup`, `aimisdown`; pistols, rifles and automatics have them
/// [observed: file list]), falling back to the plain aim.
pub fn aim_clips(stance: Stance, prefix: &str, aim: Aim, sights: bool) -> Vec<String> {
    let a = aim.ending();
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
/// down): linear between level and the extremes at ±90° [observed: the rifles' up and
/// down clips turn each of the three spine bones 30° from the level clip's pose,
/// animation.md §9.6; hypothesis: that the three add up to the 90° the extremes stand
/// for, and that the game's weights are linear in the pitch]. The level clip's weight
/// reaches 0 at the extremes but it is still to be played: it may be the only one of the
/// three with the arms.
pub fn aim_weights(pitch: f32) -> [(Aim, f32); 3] {
    let t = (pitch.abs() / std::f32::consts::FRAC_PI_2).clamp(0.0, 1.0);
    let (up, down) = if pitch >= 0.0 { (t, 0.0) } else { (0.0, t) };
    [(Aim::Level, 1.0 - t), (Aim::Up, up), (Aim::Down, down)]
}

/// Whether the Block control blocks with weapon `prefix` (fists and melee) rather than
/// aiming down the sights (guns) [hypothesis: the control's name, and the block clips
/// exist only for `1hm`, `2hm` and the unarmed `mt`, §9.1].
pub fn blocks(prefix: &str) -> bool {
    matches!(prefix, "h2h" | "1hm" | "2hm")
}

/// Holding a block with weapon `prefix` (`1hmblockidle.kf`; fists use
/// `mtblockidle.kf`), crouched first when sneaking.
pub fn block_clips(stance: Stance, prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    if stance == Stance::Sneak {
        out.push(format!("sneak{prefix}blockidle.kf"));
    }
    out.push(format!("{prefix}blockidle.kf"));
    out.push("mtblockidle.kf".to_string());
    out
}

/// The weapon in its holster: a short clamped clip whose `Weapon` track places the weapon
/// on the bone its `prn:` key names (`1hpholster.kf`: `Bip01 Pelvis`; `2hrholster.kf`:
/// `Bip01 Spine2`) [observed: tracks and keys, §9.3; hypothesis: it is held while the
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

/// The clip file stem of a WEAP attack animation (`AttackRight` → `attackright`); the
/// weapon's default (`DEFAULT`, or none) plays `AttackRight` [hypothesis].
pub fn attack_stem(attack: Option<&str>) -> String {
    match attack.map(str::to_ascii_lowercase) {
        Some(a) if a.starts_with("attack") || a.starts_with("placemine") => a,
        _ => "attackright".to_string(),
    }
}

/// Attacking with weapon `prefix` (`1hpattackright.kf`; `…is.kf` down the sights; melee
/// and fists have `_a`/`_b` variants, `1hmattackright_a.kf`) [observed: the
/// `_1stperson` and `_male` file lists]: sneaking first when crouched, then the sights
/// form, then the plain one, then the `_a` variant, then `AttackRight` as a fallback.
/// `attack` is [`attack_stem`]'s result, or a melee swing's stem with its variant
/// (`attackleft_b`, `fnv_combat::melee`): that form first, then the group's as above
/// (`h2hattack5_b` falls back to `h2hattack5_a.kf`, `h2hattack6_a` to `h2hattack6.kf`).
pub fn attack_clips(stance: Stance, prefix: &str, attack: &str, sights: bool) -> Vec<String> {
    let mut out = Vec::new();
    let group = match attack.rsplit_once('_') {
        Some((g, "a" | "b")) => {
            if stance == Stance::Sneak {
                out.push(format!("sneak{prefix}{attack}.kf"));
            }
            out.push(format!("{prefix}{attack}.kf"));
            g
        }
        _ => attack,
    };
    for a in [group, "attackright"] {
        for is in if sights { &["is", ""][..] } else { &[""][..] } {
            if stance == Stance::Sneak {
                out.push(format!("sneak{prefix}{a}{is}.kf"));
            }
            out.push(format!("{prefix}{a}{is}.kf"));
        }
        out.push(format!("{prefix}{a}_a.kf"));
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(c.clone()));
    out
}

/// Reloading weapon `prefix` with WEAP reload animation `reload` (`ReloadA` →
/// `1hpreloada.kf`) [observed: file lists; the DNAM enum names], sneaking first.
pub fn reload_clips(stance: Stance, prefix: &str, reload: &str) -> Vec<String> {
    let r = reload.to_ascii_lowercase();
    let mut out = Vec::new();
    if stance == Stance::Sneak {
        out.push(format!("sneak{prefix}{r}.kf"));
    }
    out.push(format!("{prefix}{r}.kf"));
    out
}

/// The opening clip of a reload that loads round by round (`ReloadX` →
/// `1hpreloadxstart.kf`), sneaking first. Only `ReloadX` (`1hp`, `2hr`), `ReloadY` and
/// `ReloadZ` (`2hr`) have one, in both rigs' folders [observed: the base meshes archive,
/// 2026-10-11; docs/spec-notes/combat-rules.md §4].
pub fn reload_open_clips(stance: Stance, prefix: &str, reload: &str) -> Vec<String> {
    reload_clips(stance, prefix, &format!("{reload}start"))
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
            out.push(format!("mt{t}.kf"));
        }
        s => {
            out.push(format!("{}mt{t}.kf", s.prefix()));
            out.push(format!(r"locomotion\mt{t}.kf"));
        }
    }
    out
}

/// What a clip's text keys say about blending and the weapon [observed: key texts of the
/// human clips, §9.3]: keys hold one or more lines (`Blend:6`, `prn: Bip01 R Hand`,
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
    /// `Hit` (or `Fire`, the first of either): when a ranged weapon fires or a melee one
    /// strikes [documented: GECK wiki NiTextKeyExtraData, owner export 2026-10-09]; a
    /// thrown weapon's `Release`, when it leaves the hand [observed: the throw clips
    /// `1gtattackthrow*.kf` have `Hold`, `Release` and `Attach` and no `Hit` or `Fire`;
    /// `1gtattackthrow.kf` releases at 0.867 s, where the original's player read 5 for
    /// 50–52 game loops before 6, capture `player_actions_anim2`].
    pub hit: Option<f32>,
    /// `Loop`: where the part of an attack clip that repeats while the trigger is held
    /// ends; it starts at `Fire` and the rest of the clip is the way out [documented:
    /// GECK wiki NiTextKeyExtraData, `Fire` "starts the internal loop of a clamped
    /// sequence; `Loop` jumps back to it" (combat.md §4.2); observed: in the base
    /// archives' `2hhattackspin.kf` the minigun's barrel speeds up until `Fire`, turns at
    /// one speed from there to `Loop` and slows down after it; animation.md §9.8].
    pub loop_end: Option<f32>,
    /// `a:<group>`: the attack group to play next (`L`, `R`, `7`, or blank) and, by its
    /// time, from when the clip may be cut for it [documented: GECK wiki
    /// NiTextKeyExtraData; observed: `a:L` on the melee `AttackRight` clips, `a:R` on the
    /// `AttackLeft` ones; `fnv_combat::melee`].
    pub next_attack: Option<(f32, String)>,
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
                } else if lower == "hit" || lower == "fire" || lower == "release" {
                    out.hit.get_or_insert(*t);
                } else if lower == "loop" {
                    out.loop_end.get_or_insert(*t);
                } else if let Some(v) = lower.strip_prefix("a:") {
                    out.next_attack.get_or_insert((*t, v.trim().to_string()));
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
    fn attack_and_reload_clips() {
        assert_eq!(attack_stem(Some("AttackLoop")), "attackloop");
        assert_eq!(attack_stem(Some("DEFAULT")), "attackright");
        assert_eq!(attack_stem(None), "attackright");
        let c = attack_clips(Stance::Stand, "1hp", "attackright", true);
        assert_eq!(c[..3], ["1hpattackrightis.kf".to_string(), "1hpattackright.kf".to_string(), "1hpattackright_a.kf".to_string()]);
        let c = attack_clips(Stance::Stand, "h2h", "attack5_b", false);
        assert_eq!(c[..3], ["h2hattack5_b.kf".to_string(), "h2hattack5.kf".to_string(), "h2hattack5_a.kf".to_string()]);
        assert_eq!(attack_clips(Stance::Stand, "1hm", "attackforwardpower", false)[0], "1hmattackforwardpower.kf");
        assert_eq!(attack_clips(Stance::Sneak, "h2h", "attackpower", false)[..2], ["sneakh2hattackpower.kf".to_string(), "h2hattackpower.kf".to_string()]);
        let c = attack_clips(Stance::Sneak, "2hr", "attack3", false);
        assert_eq!(c[0], "sneak2hrattack3.kf");
        assert!(c.contains(&"2hrattackright.kf".to_string()));
        assert_eq!(reload_clips(Stance::Stand, "1hp", "ReloadL"), vec!["1hpreloadl.kf".to_string()]);
        assert_eq!(reload_open_clips(Stance::Sneak, "2hr", "ReloadZ"), vec!["sneak2hrreloadzstart.kf".to_string(), "2hrreloadzstart.kf".to_string()]);
        let k = ClipKeys::parse(&[(0.0, "Start".into()), (0.1, "Hit".into()), (0.3, "Fire".into())]);
        assert_eq!(k.hit, Some(0.1));
        // h2hattackright_a.kf's keys [observed].
        let k = ClipKeys::parse(&[(0.0, "start".into()), (0.3, "Hit".into()), (0.3, "Decal: 0".into()), (0.633, "a:L".into()), (1.433, "end".into())]);
        assert_eq!((k.hit, k.next_attack), (Some(0.3), Some((0.633, "l".to_string()))));
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
        assert_eq!(move_clips(Stance::Stand, false, true, Dir::Forward, None), [r"locomotion\male\mtfastforward.kf", r"locomotion\mtfastforward.kf"]);
        assert_eq!(
            move_clips(Stance::Stand, true, false, Dir::Left, Some("1hp")),
            [r"locomotion\1hpleft.kf", r"locomotion\female\mtleft.kf", r"locomotion\male\mtleft.kf", r"locomotion\mtleft.kf"]
        );
        assert_eq!(move_clips(Stance::Sneak, false, false, Dir::Backward, Some("2hh")), ["sneak2hhbackward.kf", "sneakmtbackward.kf"]);
        assert_eq!(move_clips(Stance::Swim, false, true, Dir::Right, None), ["swimmtfastright.kf", "swimmtright.kf"]);
        assert_eq!(idle_clips(Stance::Sneak), ["sneakmtidle.kf", r"locomotion\mtidle.kf", "mtidle.kf"]);
        assert_eq!(idle_clips(Stance::Stand), [r"locomotion\mtidle.kf", "mtidle.kf"]);
    }

    #[test]
    fn weapon_files() {
        assert_eq!(aim_clips(Stance::Sneak, "1hp", Aim::Up, false), ["sneak1hpaimup.kf", "1hpaimup.kf"]);
        assert_eq!(aim_clips(Stance::Stand, "2hr", Aim::Level, true), ["2hraimis.kf", "2hraim.kf"]);
        assert_eq!(aim_variant(r"meshes\characters\_male\2hrattackleftis.kf", Aim::Down), r"meshes\characters\_male\2hrattackleftisdown.kf");
        assert_eq!(aim_variant("2hrattackleft.kf", Aim::Up), "2hrattackleftup.kf");
        assert_eq!(aim_variant("2hraim.kf", Aim::Level), "2hraim.kf");
        assert_eq!(holster_clips(Stance::Stand, "2hr"), ["2hrholster.kf"]);
        assert_eq!(equip_clips(Stance::Sneak, "h2h", false), ["sneakh2hunequip.kf", "h2hunequip.kf"]);
        assert_eq!(
            jump_clips(Stance::Stand, Some("1hp"), Some(Dir::Forward), false),
            ["1hpjumploopforward.kf", "1hpjumploop.kf", "mtjumploopforward.kf", "mtjumploop.kf"]
        );
        assert_eq!(jump_clips(Stance::Sneak, None, None, true), ["sneakmtjumpland.kf", "mtjumpland.kf"]);
        assert_eq!(
            turn_clips(Stance::Stand, Some("2hh"), true),
            ["2hhturnleft.kf", r"locomotion\2hhturnleft.kf", r"locomotion\mtturnleft.kf", "mtturnleft.kf"]
        );
        assert!(blocks("1hm") && !blocks("2hr"));
        assert_eq!(block_clips(Stance::Stand, "h2h"), ["h2hblockidle.kf", "mtblockidle.kf"]);
    }

    #[test]
    fn a_crippled_leg_limps_first() {
        assert_eq!(
            limping_move_clips(false, true, Dir::Forward, Some("1hp")),
            [
                r"locomotion\hurt\1hpfastforward_hurt.kf",
                r"locomotion\hurt\mtfastforward_hurt.kf",
                r"locomotion\1hpfastforward.kf",
                r"locomotion\male\mtfastforward.kf",
                r"locomotion\mtfastforward.kf"
            ]
        );
        assert_eq!(limping_idle_clips(), [r"locomotion\hurt\mtidle.kf", r"locomotion\mtidle.kf", "mtidle.kf"]);
        assert_eq!(
            limping_turn_clips(Some("2hh"), true),
            ["2hhturnleft.kf", r"locomotion\2hhturnleft.kf", r"locomotion\hurt\mtturnleft.kf", r"locomotion\mtturnleft.kf", "mtturnleft.kf"]
        );
        assert_eq!(limping_turn_clips(None, false), [r"locomotion\hurt\mtturnright.kf", r"locomotion\mtturnright.kf", "mtturnright.kf"]);
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
        // 2hhattackspin.kf: the shot, and where the repeated part ends.
        let spin = [(0.033, "BlendIn:3".to_string()), (0.967, "Fire".to_string()), (1.633, "Loop".to_string()), (2.5, "end".to_string())];
        let k = ClipKeys::parse(&spin);
        assert_eq!((k.hit, k.loop_end), (Some(0.967), Some(1.633)));
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
