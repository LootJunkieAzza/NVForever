//! Clean-room support for `BulletSnap.ini` projectile pass-by snap audio
//! (docs/spec-notes/audio.md §19).
//!
//! Configuration schema, section/key names, inline key comments, and default values come
//! strictly from **Bullet Snap - ReRedux v1.2**
//! (`https://www.nexusmods.com/newvegas/mods/89474`, by LowbeeBob), shipped
//! `Data\NVSE\Plugins\BulletSnapConfig\BulletSnap.ini`; the ten loose audio folder paths
//! under `Data\Sound\fx\` are `[observed]` from that same v1.2 release archive. The
//! plugin's DLL/binary and source code were never consulted.
//!
//! When `BulletSnap.ini` is absent, callers keep BulletSnap inactive so unmodded
//! vanilla projectile audio and physics remain unchanged [design].

use std::collections::HashMap;

use crate::vec::{self, V3};

/// Case-insensitive VFS path (relative to `Data\`) of `BulletSnap.ini` [documented:
/// Bullet Snap - ReRedux v1.2 (`https://www.nexusmods.com/newvegas/mods/89474`), shipped
/// archive install path `Data\NVSE\Plugins\BulletSnapConfig\BulletSnap.ini`].
pub const BULLET_SNAP_INI_PATH: &str = "nvse\\plugins\\bulletsnapconfig\\bulletsnap.ini";

/// Default INI values [documented: Bullet Snap - ReRedux v1.2
/// (`https://www.nexusmods.com/newvegas/mods/89474`), shipped
/// `Data\NVSE\Plugins\BulletSnapConfig\BulletSnap.ini`].
pub const DEFAULT_IMPACTED_PLAYER_SNAP: bool = false;
pub const DEFAULT_DISTANCED_BASED_SNAP: bool = true;
pub const DEFAULT_SUBSONIC: bool = true;
pub const DEFAULT_SUBSONIC_THRESHOLD_SPEED: f32 = 21_000.0;
pub const DEFAULT_SNAP_ANGLE_DEG: f32 = 75.0;
pub const DEFAULT_VERY_CLOSE: f32 = 768.0;
pub const DEFAULT_CLOSE: f32 = 1_536.0;
pub const DEFAULT_DISTANCE: f32 = 4_092.0;
pub const DEFAULT_FAR: f32 = 8_192.0;
pub const DEFAULT_MIDDLE_DMG: f32 = 32.0;
pub const DEFAULT_HIGH_DMG: f32 = 46.0;

/// Parsed `BulletSnap.ini` settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BulletSnapConfig {
    /// `[General] ImpactedPlayerSnap`: play a snap when a projectile hits the player
    /// [documented: Bullet Snap - ReRedux v1.2 shipped `BulletSnap.ini` comment
    /// "; Snap when hit player"].
    pub impacted_player_snap: bool,
    /// `[General] DistancedBasedSnap`: select snap sound bands using `[DistanceBased]`
    /// [documented: Bullet Snap - ReRedux v1.2 shipped `BulletSnap.ini` comment
    /// "; Will check distance between projectile and player when bullet pass by player
    /// when enable"].
    pub distanced_based_snap: bool,
    /// `[General] Subsonic`: enable subsonic pass-by audio below `subsonic_threshold_speed`
    /// [documented: Bullet Snap - ReRedux v1.2 shipped `BulletSnap.ini` comment
    /// "; enable subsonic bullet pass sound"].
    pub subsonic: bool,
    /// `[General] SubsonicThresholdSpeed`: projectile speed (game units/s) below which
    /// pass-by audio is treated as subsonic rather than supersonic [documented: Bullet
    /// Snap - ReRedux v1.2 shipped `BulletSnap.ini` comment "; Will do subsonic 'snap'
    /// when projectiles' speed lower than this value"].
    pub subsonic_threshold_speed: f32,
    /// `[General] SnapAngle`: angle (degrees, 0..=180) between projectile flight direction
    /// and the vector from projectile to the player camera; angles strictly greater than
    /// this trigger a pass-by snap (90° is perpendicular / closest approach) [documented:
    /// Bullet Snap - ReRedux v1.2 shipped `BulletSnap.ini` comments "; Check the angle
    /// between projectiles and player camera / ; Greater than this angle will do snap. /
    /// ; 90 Means that the projs' direction is perpendicular to the vector of proj to
    /// player's camera"].
    pub snap_angle_deg: f32,
    /// `[DistanceBased] VeryClose`: maximum distance (game units) for the very-close band
    /// [documented: Bullet Snap - ReRedux v1.2 shipped `BulletSnap.ini`].
    pub very_close: f32,
    /// `[DistanceBased] Close`: maximum distance (game units) for the close band
    /// [documented: Bullet Snap - ReRedux v1.2 shipped `BulletSnap.ini`].
    pub close: f32,
    /// `[DistanceBased] Distance`: maximum distance (game units) for the mid/distance band
    /// [documented: Bullet Snap - ReRedux v1.2 shipped `BulletSnap.ini`].
    pub distance: f32,
    /// `[DistanceBased] Far`: maximum distance (game units) for the far band
    /// [documented: Bullet Snap - ReRedux v1.2 shipped `BulletSnap.ini`].
    pub far: f32,
    /// `[DamageBased] middledmg`: key name and default `32` [documented: Bullet Snap -
    /// ReRedux v1.2 shipped `BulletSnap.ini`]; shipped INI has no comment stating what
    /// damage quantity is measured, so comparing against weapon `base_damage` to select
    /// the high-caliber supersonic audio tier is `[hypothesis]`.
    pub middle_dmg: f32,
    /// `[DamageBased] highdmg`: key name and default `46` [documented: Bullet Snap -
    /// ReRedux v1.2 shipped `BulletSnap.ini`]; shipped INI has no comment stating what
    /// damage quantity is measured, so comparing against weapon `base_damage` to select
    /// the very-high-caliber supersonic audio tier is `[hypothesis]`.
    pub high_dmg: f32,
}

impl Default for BulletSnapConfig {
    fn default() -> Self {
        Self {
            impacted_player_snap: DEFAULT_IMPACTED_PLAYER_SNAP,
            distanced_based_snap: DEFAULT_DISTANCED_BASED_SNAP,
            subsonic: DEFAULT_SUBSONIC,
            subsonic_threshold_speed: DEFAULT_SUBSONIC_THRESHOLD_SPEED,
            snap_angle_deg: DEFAULT_SNAP_ANGLE_DEG,
            very_close: DEFAULT_VERY_CLOSE,
            close: DEFAULT_CLOSE,
            distance: DEFAULT_DISTANCE,
            far: DEFAULT_FAR,
            middle_dmg: DEFAULT_MIDDLE_DMG,
            high_dmg: DEFAULT_HIGH_DMG,
        }
    }
}

/// The four distance bands of `[DistanceBased]` [documented: Bullet Snap - ReRedux v1.2
/// (`https://www.nexusmods.com/newvegas/mods/89474`), shipped
/// `Data\NVSE\Plugins\BulletSnapConfig\BulletSnap.ini`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SnapBand {
    VeryClose,
    Close,
    Distance,
    Far,
}

/// Sonic and damage-tier classification of a passing projectile [hypothesis].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SnapKind {
    Subsonic,
    General,
    HighCaliber,
    VeryHighCaliber,
}

/// The ten loose audio folders under `Data\Sound\fx\` [observed: Bullet Snap - ReRedux
/// v1.2 (`https://www.nexusmods.com/newvegas/mods/89474`) release archive directory
/// layout under `sound\fx\`]; mapping from `(SnapKind, SnapBand)` onto those folders is
/// `[hypothesis]` / `[design]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoundFolder {
    SubsonicClose,
    SubsonicMid,
    GeneralClose,
    GeneralMid,
    GeneralFar,
    HighCaliberClose,
    HighCaliberMid,
    VeryHighCaliberClose,
    VeryHighCaliberMid,
    VeryHighCaliberFar,
}

impl SoundFolder {
    pub const ALL: [SoundFolder; 10] = [
        SoundFolder::SubsonicClose,
        SoundFolder::SubsonicMid,
        SoundFolder::GeneralClose,
        SoundFolder::GeneralMid,
        SoundFolder::GeneralFar,
        SoundFolder::HighCaliberClose,
        SoundFolder::HighCaliberMid,
        SoundFolder::VeryHighCaliberClose,
        SoundFolder::VeryHighCaliberMid,
        SoundFolder::VeryHighCaliberFar,
    ];

    pub const fn index(self) -> usize {
        match self {
            SoundFolder::SubsonicClose => 0,
            SoundFolder::SubsonicMid => 1,
            SoundFolder::GeneralClose => 2,
            SoundFolder::GeneralMid => 3,
            SoundFolder::GeneralFar => 4,
            SoundFolder::HighCaliberClose => 5,
            SoundFolder::HighCaliberMid => 6,
            SoundFolder::VeryHighCaliberClose => 7,
            SoundFolder::VeryHighCaliberMid => 8,
            SoundFolder::VeryHighCaliberFar => 9,
        }
    }

    /// Normalized VFS folder path (lower-case, trailing `\`) under `Data\`.
    pub const fn vfs_folder(self) -> &'static str {
        match self {
            SoundFolder::SubsonicClose => "sound\\fx\\subsonic\\close\\",
            SoundFolder::SubsonicMid => "sound\\fx\\subsonic\\mid\\",
            SoundFolder::GeneralClose => "sound\\fx\\generalclosemid\\close\\",
            SoundFolder::GeneralMid => "sound\\fx\\generalclosemid\\mid\\",
            SoundFolder::GeneralFar => "sound\\fx\\generalfar\\",
            SoundFolder::HighCaliberClose => "sound\\fx\\hcclosemid\\close\\",
            SoundFolder::HighCaliberMid => "sound\\fx\\hcclosemid\\mid\\",
            SoundFolder::VeryHighCaliberClose => "sound\\fx\\veryhighcaliber\\close\\",
            SoundFolder::VeryHighCaliberMid => "sound\\fx\\veryhighcaliber\\mid\\",
            SoundFolder::VeryHighCaliberFar => "sound\\fx\\veryhighcaliber\\far\\",
        }
    }

    /// SOUN `FNAM`-style folder path relative to `sound\`.
    pub const fn sfx_fnam(self) -> &'static str {
        match self {
            SoundFolder::SubsonicClose => "fx\\subsonic\\close\\",
            SoundFolder::SubsonicMid => "fx\\subsonic\\mid\\",
            SoundFolder::GeneralClose => "fx\\generalclosemid\\close\\",
            SoundFolder::GeneralMid => "fx\\generalclosemid\\mid\\",
            SoundFolder::GeneralFar => "fx\\generalfar\\",
            SoundFolder::HighCaliberClose => "fx\\hcclosemid\\close\\",
            SoundFolder::HighCaliberMid => "fx\\hcclosemid\\mid\\",
            SoundFolder::VeryHighCaliberClose => "fx\\veryhighcaliber\\close\\",
            SoundFolder::VeryHighCaliberMid => "fx\\veryhighcaliber\\mid\\",
            SoundFolder::VeryHighCaliberFar => "fx\\veryhighcaliber\\far\\",
        }
    }

    /// Human-readable synthetic SOUN label for logs.
    pub const fn label(self) -> &'static str {
        match self {
            SoundFolder::SubsonicClose => "BulletSnapSubsonicClose",
            SoundFolder::SubsonicMid => "BulletSnapSubsonicMid",
            SoundFolder::GeneralClose => "BulletSnapGeneralClose",
            SoundFolder::GeneralMid => "BulletSnapGeneralMid",
            SoundFolder::GeneralFar => "BulletSnapGeneralFar",
            SoundFolder::HighCaliberClose => "BulletSnapHighCaliberClose",
            SoundFolder::HighCaliberMid => "BulletSnapHighCaliberMid",
            SoundFolder::VeryHighCaliberClose => "BulletSnapVeryHighCaliberClose",
            SoundFolder::VeryHighCaliberMid => "BulletSnapVeryHighCaliberMid",
            SoundFolder::VeryHighCaliberFar => "BulletSnapVeryHighCaliberFar",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    General,
    DistanceBased,
    DamageBased,
    Other,
}

fn trim_ascii(mut s: &[u8]) -> &[u8] {
    while let Some((&b, rest)) = s.split_first() {
        if matches!(b, b' ' | b'\t' | b'\r' | 0x0B | 0x0C) {
            s = rest;
        } else {
            break;
        }
    }
    while let Some((&b, rest)) = s.split_last() {
        if matches!(b, b' ' | b'\t' | b'\r' | 0x0B | 0x0C) {
            s = rest;
        } else {
            break;
        }
    }
    s
}

fn strip_comment(line: &[u8]) -> &[u8] {
    match line.iter().position(|&b| b == b';' || b == b'#') {
        Some(i) => &line[..i],
        None => line,
    }
}

fn parse_f32_bytes(raw: &[u8]) -> Option<f32> {
    let s = std::str::from_utf8(raw).ok()?;
    let v = s.parse::<f32>().ok()?;
    v.is_finite().then_some(v)
}

fn parse_bool_bytes(raw: &[u8]) -> Option<bool> {
    if raw.eq_ignore_ascii_case(b"1")
        || raw.eq_ignore_ascii_case(b"true")
        || raw.eq_ignore_ascii_case(b"yes")
        || raw.eq_ignore_ascii_case(b"on")
    {
        return Some(true);
    }
    if raw.eq_ignore_ascii_case(b"0")
        || raw.eq_ignore_ascii_case(b"false")
        || raw.eq_ignore_ascii_case(b"no")
        || raw.eq_ignore_ascii_case(b"off")
    {
        return Some(false);
    }
    parse_f32_bytes(raw).map(|v| v != 0.0)
}

impl BulletSnapConfig {
    /// Parse `BulletSnap.ini` from raw bytes.
    ///
    /// Strips an optional UTF-8 BOM and performs all section/key/comment splitting on
    /// `&[u8]` so non-ASCII or legacy Windows-1252 bytes in comments or unknown lines
    /// never panic or corrupt parsed settings [design].
    pub fn parse_bytes(bytes: &[u8]) -> BulletSnapConfig {
        let mut cfg = BulletSnapConfig::default();
        let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
        let mut section = Section::None;

        for raw_line in bytes.split(|&b| b == b'\n') {
            let line = trim_ascii(strip_comment(raw_line));
            if line.is_empty() {
                continue;
            }
            if line.first() == Some(&b'[') {
                if let Some(end) = line.iter().position(|&b| b == b']') {
                    let name = trim_ascii(&line[1..end]);
                    section = if name.eq_ignore_ascii_case(b"general") {
                        Section::General
                    } else if name.eq_ignore_ascii_case(b"distancebased") {
                        Section::DistanceBased
                    } else if name.eq_ignore_ascii_case(b"damagebased") {
                        Section::DamageBased
                    } else {
                        Section::Other
                    };
                }
                continue;
            }
            let Some(eq) = line.iter().position(|&b| b == b'=') else {
                continue;
            };
            let key = trim_ascii(&line[..eq]);
            let val = trim_ascii(&line[eq + 1..]);
            if key.is_empty() || val.is_empty() {
                continue;
            }
            match section {
                Section::General => {
                    if key.eq_ignore_ascii_case(b"impactedplayersnap") {
                        if let Some(b) = parse_bool_bytes(val) {
                            cfg.impacted_player_snap = b;
                        }
                    } else if key.eq_ignore_ascii_case(b"distancedbasedsnap") {
                        if let Some(b) = parse_bool_bytes(val) {
                            cfg.distanced_based_snap = b;
                        }
                    } else if key.eq_ignore_ascii_case(b"subsonic") {
                        if let Some(b) = parse_bool_bytes(val) {
                            cfg.subsonic = b;
                        }
                    } else if key.eq_ignore_ascii_case(b"subsonicthresholdspeed") {
                        if let Some(v) = parse_f32_bytes(val) {
                            cfg.subsonic_threshold_speed = v;
                        }
                    } else if key.eq_ignore_ascii_case(b"snapangle") {
                        if let Some(v) = parse_f32_bytes(val) {
                            cfg.snap_angle_deg = v;
                        }
                    }
                }
                Section::DistanceBased => {
                    if key.eq_ignore_ascii_case(b"veryclose") {
                        if let Some(v) = parse_f32_bytes(val) {
                            cfg.very_close = v;
                        }
                    } else if key.eq_ignore_ascii_case(b"close") {
                        if let Some(v) = parse_f32_bytes(val) {
                            cfg.close = v;
                        }
                    } else if key.eq_ignore_ascii_case(b"distance") {
                        if let Some(v) = parse_f32_bytes(val) {
                            cfg.distance = v;
                        }
                    } else if key.eq_ignore_ascii_case(b"far") {
                        if let Some(v) = parse_f32_bytes(val) {
                            cfg.far = v;
                        }
                    }
                }
                Section::DamageBased => {
                    if key.eq_ignore_ascii_case(b"middledmg") {
                        if let Some(v) = parse_f32_bytes(val) {
                            cfg.middle_dmg = v;
                        }
                    } else if key.eq_ignore_ascii_case(b"highdmg") {
                        if let Some(v) = parse_f32_bytes(val) {
                            cfg.high_dmg = v;
                        }
                    }
                }
                Section::None | Section::Other => {}
            }
        }
        cfg.sanitize()
    }

    /// Parse `BulletSnap.ini` from a UTF-8 string.
    pub fn parse_str(text: &str) -> BulletSnapConfig {
        Self::parse_bytes(text.as_bytes())
    }

    /// Clamp thresholds to finite, non-negative, monotonically ordered ranges [design].
    pub fn sanitize(mut self) -> BulletSnapConfig {
        self.subsonic_threshold_speed = if self.subsonic_threshold_speed.is_finite() {
            self.subsonic_threshold_speed.max(0.0)
        } else {
            DEFAULT_SUBSONIC_THRESHOLD_SPEED
        };
        self.snap_angle_deg = if self.snap_angle_deg.is_finite() {
            self.snap_angle_deg.clamp(0.0, 180.0)
        } else {
            DEFAULT_SNAP_ANGLE_DEG
        };
        self.very_close = if self.very_close.is_finite() { self.very_close.max(0.0) } else { DEFAULT_VERY_CLOSE };
        self.close = if self.close.is_finite() { self.close.max(self.very_close) } else { DEFAULT_CLOSE.max(self.very_close) };
        self.distance = if self.distance.is_finite() { self.distance.max(self.close) } else { DEFAULT_DISTANCE.max(self.close) };
        self.far = if self.far.is_finite() { self.far.max(self.distance) } else { DEFAULT_FAR.max(self.distance) };
        self.middle_dmg = if self.middle_dmg.is_finite() { self.middle_dmg.max(0.0) } else { DEFAULT_MIDDLE_DMG };
        self.high_dmg = if self.high_dmg.is_finite() { self.high_dmg.max(self.middle_dmg) } else { DEFAULT_HIGH_DMG.max(self.middle_dmg) };
        self
    }

    /// Select the distance band for a pass-by at `distance` game units from the listener.
    ///
    /// When `distanced_based_snap` is enabled, uses the four `[DistanceBased]` thresholds
    /// (`VeryClose`, `Close`, `Distance`, `Far`) and returns `None` beyond `Far`.
    /// When `distanced_based_snap` is disabled, ignores `[DistanceBased]` and accepts any
    /// pass-by within [`DEFAULT_FAR`] as [`SnapBand::Close`] [design].
    pub fn band_for_distance(&self, distance: f32) -> Option<SnapBand> {
        if !distance.is_finite() || distance < 0.0 {
            return None;
        }
        if !self.distanced_based_snap {
            return (distance <= DEFAULT_FAR).then_some(SnapBand::Close);
        }
        if distance <= self.very_close {
            Some(SnapBand::VeryClose)
        } else if distance <= self.close {
            Some(SnapBand::Close)
        } else if distance <= self.distance {
            Some(SnapBand::Distance)
        } else if distance <= self.far {
            Some(SnapBand::Far)
        } else {
            None
        }
    }

    /// Band-relative gain multiplier applied on top of 3D attenuation [design].
    pub fn gain_for_band(&self, band: SnapBand) -> f32 {
        if !self.distanced_based_snap {
            return 1.0;
        }
        match band {
            SnapBand::VeryClose => 1.0,
            SnapBand::Close => 0.85,
            SnapBand::Distance => 0.65,
            SnapBand::Far => 0.45,
        }
    }

    /// Classify a projectile into subsonic vs supersonic damage tiers.
    ///
    /// Returns `None` for non-positive/non-finite speed or when the projectile is below
    /// `subsonic_threshold_speed` and `subsonic` is disabled.
    pub fn classify(&self, speed: f32, damage: f32) -> Option<SnapKind> {
        if !speed.is_finite() || speed <= 0.0 {
            return None;
        }
        if speed < self.subsonic_threshold_speed {
            return self.subsonic.then_some(SnapKind::Subsonic);
        }
        let dmg = if damage.is_finite() { damage.max(0.0) } else { 0.0 };
        if dmg >= self.high_dmg {
            Some(SnapKind::VeryHighCaliber)
        } else if dmg >= self.middle_dmg {
            Some(SnapKind::HighCaliber)
        } else {
            Some(SnapKind::General)
        }
    }

    /// Maximum audible reach used when registering synthetic BulletSnap sound records [design].
    pub fn max_reach(&self) -> f32 {
        let base = if self.distanced_based_snap { self.far } else { DEFAULT_FAR };
        (base.max(DEFAULT_FAR) * 1.25).max(2_000.0)
    }
}

/// Ordered folder preference list for a `(SnapKind, SnapBand)` pair [hypothesis/design].
pub fn sound_folders(kind: SnapKind, band: SnapBand) -> &'static [SoundFolder] {
    match (kind, band) {
        (SnapKind::Subsonic, SnapBand::VeryClose | SnapBand::Close) => &[SoundFolder::SubsonicClose, SoundFolder::SubsonicMid],
        (SnapKind::Subsonic, SnapBand::Distance | SnapBand::Far) => &[SoundFolder::SubsonicMid, SoundFolder::SubsonicClose],
        (SnapKind::General, SnapBand::VeryClose | SnapBand::Close) => &[SoundFolder::GeneralClose, SoundFolder::GeneralMid, SoundFolder::GeneralFar],
        (SnapKind::General, SnapBand::Distance) => &[SoundFolder::GeneralMid, SoundFolder::GeneralClose, SoundFolder::GeneralFar],
        (SnapKind::General, SnapBand::Far) => &[SoundFolder::GeneralFar, SoundFolder::GeneralMid, SoundFolder::GeneralClose],
        (SnapKind::HighCaliber, SnapBand::VeryClose | SnapBand::Close) => &[SoundFolder::HighCaliberClose, SoundFolder::HighCaliberMid, SoundFolder::GeneralClose],
        (SnapKind::HighCaliber, SnapBand::Distance) => &[SoundFolder::HighCaliberMid, SoundFolder::HighCaliberClose, SoundFolder::GeneralMid],
        (SnapKind::HighCaliber, SnapBand::Far) => &[SoundFolder::GeneralFar, SoundFolder::HighCaliberMid, SoundFolder::GeneralMid],
        (SnapKind::VeryHighCaliber, SnapBand::VeryClose | SnapBand::Close) => &[SoundFolder::VeryHighCaliberClose, SoundFolder::VeryHighCaliberMid, SoundFolder::HighCaliberClose, SoundFolder::GeneralClose],
        (SnapKind::VeryHighCaliber, SnapBand::Distance) => &[SoundFolder::VeryHighCaliberMid, SoundFolder::VeryHighCaliberClose, SoundFolder::HighCaliberMid, SoundFolder::GeneralMid],
        (SnapKind::VeryHighCaliber, SnapBand::Far) => &[SoundFolder::VeryHighCaliberFar, SoundFolder::GeneralFar, SoundFolder::VeryHighCaliberMid],
    }
}

/// Choose the first available folder for `(kind, band)` given a predicate `has_folder`.
pub fn pick_folder(kind: SnapKind, band: SnapBand, has_folder: impl Fn(SoundFolder) -> bool) -> Option<SoundFolder> {
    sound_folders(kind, band).iter().copied().find(|&f| has_folder(f))
}

/// Closest point on the line segment `a` → `b` to `p`.
pub fn closest_on_segment(a: V3, b: V3, p: V3) -> V3 {
    let ab = vec::sub(b, a);
    let len2 = vec::dot(ab, ab);
    if !len2.is_finite() || len2 < 1e-6 {
        return a;
    }
    let t = (vec::dot(vec::sub(p, a), ab) / len2).clamp(0.0, 1.0);
    vec::add(a, vec::scale(ab, t))
}

/// Angle in degrees (0..=180) between projectile direction `dir` and the vector from
/// `pos` to `eye`. 0° means flying directly toward `eye`; 90° means perpendicular;
/// 180° means flying directly away from `eye`.
pub fn angle_to_eye_deg(pos: V3, dir: V3, eye: V3) -> Option<f32> {
    let d_len = vec::len(dir);
    let to_eye = vec::sub(eye, pos);
    let e_len = vec::len(to_eye);
    if !d_len.is_finite() || d_len < 1e-4 || !e_len.is_finite() || e_len < 1e-4 {
        return None;
    }
    let cos = (vec::dot(dir, to_eye) / (d_len * e_len)).clamp(-1.0, 1.0);
    Some(cos.acos().to_degrees())
}

/// A pass-by (or player-impact) snap event ready to play.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapTrigger {
    pub id: u64,
    pub at: V3,
    pub distance: f32,
    pub kind: SnapKind,
    pub band: SnapBand,
    pub gain: f32,
    pub on_player_impact: bool,
}

#[derive(Debug, Clone, Copy)]
struct TrackState {
    approached: bool,
    snapped: bool,
}

/// Per-projectile pass-by tracker ensuring each projectile snaps at most once.
#[derive(Debug, Default)]
pub struct SnapTracker {
    states: HashMap<u64, TrackState>,
}

impl SnapTracker {
    pub fn clear(&mut self) {
        self.states.clear();
    }

    pub fn tracked_len(&self) -> usize {
        self.states.len()
    }

    /// Drop state for projectiles that are no longer in flight.
    pub fn retain_active(&mut self, active: impl Fn(u64) -> bool) {
        self.states.retain(|&id, _| active(id));
    }

    /// Observe one swept segment `from` → `to` of projectile `id`.
    ///
    /// Fires at most once per `id`: the projectile must not be player-owned, must be
    /// eligible (ballistic/bullet projectile), must have approached the listener
    /// (`dot(dir, eye - pos) > 0`), and its angle at `to` must exceed `config.snap_angle_deg`.
    #[allow(clippy::too_many_arguments)]
    pub fn observe_segment(
        &mut self,
        id: u64,
        from: V3,
        to: V3,
        dir: V3,
        speed: f32,
        damage: f32,
        is_player_owned: bool,
        eligible: bool,
        eye: V3,
        config: &BulletSnapConfig,
    ) -> Option<SnapTrigger> {
        if is_player_owned || !eligible {
            return None;
        }
        let seg = vec::sub(to, from);
        let seg_len = vec::len(seg);
        let flight_dir = if seg_len.is_finite() && seg_len > 1e-4 {
            vec::scale(seg, 1.0 / seg_len)
        } else {
            let dl = vec::len(dir);
            if !dl.is_finite() || dl < 1e-4 {
                return None;
            }
            vec::scale(dir, 1.0 / dl)
        };
        let state = self.states.entry(id).or_insert(TrackState { approached: false, snapped: false });
        if state.snapped {
            return None;
        }
        if vec::dot(flight_dir, vec::sub(eye, from)) > 0.0 {
            state.approached = true;
        }
        if !state.approached {
            return None;
        }
        let angle_end = angle_to_eye_deg(to, flight_dir, eye).unwrap_or(90.0);
        if angle_end <= config.snap_angle_deg {
            return None;
        }
        let at = closest_on_segment(from, to, eye);
        let distance = vec::len(vec::sub(at, eye));
        let Some(band) = config.band_for_distance(distance) else {
            // Once past perpendicular (90°), the projectile is only drawing further away.
            if angle_end >= 90.0 {
                state.snapped = true;
            }
            return None;
        };
        state.snapped = true;
        let kind = config.classify(speed, damage)?;
        Some(SnapTrigger {
            id,
            at,
            distance,
            kind,
            band,
            gain: config.gain_for_band(band),
            on_player_impact: false,
        })
    }

    /// Observe projectile `id` impacting the player at `at`.
    ///
    /// Always marks `id` as finished so a player-impacting round cannot also produce a
    /// stray pass-by snap when `impacted_player_snap` is `false`.
    #[allow(clippy::too_many_arguments)]
    pub fn observe_player_impact(
        &mut self,
        id: u64,
        at: V3,
        speed: f32,
        damage: f32,
        is_player_owned: bool,
        eligible: bool,
        eye: V3,
        config: &BulletSnapConfig,
    ) -> Option<SnapTrigger> {
        let state = self.states.entry(id).or_insert(TrackState { approached: true, snapped: false });
        if state.snapped {
            return None;
        }
        state.snapped = true;
        if !config.impacted_player_snap || is_player_owned || !eligible {
            return None;
        }
        let distance = vec::len(vec::sub(at, eye));
        let band = config.band_for_distance(distance)?;
        let kind = config.classify(speed, damage)?;
        Some(SnapTrigger {
            id,
            at,
            distance,
            kind,
            band,
            gain: config.gain_for_band(band),
            on_player_impact: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_public_ini_schema() {
        let cfg = BulletSnapConfig::default();
        assert!(!cfg.impacted_player_snap);
        assert!(cfg.distanced_based_snap);
        assert!(cfg.subsonic);
        assert_eq!(cfg.subsonic_threshold_speed, 21_000.0);
        assert_eq!(cfg.snap_angle_deg, 75.0);
        assert_eq!(cfg.very_close, 768.0);
        assert_eq!(cfg.close, 1_536.0);
        assert_eq!(cfg.distance, 4_092.0);
        assert_eq!(cfg.far, 8_192.0);
        assert_eq!(cfg.middle_dmg, 32.0);
        assert_eq!(cfg.high_dmg, 46.0);
    }

    #[test]
    fn parses_bom_crlf_comments_case_insensitive_and_clamps() {
        let mut raw = vec![0xEF, 0xBB, 0xBF];
        raw.extend_from_slice(
            b"[general]\r\n\
              impactedplayersnap = 1 ; inline comment\r\n\
              DISTANCEDBASEDSNAP = 0 # hash comment\r\n\
              subsonic = false\r\n\
              SubsonicThresholdSpeed = 18500.5\r\n\
              SnapAngle = 220\r\n\
              [distancebased]\r\n\
              VeryClose = 900\r\n\
              Close = 400\r\n\
              Distance = 2000\r\n\
              Far = 1000\r\n\
              [DAMAGEBASED]\r\n\
              MiddleDmg = 40\r\n\
              HighDmg = 20\r\n",
        );
        let cfg = BulletSnapConfig::parse_bytes(&raw);
        assert!(cfg.impacted_player_snap);
        assert!(!cfg.distanced_based_snap);
        assert!(!cfg.subsonic);
        assert_eq!(cfg.subsonic_threshold_speed, 18_500.5);
        assert_eq!(cfg.snap_angle_deg, 180.0);
        assert_eq!(cfg.very_close, 900.0);
        assert_eq!(cfg.close, 900.0, "Close clamped to >= VeryClose");
        assert_eq!(cfg.distance, 2_000.0);
        assert_eq!(cfg.far, 2_000.0, "Far clamped to >= Distance");
        assert_eq!(cfg.middle_dmg, 40.0);
        assert_eq!(cfg.high_dmg, 40.0, "highdmg clamped to >= middledmg");
    }

    #[test]
    fn parses_non_ascii_and_invalid_utf8_bytes_without_panicking() {
        // Mix valid multi-byte UTF-8 (curly quotes, CJK, accented letters) and raw
        // invalid UTF-8 / Windows-1252 bytes (0x91, 0x92, 0xFF, 0x80) in comments,
        // section names, unknown keys and malformed values.
        let mut raw = Vec::new();
        raw.extend_from_slice(b"; \xE2\x80\x98snap\xE2\x80\x99 \x91legacy\x92 \xFF\x80\n");
        raw.extend_from_slice("[Ünknownセクション]\n".as_bytes());
        raw.extend_from_slice(b"SnapAngle = 10\n");
        raw.extend_from_slice(b"[General]\n");
        raw.extend_from_slice(b"\xFFbadkey = 123\n");
        raw.extend_from_slice(b"SnapAngle = 82.5 ; \x91angle\x92 \xE6\x97\xA5\xE6\x9C\xAC\n");
        raw.extend_from_slice(b"SubsonicThresholdSpeed = NaN\n");
        raw.extend_from_slice(b"ImpactedPlayerSnap = \xFF\n");
        raw.extend_from_slice(b"[DistanceBased]\n");
        raw.extend_from_slice(b"VeryClose = 512 ; \x80\x81\n");
        raw.extend_from_slice(b"Far = inf\n");

        let cfg = BulletSnapConfig::parse_bytes(&raw);
        assert_eq!(cfg.snap_angle_deg, 82.5);
        assert_eq!(cfg.subsonic_threshold_speed, DEFAULT_SUBSONIC_THRESHOLD_SPEED);
        assert!(!cfg.impacted_player_snap);
        assert_eq!(cfg.very_close, 512.0);
        assert_eq!(cfg.far, DEFAULT_FAR);
    }

    #[test]
    fn classifies_subsonic_and_supersonic_damage_tiers() {
        let mut cfg = BulletSnapConfig::default();
        assert_eq!(cfg.classify(20_999.0, 60.0), Some(SnapKind::Subsonic));
        assert_eq!(cfg.classify(21_000.0, 20.0), Some(SnapKind::General));
        assert_eq!(cfg.classify(25_000.0, 31.9), Some(SnapKind::General));
        assert_eq!(cfg.classify(25_000.0, 32.0), Some(SnapKind::HighCaliber));
        assert_eq!(cfg.classify(25_000.0, 45.9), Some(SnapKind::HighCaliber));
        assert_eq!(cfg.classify(25_000.0, 46.0), Some(SnapKind::VeryHighCaliber));
        assert_eq!(cfg.classify(0.0, 50.0), None);
        assert_eq!(cfg.classify(f32::NAN, 50.0), None);

        cfg.subsonic = false;
        assert_eq!(cfg.classify(15_000.0, 50.0), None);
        assert_eq!(cfg.classify(21_000.0, 50.0), Some(SnapKind::VeryHighCaliber));
    }

    #[test]
    fn distance_bands_respect_thresholds_and_toggle() {
        let mut cfg = BulletSnapConfig::default();
        assert_eq!(cfg.band_for_distance(0.0), Some(SnapBand::VeryClose));
        assert_eq!(cfg.band_for_distance(768.0), Some(SnapBand::VeryClose));
        assert_eq!(cfg.band_for_distance(768.1), Some(SnapBand::Close));
        assert_eq!(cfg.band_for_distance(1_536.0), Some(SnapBand::Close));
        assert_eq!(cfg.band_for_distance(1_536.1), Some(SnapBand::Distance));
        assert_eq!(cfg.band_for_distance(4_092.0), Some(SnapBand::Distance));
        assert_eq!(cfg.band_for_distance(4_092.1), Some(SnapBand::Far));
        assert_eq!(cfg.band_for_distance(8_192.0), Some(SnapBand::Far));
        assert_eq!(cfg.band_for_distance(8_192.1), None);
        assert_eq!(cfg.band_for_distance(-1.0), None);
        assert_eq!(cfg.band_for_distance(f32::NAN), None);

        // Monotonically decreasing band gains when DistancedBasedSnap = 1.
        assert!(cfg.gain_for_band(SnapBand::VeryClose) > cfg.gain_for_band(SnapBand::Close));
        assert!(cfg.gain_for_band(SnapBand::Close) > cfg.gain_for_band(SnapBand::Distance));
        assert!(cfg.gain_for_band(SnapBand::Distance) > cfg.gain_for_band(SnapBand::Far));

        // When DistancedBasedSnap = 0, [DistanceBased] cutoffs are ignored.
        cfg.distanced_based_snap = false;
        cfg.far = 100.0;
        assert_eq!(cfg.band_for_distance(500.0), Some(SnapBand::Close));
        assert_eq!(cfg.gain_for_band(SnapBand::Close), 1.0);
        assert_eq!(cfg.band_for_distance(DEFAULT_FAR + 1.0), None);
    }

    #[test]
    fn folder_selection_and_fallbacks_cover_all_tiers() {
        let all = |_| true;
        assert_eq!(pick_folder(SnapKind::Subsonic, SnapBand::VeryClose, all), Some(SoundFolder::SubsonicClose));
        assert_eq!(pick_folder(SnapKind::Subsonic, SnapBand::Distance, all), Some(SoundFolder::SubsonicMid));
        assert_eq!(pick_folder(SnapKind::General, SnapBand::VeryClose, all), Some(SoundFolder::GeneralClose));
        assert_eq!(pick_folder(SnapKind::General, SnapBand::Distance, all), Some(SoundFolder::GeneralMid));
        assert_eq!(pick_folder(SnapKind::General, SnapBand::Far, all), Some(SoundFolder::GeneralFar));
        assert_eq!(pick_folder(SnapKind::HighCaliber, SnapBand::Close, all), Some(SoundFolder::HighCaliberClose));
        assert_eq!(pick_folder(SnapKind::HighCaliber, SnapBand::Distance, all), Some(SoundFolder::HighCaliberMid));
        assert_eq!(pick_folder(SnapKind::HighCaliber, SnapBand::Far, all), Some(SoundFolder::GeneralFar));
        assert_eq!(pick_folder(SnapKind::VeryHighCaliber, SnapBand::VeryClose, all), Some(SoundFolder::VeryHighCaliberClose));
        assert_eq!(pick_folder(SnapKind::VeryHighCaliber, SnapBand::Distance, all), Some(SoundFolder::VeryHighCaliberMid));
        assert_eq!(pick_folder(SnapKind::VeryHighCaliber, SnapBand::Far, all), Some(SoundFolder::VeryHighCaliberFar));

        // Missing primary folder falls back gracefully.
        let no_vhc_far = |f| f != SoundFolder::VeryHighCaliberFar;
        assert_eq!(pick_folder(SnapKind::VeryHighCaliber, SnapBand::Far, no_vhc_far), Some(SoundFolder::GeneralFar));
        assert_eq!(pick_folder(SnapKind::General, SnapBand::VeryClose, |_| false), None);
    }

    #[test]
    fn snap_angle_cone_single_fire_and_player_exclusion() {
        let cfg = BulletSnapConfig::default(); // SnapAngle = 75°
        let eye = [0.0, 0.0, 0.0];
        let mut tracker = SnapTracker::default();

        // Bullet flying along +X at y = 200 (VeryClose).
        // At x = -1000 -> -200: angle at -200 is atan2(200, 200) = 45° <= 75° -> no snap yet.
        assert!(tracker.observe_segment(1, [-1000.0, 200.0, 0.0], [-200.0, 200.0, 0.0], [1.0, 0.0, 0.0], 25_000.0, 35.0, false, true, eye, &cfg).is_none());

        // At x = -200 -> -20: angle at -20 is atan2(200, 20) = 84.3° > 75° -> SNAP fires once!
        let trig = tracker
            .observe_segment(1, [-200.0, 200.0, 0.0], [-20.0, 200.0, 0.0], [1.0, 0.0, 0.0], 25_000.0, 35.0, false, true, eye, &cfg)
            .expect("should snap once angle exceeds SnapAngle");
        assert_eq!(trig.kind, SnapKind::HighCaliber);
        assert_eq!(trig.band, SnapBand::VeryClose);
        assert!(!trig.on_player_impact);

        // Subsequent segments of the same projectile must NOT snap again.
        assert!(tracker.observe_segment(1, [-20.0, 200.0, 0.0], [400.0, 200.0, 0.0], [1.0, 0.0, 0.0], 25_000.0, 35.0, false, true, eye, &cfg).is_none());

        // Fast single-frame pass-by (from x = -500 straight through to x = +500 in one step):
        // closest point is exact (0, 2000, 0) -> Distance band (1536..4092).
        let fast = tracker
            .observe_segment(2, [-500.0, 2000.0, 0.0], [500.0, 2000.0, 0.0], [1.0, 0.0, 0.0], 18_000.0, 50.0, false, true, eye, &cfg)
            .expect("single-step fast pass-by should snap at closest approach");
        assert_eq!(fast.at, [0.0, 2000.0, 0.0]);
        assert!((fast.distance - 2000.0).abs() < 1e-3);
        assert_eq!(fast.kind, SnapKind::Subsonic);
        assert_eq!(fast.band, SnapBand::Distance);

        // Bullet fired AWAY from player never approached -> never snaps.
        assert!(tracker.observe_segment(3, [100.0, 100.0, 0.0], [600.0, 100.0, 0.0], [1.0, 0.0, 0.0], 25_000.0, 50.0, false, true, eye, &cfg).is_none());

        // Player-owned bullet never snaps.
        assert!(tracker.observe_segment(4, [-500.0, 100.0, 0.0], [500.0, 100.0, 0.0], [1.0, 0.0, 0.0], 25_000.0, 50.0, true, true, eye, &cfg).is_none());

        // Ineligible projectile (beam/flame/lobber) never snaps.
        assert!(tracker.observe_segment(5, [-500.0, 100.0, 0.0], [500.0, 100.0, 0.0], [1.0, 0.0, 0.0], 25_000.0, 50.0, false, false, eye, &cfg).is_none());

        // Pruning drops inactive IDs.
        tracker.retain_active(|id| id == 2);
        assert_eq!(tracker.tracked_len(), 1);
    }

    #[test]
    fn impacted_player_snap_respects_toggle() {
        let eye = [0.0, 0.0, 0.0];
        let mut cfg = BulletSnapConfig::default(); // ImpactedPlayerSnap = 0
        let mut tracker = SnapTracker::default();
        assert!(tracker.observe_player_impact(10, [10.0, 0.0, 0.0], 25_000.0, 50.0, false, true, eye, &cfg).is_none());
        // Already marked snapped: no follow-up pass-by snap either.
        assert!(tracker.observe_segment(10, [-100.0, 10.0, 0.0], [100.0, 10.0, 0.0], [1.0, 0.0, 0.0], 25_000.0, 50.0, false, true, eye, &cfg).is_none());

        cfg.impacted_player_snap = true;
        let trig = tracker
            .observe_player_impact(11, [10.0, 0.0, 0.0], 25_000.0, 50.0, false, true, eye, &cfg)
            .expect("ImpactedPlayerSnap=1 should fire on player impact");
        assert!(trig.on_player_impact);
        assert_eq!(trig.kind, SnapKind::VeryHighCaliber);
        assert_eq!(trig.band, SnapBand::VeryClose);
        // Second impact call for same id is ignored.
        assert!(tracker.observe_player_impact(11, [10.0, 0.0, 0.0], 25_000.0, 50.0, false, true, eye, &cfg).is_none());
    }
}
