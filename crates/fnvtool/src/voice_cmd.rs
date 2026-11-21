//! `fnvtool voice ...`: dialogue voice files (fnv-voice). Rules and numbers:
//! docs/spec-notes/voice.md.

use crate::esm_cmd::open_load_order;
use crate::LoadOrderArgs;
use anyhow::{bail, Context};
use clap::Subcommand;
use fnv_esm::LoadOrder;
use fnv_vfs::{Vfs, DEFAULT_ARCHIVE_LIST};
use fnv_voice::path::VOICE_ROOT;
use fnv_voice::VoiceBank;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Subcommand)]
pub enum VoiceCmd {
    /// Where the sound of each response of a dialogue line lives and how long it is, for
    /// every voice type that has it and for one speaker (fnv.voice.line/1).
    Line {
        #[command(flatten)]
        lo: LoadOrderArgs,
        /// The line (INFO), load-order FormID in hex.
        #[arg(long, value_parser = crate::parse_formid)]
        info: u32,
        /// Who says it: Editor ID or hex FormID of an NPC, creature or talking activator,
        /// or of a placed reference of one. Default: the line's fixed speaker (ANAM).
        #[arg(long)]
        speaker: Option<String>,
    },
    /// The voice files of the install against the lines of the load order: counts by
    /// type, how many responses have a sound, sounds no line names (fnv.voice.coverage/1).
    Coverage {
        #[command(flatten)]
        lo: LoadOrderArgs,
        /// Also read every sound's headers: total length, sample rates (slow).
        #[arg(long)]
        lengths: bool,
    },
    /// The length of sound files from their headers (fnv.voice.length/1).
    Length {
        /// Loose `.ogg` or `.wav` files.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// A `.lip` file decoded: the face animation of a response as one track of weights
    /// per channel, 30 frames per second (fnv.voice.lip/1; docs/spec-notes/lipsync.md).
    Lip {
        #[command(flatten)]
        lo: LoadOrderArgs,
        /// Path of the `.lip` (or of its sound) in the archives, e.g.
        /// `sound\voice\falloutnv.esm\<voice type>\<name>.lip`.
        #[arg(long, conflicts_with = "info")]
        path: Option<String>,
        /// Instead of a path: the line (INFO), load-order FormID in hex.
        #[arg(long, value_parser = crate::parse_formid)]
        info: Option<u32>,
        /// With --info: the response's number (default: the line's first response).
        #[arg(long)]
        response: Option<u8>,
        /// With --info: who says it (Editor ID or hex FormID). Default: the line's fixed
        /// speaker, else the first voice type that has the file.
        #[arg(long)]
        speaker: Option<String>,
    },
    /// Loose `.lip` files decoded (fnv.voice.lip/1 each).
    LipFile {
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Every `.lip` file of the install decoded and counted: files decoded without
    /// residue, frames, channels in use, the letter test (fnv.voice.lip_coverage/1).
    LipCoverage {
        #[command(flatten)]
        lo: LoadOrderArgs,
        /// Also measure every sound and compare the animation's end with it (slow).
        #[arg(long)]
        lengths: bool,
    },
}

/// The archives and loose files of `data_dir` for `plugins`, in the game's precedence.
pub fn open_vfs(data_dir: &Path, plugins: &[String]) -> anyhow::Result<Vfs> {
    let names: Vec<&str> = plugins.iter().map(String::as_str).collect();
    let order = Vfs::archive_order(data_dir, &DEFAULT_ARCHIVE_LIST, &names);
    Vfs::open(data_dir, &order).with_context(|| data_dir.display().to_string())
}

pub fn run(cmd: VoiceCmd) -> anyhow::Result<Value> {
    match cmd {
        VoiceCmd::Line { lo, info, speaker } => with_bank(&lo, |bank, lo| line(bank, lo, info, speaker.as_deref())),
        VoiceCmd::Coverage { lo, lengths } => with_bank(&lo, |bank, _| Ok(fnv_voice::coverage::coverage(bank, lengths))),
        VoiceCmd::Lip { lo, path, info, response, speaker } => with_bank(&lo, |bank, lo| lip(bank, lo, path.as_deref(), info, response, speaker.as_deref())),
        VoiceCmd::LipCoverage { lo, lengths } => with_bank(&lo, |bank, _| Ok(fnv_voice::lip_coverage::lip_coverage(bank, lengths))),
        VoiceCmd::LipFile { files } => {
            let rows: Vec<Value> = files
                .iter()
                .map(|f| {
                    let r = std::fs::read(f).map_err(|e| e.to_string()).and_then(|b| fnv_voice::Lip::parse(&b).map_err(|e| e.to_string()));
                    match r {
                        Ok(l) => l.to_json(Some(&f.display().to_string()), None),
                        Err(e) => json!({"format": fnv_voice::lip::LIP_FORMAT, "path": f.display().to_string(), "error": e}),
                    }
                })
                .collect();
            Ok(json!({"format": "fnv.voice.lip_files/1", "files": rows}))
        }
        VoiceCmd::Length { files } => {
            let rows: Vec<Value> = files
                .iter()
                .map(|f| {
                    let r = std::fs::read(f).map_err(|e| e.to_string()).and_then(|b| fnv_voice::sound_info(&b).map_err(|e| e.to_string()));
                    match r {
                        Ok(i) => json!({"file": f.display().to_string(), "info": i}),
                        Err(e) => json!({"file": f.display().to_string(), "error": e}),
                    }
                })
                .collect();
            Ok(json!({"format": "fnv.voice.length/1", "files": rows}))
        }
    }
}

fn with_bank(a: &LoadOrderArgs, f: impl FnOnce(&VoiceBank, &LoadOrder<'_>) -> anyhow::Result<Value>) -> anyhow::Result<Value> {
    let plugins = open_load_order(a)?;
    let lo = LoadOrder::new(&plugins)?;
    let vfs = Arc::new(open_vfs(&a.data_dir, &a.plugins)?);
    f(&VoiceBank::build(&lo, vfs), &lo)
}

/// `fnvtool voice lip`: one `.lip` of the archives, by path or by line.
fn lip(bank: &VoiceBank, lo: &LoadOrder, path: Option<&str>, info: Option<u32>, response: Option<u8>, speaker: Option<&str>) -> anyhow::Result<Value> {
    let sound = match (path, info) {
        (Some(p), _) => {
            let p = fnv_vfs::normalize(p);
            let stem = p.rsplit_once('.').map_or(p.as_str(), |x| x.0).to_string();
            fnv_voice::path::SOUND_EXTENSIONS.iter().map(|e| format!("{stem}.{e}")).find(|s| bank.vfs().exists(s)).unwrap_or(format!("{stem}.ogg"))
        }
        (None, Some(info)) => {
            let ix = &bank.index;
            let Some(l) = ix.lines.get(&info) else { bail!("no dialogue line (INFO) {}", hex(info)) };
            let k = match response {
                Some(n) => l.responses.iter().position(|r| r.number == n).with_context(|| format!("line {} has no response number {n}", hex(info)))?,
                None => 0,
            };
            let base = match speaker {
                Some(s) => Some(speaker_base(lo, s)?),
                None => l.speaker,
            };
            let by_speaker = base.and_then(|b| bank.file(info, k, b));
            let any = || ix.voice_types.keys().find_map(|&vt| bank.file_for_voice(info, k, vt));
            by_speaker.or_else(any).with_context(|| format!("no voice file for line {} response index {k}", hex(info)))?
        }
        (None, None) => bail!("give --path or --info"),
    };
    let lip_path = fnv_voice::lip::lip_path(&sound);
    let bytes = bank.read(&lip_path).with_context(|| format!("no file {lip_path}"))?;
    let lip = fnv_voice::Lip::parse(&bytes).with_context(|| lip_path.clone())?;
    let mut v = lip.to_json(Some(&lip_path), bank.seconds(&sound));
    v["source"] = json!(bank.vfs().source_of(&lip_path));
    v["stored_bytes"] = json!(bytes.len());
    v["reencode_byte_identical"] = json!(lip.to_bytes() == bytes);
    Ok(v)
}

fn hex(id: u32) -> String {
    format!("0x{id:08X}")
}

/// A speaker given as hex FormID or Editor ID: an actor base or talking activator, or a
/// placed reference of one (its base).
fn speaker_base(lo: &LoadOrder, text: &str) -> anyhow::Result<u32> {
    let as_id = crate::parse_formid(text).ok().filter(|_| text.len() >= 3 && text.chars().all(|c| c.is_ascii_hexdigit() || c == 'x' || c == 'X'));
    let base_of = |pi: usize, r: &fnv_esm::Record| -> Option<u32> {
        match &r.header.sig {
            b"NPC_" | b"CREA" | b"TACT" => lo.to_global(pi, r.header.form_id),
            b"ACHR" | b"ACRE" | b"REFR" => {
                let subs = r.subrecords().ok()?;
                let name = subs.iter().find(|s| &s.sig == b"NAME")?;
                lo.to_global(pi, u32::from_le_bytes(name.data.get(0..4)?.try_into().ok()?))
            }
            _ => None,
        }
    };
    if let Some(id) = as_id {
        let Some(d) = lo.index.get(&id).and_then(|d| d.last()) else { bail!("no record {}", hex(id)) };
        let pi = d.plugin as usize;
        return base_of(pi, lo.records[pi][d.ordinal as usize]).with_context(|| format!("{} is no actor, talking activator or reference", hex(id)));
    }
    let want = text.to_ascii_lowercase();
    for defs in lo.index.values() {
        let Some(d) = defs.last() else { continue };
        let pi = d.plugin as usize;
        let r = lo.records[pi][d.ordinal as usize];
        if !matches!(&r.header.sig, b"NPC_" | b"CREA" | b"TACT" | b"ACHR" | b"ACRE") {
            continue;
        }
        let Ok(subs) = r.subrecords() else { continue };
        let Some(e) = subs.iter().find(|s| &s.sig == b"EDID") else { continue };
        let edid = e.data.split(|&b| b == 0).next().unwrap_or_default();
        if edid.eq_ignore_ascii_case(want.as_bytes()) {
            return base_of(pi, r).with_context(|| format!("{text} has no base"));
        }
    }
    bail!("no actor, talking activator or actor reference with Editor ID {text}")
}

fn line(bank: &VoiceBank, lo: &LoadOrder, info: u32, speaker: Option<&str>) -> anyhow::Result<Value> {
    let ix = &bank.index;
    let Some(l) = ix.lines.get(&info) else { bail!("no dialogue line (INFO) {}", hex(info)) };
    let name = |id: Option<u32>| id.map(|i| json!({"form": hex(i), "edid": ix.edids.get(&i)}));
    let base = match speaker {
        Some(s) => Some(speaker_base(lo, s)?),
        None => l.speaker,
    };
    let voice = base.and_then(|b| ix.voice_of(b));
    let mut total = 0.0;
    let mut all_spoken = base.is_some();
    let mut responses = Vec::new();
    for (k, r) in l.responses.iter().enumerate() {
        let mut files = Vec::new();
        for (&vt, vt_name) in &ix.voice_types {
            let Some(path) = bank.file_for_voice(info, k, vt) else { continue };
            let lip = format!("{}.lip", path.rsplit_once('.').map_or(path.as_str(), |x| x.0));
            files.push(json!({
                "voice_type": vt_name,
                "path": path,
                "source": bank.vfs().source_of(&path),
                "sound": bank.info(&path),
                "lip": bank.vfs().exists(&lip),
            }));
        }
        let spoken = voice.and_then(|v| bank.file_for_voice(info, k, v)).map(|p| {
            let seconds = bank.seconds(&p);
            json!({"path": p, "seconds": seconds})
        });
        match spoken.as_ref().and_then(|s| s["seconds"].as_f64()) {
            Some(s) => total += s,
            None => all_spoken = false,
        }
        responses.push(json!({
            "index": k,
            "number": r.number,
            "text_chars": r.chars,
            "sound_form": r.sound.map(hex),
            "file_name": ix.stem(info, k),
            "spoken": spoken,
            "files": files,
        }));
    }
    Ok(json!({
        "format": "fnv.voice.line/1",
        "info": hex(info),
        "plugin": ix.plugin_of(info),
        "folder": ix.plugin_of(info).map(|p| format!("{VOICE_ROOT}\\{}", p.to_ascii_lowercase())),
        "quest": name(l.quest),
        "topic": name(l.topic),
        "fixed_speaker": l.speaker.map(hex),
        "speaker": base.map(|b| json!({
            "base": hex(b),
            "voice_type": voice.map(|v| json!({"form": hex(v), "edid": ix.voice_types.get(&v)})),
        })),
        "responses": responses,
        "seconds": if all_spoken && !l.responses.is_empty() { json!(total) } else { Value::Null },
    }))
}
