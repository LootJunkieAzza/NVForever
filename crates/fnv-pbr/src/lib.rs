//! Authored PBR material maps (#2981): NVForever's own file format under `textures\pbr\`
//! (docs/spec-notes/pbr-materials.md). Pure logic over paths and bytes a caller hands in;
//! nothing here opens a file.
//!
//! - [`stem_for`] maps a game diffuse's path to its set's stem, [`member_path`] a stem
//!   to a member's file;
//! - [`classify`] reads a file found under `textures\pbr\` back to (stem, member), taking
//!   exactly one suffix off its name;
//! - [`PbrIndex`] gathers the sets a load order supplies from its list of paths, answers
//!   [`PbrIndex::lookup`] by resolved diffuse path (with the first-person fallback) and
//!   counts what it found for a log line;
//! - [`parse_scalars`] reads a set's optional `.toml` file of scalars.
//!
//! [design] Everything about the layout is NVForever's own choice; it describes no
//! behaviour of the original game.

mod scalars;

pub use scalars::{parse_scalars, ScalarFile, Scalars, DEFAULT_METAL_GAIN, DEFAULT_METAL_GAIN_WITH_ALBEDO, DEFAULT_REFLECTANCE, DEFAULT_ROUGHNESS_SCALE, METAL_GAIN_RANGE, PARALLAX_DEPTH_RANGE, REFLECTANCE_RANGE, ROUGHNESS_SCALE_RANGE};

use std::collections::{BTreeMap, BTreeSet};

/// The folder every authored file lives under, as a normalised VFS prefix.
pub const PBR_DIR: &str = "textures\\pbr\\";

const TEXTURES_DIR: &str = "textures\\";

/// The file-name prefix of the first-person twin of a texture (nvf-assets `texture.rs`,
/// `is_first_person_texture`).
const FIRST_PERSON: &str = "1stperson";

/// One file of a set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Member {
    /// `<stem>_orm.dds`: red occlusion, green perceptual roughness, blue metallic (linear).
    Orm,
    /// `<stem>_n.dds`: two-channel tangent-space normal, green down like the game's (linear).
    Normal,
    /// `<stem>_p.dds`: height, brighter is higher (linear).
    Height,
    /// `<stem>.toml`: optional scalars ([`parse_scalars`]).
    Scalars,
    /// `<stem>_albedo.dds`: optional base colour (sRGB colour, its alpha the material's
    /// alpha), drawn in place of the game's diffuse by an authored set; never activates a set
    /// on its own (`_orm` stays the required member).
    Albedo,
}

impl Member {
    pub const ALL: [Member; 5] = [Member::Orm, Member::Normal, Member::Height, Member::Scalars, Member::Albedo];

    /// The text after the stem in the member's file name.
    pub fn suffix(self) -> &'static str {
        match self {
            Member::Orm => "_orm.dds",
            Member::Normal => "_n.dds",
            Member::Height => "_p.dds",
            Member::Scalars => ".toml",
            Member::Albedo => "_albedo.dds",
        }
    }

    /// A short name for listings and logs.
    pub fn name(self) -> &'static str {
        match self {
            Member::Orm => "orm",
            Member::Normal => "normal",
            Member::Height => "height",
            Member::Scalars => "scalars",
            Member::Albedo => "albedo",
        }
    }

    fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// The stem of the set for a diffuse texture: the diffuse's path in the form the engine
/// looks textures up by (`fnv_vfs::texture_path`: normalised, a leading `data\` or an
/// absolute prefix cut at `textures\`), its extension taken off whatever it is, placed
/// under `textures\pbr\`. `textures\architecture\urban\sidewalkquad01.dds` →
/// `textures\pbr\architecture\urban\sidewalkquad01`. `None` when the path has no file
/// name left once the extension is off.
///
/// [design] The extension is not kept, so `foo.dds` and `foo.tga` in one folder share a
/// set (docs/spec-notes/pbr-materials.md).
pub fn stem_for(diffuse: &str) -> Option<String> {
    let path = fnv_vfs::texture_path(diffuse);
    let rest = path.strip_prefix(TEXTURES_DIR)?;
    let file_start = rest.rfind('\\').map_or(0, |i| i + 1);
    let name = &rest[file_start..];
    let base = name.rfind('.').map_or(name, |i| &name[..i]);
    if base.is_empty() {
        return None;
    }
    Some(format!("{PBR_DIR}{}{base}", &rest[..file_start]))
}

/// The path of `member`'s file in the set `stem` ([`stem_for`]).
pub fn member_path(stem: &str, member: Member) -> String {
    format!("{stem}{}", member.suffix())
}

/// The diffuse a stem belongs to, without its extension: the stem with `pbr\` taken out
/// (`textures\pbr\a\b` → `textures\a\b`). `None` for a path not under `textures\pbr\`.
pub fn diffuse_stem(stem: &str) -> Option<String> {
    let key = fnv_vfs::normalize(stem);
    key.strip_prefix(PBR_DIR).map(|rest| format!("{TEXTURES_DIR}{rest}"))
}

/// The set and member a file under `textures\pbr\` belongs to: exactly one terminal
/// suffix ([`Member::suffix`]) is taken off the final path component, and what is left of
/// the file name must not be empty. Never peels twice: `foo_n_orm.dds` is the ORM of the
/// diffuse `foo_n`, `foo_n.dds` the normal of `foo`. `None` for an unmatched file (a bare
/// `foo.dds`, another extension, an empty name) or a path outside `textures\pbr\`.
pub fn classify(path: &str) -> Option<(String, Member)> {
    let key = fnv_vfs::normalize(path);
    let rest = key.strip_prefix(PBR_DIR)?;
    // At most one suffix can match: they differ in the characters before `.dds`.
    let (stem_rest, member) = Member::ALL.iter().find_map(|&m| rest.strip_suffix(m.suffix()).map(|s| (s, m)))?;
    let name = &stem_rest[stem_rest.rfind('\\').map_or(0, |i| i + 1)..];
    if name.is_empty() {
        return None;
    }
    Some((format!("{PBR_DIR}{stem_rest}"), member))
}

/// What a set does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetState {
    /// It has `_orm`: the authored material applies (with `_n`, `_p`, `_albedo` and scalars if
    /// present).
    Authored,
    /// `_p` without `_orm`: the game's own material with height only.
    HeightOnly,
    /// Neither `_orm` nor `_p` (only `_n`, `.toml` or `_albedo`): activates nothing.
    Incomplete,
}

impl SetState {
    /// Whether the set changes a material at all.
    pub fn is_active(self) -> bool {
        self != SetState::Incomplete
    }

    pub fn name(self) -> &'static str {
        match self {
            SetState::Authored => "authored",
            SetState::HeightOnly => "height-only",
            SetState::Incomplete => "incomplete",
        }
    }
}

/// The members a set has. Presence only: the files themselves (their sizes, formats) are
/// read by whoever uses them, and each member's size is its own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SetFiles {
    bits: u8,
}

impl SetFiles {
    pub fn has(&self, m: Member) -> bool {
        self.bits & m.bit() != 0
    }

    /// The members present, in [`Member::ALL`] order.
    pub fn members(&self) -> impl Iterator<Item = Member> + '_ {
        Member::ALL.into_iter().filter(|m| self.has(*m))
    }

    pub fn state(&self) -> SetState {
        if self.has(Member::Orm) {
            SetState::Authored
        } else if self.has(Member::Height) {
            SetState::HeightOnly
        } else {
            SetState::Incomplete
        }
    }

    fn insert(&mut self, m: Member) -> bool {
        let new = !self.has(m);
        self.bits |= m.bit();
        new
    }
}

/// What the index found, for a log line and the listing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// Every distinct file under `textures\pbr\`.
    pub files: usize,
    /// Distinct stems with at least one member.
    pub sets: usize,
    /// Sets with `_orm`.
    pub authored: usize,
    /// Sets with `_p` and no `_orm`.
    pub height_only: usize,
    /// Sets with neither (activate nothing).
    pub incomplete: usize,
    pub orm: usize,
    pub normal: usize,
    pub height: usize,
    pub scalars: usize,
    /// `_albedo` files (drawn by authored sets only).
    pub albedo: usize,
    /// Files under `textures\pbr\` that are no member of any set.
    pub unmatched: usize,
}

impl Counts {
    /// Sets that change a material: authored plus height only.
    pub fn active(&self) -> usize {
        self.authored + self.height_only
    }

    fn member(&mut self, m: Member) -> &mut usize {
        match m {
            Member::Orm => &mut self.orm,
            Member::Normal => &mut self.normal,
            Member::Height => &mut self.height,
            Member::Scalars => &mut self.scalars,
            Member::Albedo => &mut self.albedo,
        }
    }
}

/// A set [`PbrIndex::lookup`] found for a diffuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The set's stem (the first-person twin's or the plain texture's).
    pub stem: String,
    pub files: SetFiles,
    /// The diffuse is a `1stperson` texture without a set of its own, and this is the set of
    /// the same name without the prefix.
    pub first_person_fallback: bool,
}

impl Found {
    /// The path of `m`'s file when the set has it.
    pub fn path(&self, m: Member) -> Option<String> {
        self.files.has(m).then(|| member_path(&self.stem, m))
    }

    pub fn state(&self) -> SetState {
        self.files.state()
    }
}

/// The sets a load order supplies, built once from its list of (normalised) VFS paths.
#[derive(Debug, Clone, Default)]
pub struct PbrIndex {
    sets: BTreeMap<String, SetFiles>,
    unmatched: Vec<String>,
    counts: Counts,
}

impl PbrIndex {
    /// The index of every path under `textures\pbr\` in `paths` (others are skipped, so a
    /// caller may hand in the whole namespace). Paths are normalised here; one given twice
    /// counts once. Each member resolves on its own through the VFS before it gets here, so
    /// a set may mix files of several packs.
    pub fn build<I, S>(paths: I) -> PbrIndex
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let keys: BTreeSet<String> = paths.into_iter().map(|p| fnv_vfs::normalize(p.as_ref())).filter(|k| k.starts_with(PBR_DIR)).collect();
        let mut index = PbrIndex::default();
        for key in keys {
            index.counts.files += 1;
            match classify(&key) {
                Some((stem, m)) => {
                    if index.sets.entry(stem).or_default().insert(m) {
                        *index.counts.member(m) += 1;
                    }
                }
                None => index.unmatched.push(key),
            }
        }
        index.counts.unmatched = index.unmatched.len();
        index.counts.sets = index.sets.len();
        for files in index.sets.values() {
            match files.state() {
                SetState::Authored => index.counts.authored += 1,
                SetState::HeightOnly => index.counts.height_only += 1,
                SetState::Incomplete => index.counts.incomplete += 1,
            }
        }
        index
    }

    /// The members of the set `stem`, whatever its state.
    pub fn get(&self, stem: &str) -> Option<SetFiles> {
        self.sets.get(&fnv_vfs::normalize(stem)).copied()
    }

    /// The active set ([`SetState::is_active`]) for a diffuse, by its resolved path (after
    /// nvf-assets' `resolved_path`: normalised, `\xbox\` → `\pc\` where the PC file exists).
    /// When the diffuse's file name starts with `1stperson` and no active set exists for
    /// it, the set of the name without that prefix is tried.
    pub fn lookup(&self, diffuse: &str) -> Option<Found> {
        let stem = stem_for(diffuse)?;
        if let Some(files) = self.active(&stem) {
            return Some(Found { stem, files, first_person_fallback: false });
        }
        let file_start = stem.rfind('\\').map_or(0, |i| i + 1);
        let plain = stem[file_start..].strip_prefix(FIRST_PERSON).filter(|p| !p.is_empty())?;
        let plain_stem = format!("{}{plain}", &stem[..file_start]);
        let files = self.active(&plain_stem)?;
        Some(Found { stem: plain_stem, files, first_person_fallback: true })
    }

    fn active(&self, stem: &str) -> Option<SetFiles> {
        self.sets.get(stem).copied().filter(|f| f.state().is_active())
    }

    /// Every set (any state), by stem in sorted order.
    pub fn sets(&self) -> impl Iterator<Item = (&str, SetFiles)> + '_ {
        self.sets.iter().map(|(k, v)| (k.as_str(), *v))
    }

    /// The files under `textures\pbr\` that are no member of any set, sorted.
    pub fn unmatched(&self) -> &[String] {
        &self.unmatched
    }

    pub fn counts(&self) -> Counts {
        self.counts
    }

    /// No file under `textures\pbr\` at all.
    pub fn is_empty(&self) -> bool {
        self.counts.files == 0
    }

    /// One line for the log: the sets and what was left out.
    pub fn summary(&self) -> String {
        let c = &self.counts;
        format!(
            "authored materials: {} sets under textures\\pbr\\ ({} with _orm, {} height only; files: {} _orm, {} _n, {} _p, {} .toml, {} _albedo), {} incomplete sets and {} unmatched files ignored",
            c.active(),
            c.authored,
            c.height_only,
            c.orm,
            c.normal,
            c.height,
            c.scalars,
            c.albedo,
            c.incomplete,
            c.unmatched
        )
    }
}

#[cfg(test)]
mod tests;
