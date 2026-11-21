//! The pack validator behind `fnvtool pbr check` and `fnvtool pbr selftest` (#2981;
//! the format: docs/spec-notes/pbr-materials.md; the output: docs/cli-contract.md,
//! `fnv.pbr.check/1`). It reads DDS headers and file sizes and parses scalar files; it
//! never decodes pixels and does not judge what the maps hold.
//!
//! [`check_file`] judges one file from its game path (`textures\pbr\...`), the bytes of
//! its head ([`head_len`]: the DDS headers, or the whole scalar file) and its length;
//! [`check_pack`] runs it over every file of a pack and adds what only the whole pack
//! shows (incomplete sets) and the totals. `check` feeds it files on disk, `selftest`
//! bytes built in memory ([`selftest_inputs`]).
//!
//! [design] Every rule is NVForever's own (the note's "Compression and sizes" section);
//! the facts about DDS headers are fnv-dds' [documented: Microsoft, "Programming Guide for
//! DDS", "DDS_HEADER_DXT10"].

use fnv_dds::{DdsHeader, Format};
use fnv_pbr::{classify, parse_scalars, Member, PbrIndex, SetState};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// The most bytes of a DDS member the checks read: the header and the DX10 extension.
pub const DDS_HEAD_LEN: usize = fnv_dds::HEADER_LEN + fnv_dds::DX10_HEADER_LEN;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// The file breaks the pack format's rules (docs/spec-notes/pbr-materials.md) or is
    /// broken: fails `pbr check`.
    Error,
    /// Allowed, but probably not what the author meant, or ignored by this version.
    Warning,
}

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warning => "warning",
        }
    }
}

/// One thing the checker found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub level: Level,
    /// Stable, kebab-case (docs/cli-contract.md lists them).
    pub code: &'static str,
    /// The file's (or, for a set, the stem's) game path, `textures\pbr\...`.
    pub path: String,
    pub message: String,
}

impl Finding {
    fn json(&self) -> Value {
        json!({"level": self.level.name(), "code": self.code, "path": self.path, "message": self.message})
    }
}

/// What the header of a DDS member says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdsInfo {
    /// The format as the file states it (`BC7_UNORM_SRGB`, `DXT1`), or a description of
    /// an unknown one.
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub levels: u32,
    /// Payload bytes the header's layout needs, when it could be sized.
    pub payload_bytes: Option<u64>,
}

/// One file judged.
#[derive(Debug, Clone)]
pub struct FileCheck {
    pub path: String,
    /// `None`: no member of any set (unmatched).
    pub member: Option<Member>,
    pub bytes: u64,
    pub dds: Option<DdsInfo>,
    pub findings: Vec<Finding>,
}

impl FileCheck {
    fn add(&mut self, level: Level, code: &'static str, message: String) {
        self.findings.push(Finding { level, code, path: self.path.clone(), message });
    }

    /// No error was found in the file.
    pub fn accepted(&self) -> bool {
        !self.findings.iter().any(|f| f.level == Level::Error)
    }
}

/// How many bytes from the start of the file [`check_file`] needs: the DDS headers for
/// `_orm`, `_n`, `_p` and `_albedo`, the whole file for a scalar file (`None`), nothing for
/// unmatched files (they are not read).
pub fn head_len(path: &str) -> Option<usize> {
    match classify(path).map(|(_, m)| m) {
        Some(Member::Orm | Member::Normal | Member::Height | Member::Albedo) => Some(DDS_HEAD_LEN),
        Some(Member::Scalars) => None,
        None => Some(0),
    }
}

/// Whether a member holds colour (sRGB) data: `_albedo` alone; every other DDS member is
/// linear.
fn colour_member(m: Member) -> bool {
    m == Member::Albedo
}

/// [design] The formats each member may use (docs/spec-notes/pbr-materials.md,
/// "Compression and sizes" and "Channels and colour spaces"): `_orm` BC7, BC1 or BC3;
/// `_n` BC5 (unsigned or signed), BC7, BC1 or BC3 (the last three as three-channel
/// normals); `_p` BC4 (unsigned or signed), BC1 or BC3; `_albedo` BC7, BC1 or BC3 (the
/// colour formats a game diffuse uses, with an alpha channel). The DX10 sRGB variants count
/// as their format (the colour space follows the role; [`check_file`] warns about them on
/// linear members).
pub fn allowed_formats(m: Member) -> &'static [Format] {
    match m {
        Member::Orm | Member::Albedo => &[Format::Bc7, Format::Dxt1, Format::Dxt5],
        Member::Normal => &[Format::Bc5U, Format::Bc5S, Format::Bc7, Format::Dxt1, Format::Dxt5],
        Member::Height => &[Format::Bc4U, Format::Bc4S, Format::Dxt1, Format::Dxt5],
        Member::Scalars => &[],
    }
}

fn allowed_text(m: Member) -> &'static str {
    match m {
        Member::Orm | Member::Albedo => "BC7, BC1 or BC3",
        Member::Normal => "BC5 (unsigned or signed), BC7, BC1 or BC3",
        Member::Height => "BC4 (unsigned or signed), BC1 or BC3",
        Member::Scalars => "none",
    }
}

/// Mip levels a full chain of a `w`×`h` top level has, down to 1×1.
fn full_chain(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

/// Judge one file: `path` its game path (`textures\pbr\...`), `head` at least the first
/// [`head_len`] bytes of it (or all of it, when shorter), `len` its whole length.
pub fn check_file(path: &str, head: &[u8], len: u64) -> FileCheck {
    let member = classify(path).map(|(_, m)| m);
    let mut fc = FileCheck { path: path.to_string(), member, bytes: len, dds: None, findings: Vec::new() };
    match member {
        None => fc.add(Level::Warning, "unmatched-file", "This file is no member of any set (no `_orm.dds`, `_n.dds`, `_p.dds`, `_albedo.dds` or `.toml` ending), so the engine ignores it.".into()),
        Some(Member::Scalars) => {
            let f = parse_scalars(head);
            if f.unreadable {
                let why = f.warnings.first().map_or("unreadable", String::as_str);
                fc.add(Level::Error, "toml-invalid", format!("The scalar file cannot be read ({why}), so none of its values apply."));
            } else {
                for w in f.warnings {
                    fc.add(Level::Warning, "scalar-ignored-or-clamped", format!("Scalar file: {w}."));
                }
            }
        }
        Some(m) => check_dds(&mut fc, m, head, len),
    }
    fc
}

fn check_dds(fc: &mut FileCheck, m: Member, head: &[u8], len: u64) {
    let h = match DdsHeader::parse(head) {
        Ok(h) => h,
        Err(e) => {
            fc.add(Level::Error, "dds-unreadable", format!("Not a readable DDS file: {e}."));
            return;
        }
    };
    // The header's own size field and the pixel format's (dds.md items 2 and 3: 124
    // and 32 [documented: MS "DDS_HEADER", "DDS_PIXELFORMAT"]). fnv-dds does not check
    // them; a file with other values is malformed even when the rest reads.
    let (header_size, pf_size) = (u32::from_le_bytes(head[4..8].try_into().unwrap()), u32::from_le_bytes(head[76..80].try_into().unwrap()));
    if header_size != 124 || pf_size != 32 {
        fc.add(Level::Error, "bad-header-size", format!("The header states its size as {header_size} and the pixel format's as {pf_size}; a DDS file has 124 and 32."));
    }
    let format = h.format();
    let name = match h.format_name() {
        Ok(n) => n.to_string(),
        Err(d) => d,
    };
    let (w, ht, levels) = (h.width, h.height, h.levels());
    let mut info = DdsInfo { format: name.clone(), width: w, height: ht, levels, payload_bytes: None };

    let shape = if h.is_cubemap() {
        Some("a cube map".to_string())
    } else if h.is_volume() {
        Some("a volume texture".to_string())
    } else {
        h.dx10_refusal()
    };
    if let Some(why) = &shape {
        fc.add(Level::Error, "not-single-2d", format!("The file is {why}; members must be one flat 2D texture."));
    }

    let size_ok = (1..=fnv_dds::MAX_SIDE).contains(&w) && (1..=fnv_dds::MAX_SIDE).contains(&ht);
    if !size_ok {
        fc.add(Level::Error, "bad-size", format!("The header's size {w}x{ht} is outside 1 to {} on a side.", fnv_dds::MAX_SIDE));
    } else {
        if w % 4 != 0 || ht % 4 != 0 {
            fc.add(Level::Error, "not-block-aligned", format!("The top level is {w}x{ht}; both sides must be multiples of 4 for the file to be uploaded whole."));
        }
        if !w.is_power_of_two() || !ht.is_power_of_two() {
            fc.add(Level::Warning, "not-power-of-two", format!("The top level is {w}x{ht}, not powers of two: allowed, but tools disagree on the smallest mip levels."));
        }
        let full = full_chain(w, ht);
        if levels > 32 {
            fc.add(Level::Error, "dds-unreadable", format!("The header counts {levels} mip levels, more than any file can hold."));
        } else if levels < full {
            fc.add(Level::Warning, "incomplete-mips", format!("The file has {levels} of the {full} mip levels a {w}x{ht} texture has down to 1x1; ship the full chain."));
        } else if levels > full {
            // A texture cannot have more levels than its sides allow; the engine's
            // renderer refuses such a file (nvf-assets `texture.rs`, `checked_image`).
            fc.add(Level::Error, "extra-mips", format!("The header counts {levels} mip levels, more than the {full} a {w}x{ht} texture has down to 1x1."));
        }
    }

    match format {
        Ok(f) if allowed_formats(m).contains(&f) => {}
        _ => fc.add(Level::Error, "format-not-allowed", format!("{name} is not a format for a `{}` member; use {}.", m.suffix().trim_end_matches(".dds"), allowed_text(m))),
    }
    if h.srgb_hint() == Some(true) && !colour_member(m) {
        fc.add(Level::Warning, "srgb-linear-member", format!("{name} marks the data as sRGB, but this member holds linear data and is read as linear whatever the header says; export it without sRGB."));
    }

    if let (Ok(f), true, true, true) = (format, size_ok, levels <= 32, h.layout_sized() && shape.is_none()) {
        let expected = h.expected_size(f) as u64;
        let actual = len.saturating_sub(h.data_offset() as u64);
        if actual < expected {
            fc.add(Level::Error, "truncated", format!("The pixel data is {actual} bytes, but the header's size, format and mip levels need {expected}."));
        } else if actual > expected {
            fc.add(Level::Warning, "trailing-bytes", format!("{} bytes follow the {expected} bytes of pixel data the header describes; they are never read.", actual - expected));
        }
        info.payload_bytes = Some(expected);
    }
    fc.dds = Some(info);
}

/// A pack judged: every file, and what only the whole pack shows.
#[derive(Debug, Clone)]
pub struct PackCheck {
    /// Sorted by game path, ASCII case folded.
    pub files: Vec<FileCheck>,
    /// One per incomplete set, and one per `_albedo` of a height-only set.
    pub set_findings: Vec<Finding>,
    pub index: PbrIndex,
}

/// Judge a pack: `inputs` are (game path, head bytes, length) per file, as
/// [`check_file`] takes them.
pub fn check_pack<I>(inputs: I) -> PackCheck
where
    I: IntoIterator<Item = (String, Vec<u8>, u64)>,
{
    let mut files: Vec<FileCheck> = inputs.into_iter().map(|(p, head, len)| check_file(&p, &head, len)).collect();
    files.sort_by_cached_key(|f| fnv_vfs::normalize(&f.path));
    let index = PbrIndex::build(files.iter().map(|f| f.path.as_str()));
    // The stem as the pack spells it, from its first file.
    let mut spelled: BTreeMap<String, String> = BTreeMap::new();
    for f in &files {
        if let Some((stem, m)) = classify(&f.path) {
            spelled.entry(stem).or_insert_with(|| f.path[..f.path.len() - m.suffix().len()].to_string());
        }
    }
    let mut set_findings: Vec<Finding> = index
        .sets()
        .filter(|(_, s)| s.state() == SetState::Incomplete)
        .map(|(stem, s)| {
            let has: Vec<String> = s.members().map(|m| format!("`{}`", m.suffix())).collect();
            Finding {
                level: Level::Warning,
                code: "incomplete-set",
                path: spelled.get(stem).cloned().unwrap_or_else(|| stem.to_string()),
                message: format!("This set has only {} and no `_orm.dds` or `_p.dds`, so it changes nothing.", has.join(", ")),
            }
        })
        .collect();
    // An `_albedo` is drawn by an authored set only: in a height-only set it is never read.
    set_findings.extend(files.iter().filter(|f| f.member == Some(Member::Albedo)).filter_map(|f| {
        let (stem, _) = classify(&f.path)?;
        (index.get(&stem)?.state() == SetState::HeightOnly).then(|| Finding {
            level: Level::Warning,
            code: "albedo-without-orm",
            path: f.path.clone(),
            message: "This set has no `_orm.dds`, so it is not an authored set and its `_albedo` is never drawn; the game's diffuse stays the base colour.".into(),
        })
    }));
    PackCheck { files, set_findings, index }
}

impl PackCheck {
    /// Every finding, sorted by path, then errors first, then code.
    pub fn findings(&self) -> Vec<&Finding> {
        let mut all: Vec<&Finding> = self.files.iter().flat_map(|f| f.findings.iter()).chain(self.set_findings.iter()).collect();
        all.sort_by_cached_key(|f| (fnv_vfs::normalize(&f.path), f.level, f.code));
        all
    }

    pub fn errors(&self) -> usize {
        self.findings().iter().filter(|f| f.level == Level::Error).count()
    }

    pub fn warnings(&self) -> usize {
        self.findings().iter().filter(|f| f.level == Level::Warning).count()
    }

    /// GPU memory the pack would take: the payload bytes of every `_orm`, `_n` and `_p`
    /// without an error, in a set that changes a material (authored or height only), and of
    /// every `_albedo` without an error in an authored set (the only sets that draw it).
    pub fn gpu_bytes_estimate(&self) -> u64 {
        self.files
            .iter()
            .filter(|f| f.accepted())
            .filter(|f| {
                classify(&f.path).is_some_and(|(stem, m)| {
                    self.index.get(&stem).is_some_and(|s| if m == Member::Albedo { s.state() == SetState::Authored } else { s.state().is_active() })
                })
            })
            .filter_map(|f| f.dds.as_ref().and_then(|d| d.payload_bytes))
            .sum()
    }

    /// The format names of the DDS members, sorted, each once.
    pub fn formats(&self) -> Vec<String> {
        self.files.iter().filter_map(|f| f.dds.as_ref().map(|d| d.format.clone())).collect::<BTreeSet<_>>().into_iter().collect()
    }

    /// The `fnv.pbr.check/1` document, `root` naming what was read.
    pub fn json(&self, root: Value) -> Value {
        let files: Vec<Value> = self
            .files
            .iter()
            .map(|f| {
                let d = f.dds.as_ref();
                json!({
                    "path": f.path,
                    "member": f.member.map(Member::name),
                    "bytes": f.bytes,
                    "format": d.map(|d| d.format.clone()),
                    "width": d.map(|d| d.width),
                    "height": d.map(|d| d.height),
                    "levels": d.map(|d| d.levels),
                    "payload_bytes": d.and_then(|d| d.payload_bytes),
                    "accepted": f.accepted(),
                })
            })
            .collect();
        let mut members = Map::new();
        for m in Member::ALL {
            let of: Vec<&FileCheck> = self.files.iter().filter(|f| f.member == Some(m)).collect();
            let mut entry = Map::new();
            entry.insert("files".into(), json!(of.len()));
            if matches!(m, Member::Orm | Member::Normal | Member::Height | Member::Albedo) {
                let mut formats: BTreeMap<String, usize> = BTreeMap::new();
                for d in of.iter().filter_map(|f| f.dds.as_ref()) {
                    *formats.entry(d.format.clone()).or_default() += 1;
                }
                entry.insert("formats".into(), json!(formats));
                entry.insert("largest_side".into(), json!(of.iter().filter_map(|f| f.dds.as_ref()).map(|d| d.width.max(d.height)).max()));
            }
            members.insert(m.name().into(), Value::Object(entry));
        }
        let c = self.index.counts();
        json!({
            "schema": "fnv.pbr.check/1",
            "root": root,
            "ok": self.errors() == 0,
            "files": files,
            "findings": self.findings().iter().map(|f| f.json()).collect::<Vec<_>>(),
            "totals": {
                "files": self.files.len(),
                "bytes": self.files.iter().map(|f| f.bytes).sum::<u64>(),
                "sets": {"total": c.sets, "authored": c.authored, "height_only": c.height_only, "incomplete": c.incomplete},
                "unmatched": c.unmatched,
                "members": members,
                "formats": self.formats(),
                "gpu_bytes_estimate": self.gpu_bytes_estimate(),
                "errors": self.errors(),
                "warnings": self.warnings(),
            },
        })
    }
}

/// A DDS file built here, never copied from a game or pack file: the 128-byte header
/// with the FourCC `DX10` and the DX10 extension (`dxgi` code, 2D, no flags, an array of
/// one), `w`×`h`, `mips` levels, then as many zero bytes as the layout needs, plus
/// `extra` (negative: that many fewer). [documented: Microsoft, "DDS_HEADER" and
/// "DDS_HEADER_DXT10"]
pub fn dx10_dds(dxgi: u32, w: u32, h: u32, mips: u32, extra: i64) -> Vec<u8> {
    let mut d = vec![0u8; fnv_dds::HEADER_LEN];
    d[..4].copy_from_slice(b"DDS ");
    // Header size 124; flags caps|height|width|pixel format|mip count|linear size;
    // pixel format size 32 and its FourCC flag; caps: texture (and mipmap|complex).
    let caps = if mips > 1 { 0x40_1008 } else { 0x1000 };
    for (o, v) in [(4usize, 124u32), (8, 0x2_1007), (12, h), (16, w), (28, mips), (76, 32), (80, 4), (108, caps)] {
        d[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    d[84..88].copy_from_slice(b"DX10");
    for v in [dxgi, 3, 0, 1, 0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    let need = DdsHeader::parse(&d).ok().and_then(|hd| hd.format().ok().map(|f| hd.expected_size(f))).unwrap_or(0) as i64;
    d.resize(d.len() + (need + extra).max(0) as usize, 0);
    d
}

/// DXGI codes the self-test writes [documented: MS "DXGI_FORMAT enumeration"].
const BC4_UNORM: u32 = 80;
const BC5_UNORM: u32 = 83;
const BC7_UNORM: u32 = 98;
const BC7_UNORM_SRGB: u32 = 99;

/// The self-test's files: a clean set (BC7 `_orm` 8x8, BC5 `_n` 16x16, BC4 `_p` 4x4, an sRGB
/// BC7 `_albedo` 8x8, which raises no sRGB warning on a colour member, a scalar file), then
/// one broken file for each finding in [`SELFTEST_EXPECTED`].
pub fn selftest_inputs() -> Vec<(String, Vec<u8>)> {
    let p = |name: &str| format!("textures\\pbr\\selftest\\{name}");
    vec![
        (p("clean_orm.dds"), dx10_dds(BC7_UNORM, 8, 8, 4, 0)),
        (p("clean_n.dds"), dx10_dds(BC5_UNORM, 16, 16, 5, 0)),
        (p("clean_p.dds"), dx10_dds(BC4_UNORM, 4, 4, 3, 0)),
        (p("clean_albedo.dds"), dx10_dds(BC7_UNORM_SRGB, 8, 8, 4, 0)),
        (p("clean.toml"), b"roughness_scale = 0.9\nreflectance = 0.35\nmetal_gain = 1\n".to_vec()),
        // 8x2: powers of two with a full chain, only the 2 is not a multiple of 4.
        (p("unaligned_orm.dds"), dx10_dds(BC7_UNORM, 8, 2, 4, 0)),
        (p("wrongformat_orm.dds"), dx10_dds(BC4_UNORM, 4, 4, 3, 0)),
        (p("truncated_p.dds"), dx10_dds(BC4_UNORM, 8, 8, 4, -16)),
        (p("srgb_orm.dds"), dx10_dds(BC7_UNORM_SRGB, 4, 4, 3, 0)),
        (p("badheader_orm.dds"), {
            let mut d = dx10_dds(BC7_UNORM, 4, 4, 3, 0);
            d[4..8].copy_from_slice(&100u32.to_le_bytes());
            d
        }),
        (p("lonely_n.dds"), dx10_dds(BC5_UNORM, 4, 4, 3, 0)),
        // A plain-name file is no member.
        (p("clean.dds"), b"not a member".to_vec()),
    ]
}

/// The formats the clean set's four DDS members carry.
pub const SELFTEST_FORMATS: [&str; 4] = ["BC4_UNORM", "BC5_UNORM", "BC7_UNORM", "BC7_UNORM_SRGB"];

/// (path, code) of every finding the self-test's files must raise, and no other.
pub const SELFTEST_EXPECTED: [(&str, &str); 7] = [
    ("textures\\pbr\\selftest\\badheader_orm.dds", "bad-header-size"),
    ("textures\\pbr\\selftest\\clean.dds", "unmatched-file"),
    ("textures\\pbr\\selftest\\lonely", "incomplete-set"),
    ("textures\\pbr\\selftest\\srgb_orm.dds", "srgb-linear-member"),
    ("textures\\pbr\\selftest\\truncated_p.dds", "truncated"),
    ("textures\\pbr\\selftest\\unaligned_orm.dds", "not-block-aligned"),
    ("textures\\pbr\\selftest\\wrongformat_orm.dds", "format-not-allowed"),
];

/// The self-test's verdict.
#[derive(Debug, Clone)]
pub struct Selftest {
    pub pack: PackCheck,
    /// Formats of the clean set's members, sorted.
    pub formats: Vec<String>,
    pub missing: Vec<(String, String)>,
    pub unexpected: Vec<(String, String)>,
}

impl Selftest {
    pub fn pass(&self) -> bool {
        self.missing.is_empty() && self.unexpected.is_empty() && self.formats == SELFTEST_FORMATS
    }

    /// The last line printed, for a smoke expectation.
    pub fn line(&self) -> String {
        if self.pass() {
            format!("pbr selftest: PASS ({} formats, {} expected findings)", self.formats.len(), SELFTEST_EXPECTED.len())
        } else {
            let pairs = |v: &[(String, String)]| v.iter().map(|(p, c)| format!("{c} at {p}")).collect::<Vec<_>>().join(", ");
            format!(
                "pbr selftest: FAIL (formats {:?}, expected {:?}; missing: [{}]; unexpected: [{}])",
                self.formats,
                SELFTEST_FORMATS,
                pairs(&self.missing),
                pairs(&self.unexpected)
            )
        }
    }

    pub fn json(&self) -> Value {
        let pairs = |v: &[(String, String)]| v.iter().map(|(p, c)| json!({"path": p, "code": c})).collect::<Vec<_>>();
        let mut v = self.pack.json(json!({"folder": null, "form": "selftest", "pbr_folder": null}));
        v["selftest"] = json!({
            "formats": self.formats,
            "expected": SELFTEST_EXPECTED.iter().map(|(p, c)| json!({"path": p, "code": c})).collect::<Vec<_>>(),
            "missing": pairs(&self.missing),
            "unexpected": pairs(&self.unexpected),
            "pass": self.pass(),
            // The last line printed, also here: a tool scenario judges JSON only.
            "verdict": self.line(),
        });
        v
    }
}

/// Run the checks on [`selftest_inputs`], in memory, the same way `check` runs them on
/// files (the heads cut to [`head_len`]).
pub fn selftest() -> Selftest {
    let inputs = selftest_inputs().into_iter().map(|(p, bytes)| {
        let len = bytes.len() as u64;
        let head = match head_len(&p) {
            Some(n) => bytes[..n.min(bytes.len())].to_vec(),
            None => bytes,
        };
        (p, head, len)
    });
    let pack = check_pack(inputs);
    let raised: BTreeSet<(String, String)> = pack.findings().iter().map(|f| (f.path.clone(), f.code.to_string())).collect();
    let expected: BTreeSet<(String, String)> = SELFTEST_EXPECTED.iter().map(|(p, c)| (p.to_string(), c.to_string())).collect();
    let formats = pack
        .files
        .iter()
        .filter(|f| f.path.contains("\\clean"))
        .filter_map(|f| f.dds.as_ref().map(|d| d.format.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Selftest {
        formats,
        missing: expected.difference(&raised).cloned().collect(),
        unexpected: raised.difference(&expected).cloned().collect(),
        pack,
    }
}
