//! Offline `.fos` -> `.nvfsave` converter and `.fos` inspector (`fnvtool save`).
//!
//! The NVForever runtime never reads or writes `.fos` files; this command lives strictly
//! in `fnvtool` as an offline diagnostic and import tool (docs/design/save-load.md).

use crate::esm_cmd::open_load_order;
use crate::LoadOrderArgs;
use anyhow::Context;
use clap::Subcommand;
use fnv_esm::{LoadOrder, Schema};
use fnv_game::fos_convert::{convert_fos, inspect_fos, ConvertOptions};
use fnv_save::{PluginEntry, SaveKind};
use fnv_script::FunctionTable;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum SaveCmd {
    /// Inspect a `.fos` save file's header, plugins, global tables, and decoded record
    /// counts without requiring game plugins.
    InspectFos {
        /// Path to the `.fos` save file (read-only).
        input: PathBuf,
    },
    /// Convert a `.fos` save file into an NVForever `.nvfsave` file over a loaded plugin
    /// order, emitting a JSON conversion report on stdout.
    Convert {
        /// Path to the input `.fos` save file (read-only).
        input: PathBuf,
        /// Path to the output `.nvfsave` file (must be outside the game install and
        /// original My Games folder).
        #[arg(long)]
        out: PathBuf,
        #[command(flatten)]
        lo: LoadOrderArgs,
        /// Schema directory (defaults to `schemas/fnv`).
        #[arg(long, default_value = "schemas/fnv")]
        schema: PathBuf,
        /// Script function catalogue JSON (defaults to `<schema>/../script/functions.json`).
        #[arg(long)]
        functions: Option<PathBuf>,
        /// Omit PNG thumbnail encoding from the `.fos` RGB screenshot.
        #[arg(long)]
        no_thumbnail: bool,
    },
}

fn has_adjacent_cosave(fos_path: &Path) -> bool {
    fos_path.with_extension("nvse").is_file()
}

pub fn run(cmd: SaveCmd) -> anyhow::Result<Value> {
    match cmd {
        SaveCmd::InspectFos { input } => {
            let bytes = std::fs::read(&input).with_context(|| input.display().to_string())?;
            let fos = fnv_save::fos::Fos::parse(&bytes).with_context(|| input.display().to_string())?;
            let report = inspect_fos(&fos, has_adjacent_cosave(&input));
            Ok(serde_json::to_value(report)?)
        }
        SaveCmd::Convert { input, out, lo, schema, functions, no_thumbnail } => {
            let bytes = std::fs::read(&input).with_context(|| input.display().to_string())?;
            let fos = fnv_save::fos::Fos::parse(&bytes).with_context(|| input.display().to_string())?;
            let fp = functions.unwrap_or_else(|| schema.join("..").join("script").join("functions.json"));
            let ftext = std::fs::read_to_string(&fp).with_context(|| fp.display().to_string())?;
            let fjson: Value = serde_json::from_str(&ftext).with_context(|| fp.display().to_string())?;
            let table = FunctionTable::load(&fp).with_context(|| fp.display().to_string())?;

            let plugins = open_load_order(&lo)?;
            let plugin_entries: Vec<PluginEntry> = lo
                .plugins
                .iter()
                .zip(&plugins)
                .map(|(arg_name, plugin)| {
                    let path = lo.data_dir.join(arg_name);
                    let size = std::fs::metadata(&path).map_or(0, |m| m.len());
                    PluginEntry { name: plugin.name.clone(), size }
                })
                .collect();
            let order = LoadOrder::new(&plugins)?;
            let schema = Schema::load_dir(&schema)?;
            let forms = fnv_sim::FormStore::build(&order, &schema);
            let refs = fnv_sim::RefTable::build(&order, &schema);
            let data = fnv_game::GameData::build(&order, &schema, table, &fjson, &refs);

            let opts = ConvertOptions {
                kind: SaveKind::Manual,
                plugins: plugin_entries,
                include_thumbnail: !no_thumbnail,
                cosave_present: has_adjacent_cosave(&input),
            };
            let converted = convert_fos(&fos, &data, &forms, &refs, &opts).map_err(anyhow::Error::msg)?;
            let body_bytes = serde_json::to_vec(&converted.body)?;
            fnv_save::write(&out, &converted.header, &converted.shot, &body_bytes, false)
                .with_context(|| out.display().to_string())?;
            // Verify both the written container header and the full typed GameSnapshot body
            // round-trip cleanly (catches any JSON serialization issue such as non-finite floats).
            let verified = fnv_save::read_header(&out).with_context(|| out.display().to_string())?;
            let (_, _, verified_body): (fnv_save::SaveHeader, Vec<u8>, fnv_save::SaveBody<fnv_game::game::GameSnapshot>) =
                fnv_save::read_body(&out).with_context(|| format!("verifying written save body {}", out.display()))?;
            verified_body.game.check().map_err(anyhow::Error::msg)?;
            let mut doc = serde_json::to_value(&converted.report)?;
            doc["output"] = json!({
                "path": out.display().to_string(),
                "body_bytes": body_bytes.len(),
                "thumbnail_bytes": converted.shot.len(),
                "verified_header_format": verified.header.format,
                "verified_body_format": verified_body.format,
                "plugins": verified.header.plugins.len(),
            });
            Ok(doc)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_fos_refuses_non_fos_file() {
        let dir = std::env::temp_dir().join(format!("nvf-save-cmd-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("bad.fos");
        std::fs::write(&bad, b"NOT_A_FOS").unwrap();
        assert!(run(SaveCmd::InspectFos { input: bad.clone() }).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_body_rejects_null_float_in_quest_var() {
        let dir = std::env::temp_dir().join(format!("nvf-save-cmd-null-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("null_float.nvfsave");
        let header = fnv_save::SaveHeader::new(SaveKind::Manual);
        // Mimics serde_json serializing an unsanitized NaN/Inf Value::Num as JSON null.
        let body = br#"{"format":"nvf.save.body/1","game":{"instances":[[{"kind":"quest","quest":"00000810"},{"script":null,"vars":{"7":{"num":null}}}]]},"engine":{"format":"nvf.engine.state/1"}}"#;
        fnv_save::write(&path, &header, &[], body, false).unwrap();
        let err = fnv_save::read_body::<fnv_save::SaveBody<fnv_game::game::GameSnapshot>>(&path).unwrap_err();
        assert!(err.to_string().contains("invalid type: null, expected f64"), "unexpected error: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inspect_fos_reports_v2_decoded_counts() {
        let dir = std::env::temp_dir().join(format!("nvf-save-cmd-inspect-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("minimal.fos");
        // Build minimal valid FO3SAVEGAME with 1 ECZN changed form.
        let mut h = Vec::new();
        h.extend(0x30u32.to_le_bytes());
        h.push(0x7C);
        h.extend([0u8; 64]);
        h.push(0x7C);
        for v in [0u32, 0, 1] {
            h.extend(v.to_le_bytes());
            h.push(0x7C);
        }
        for s in ["Test", "Drifter"] {
            h.extend((s.len() as u16).to_le_bytes());
            h.push(0x7C);
            h.extend(s.as_bytes());
            h.push(0x7C);
        }
        h.extend(2u32.to_le_bytes());
        h.push(0x7C);
        h.extend((4u16).to_le_bytes());
        h.push(0x7C);
        h.extend(b"Room");
        h.push(0x7C);
        h.extend((9u16).to_le_bytes());
        h.push(0x7C);
        h.extend(b"000.01.00");

        let mut b = b"FO3SAVEGAME".to_vec();
        b.extend((h.len() as u32).to_le_bytes());
        b.extend(&h);
        let mut pl = Vec::new();
        pl.extend([1u8, 0x7C]);
        let name = b"FalloutNV.esm";
        pl.extend((name.len() as u16).to_le_bytes());
        pl.push(0x7C);
        pl.extend(name);
        pl.push(0x7C);
        b.push(27);
        b.extend((pl.len() as u32).to_le_bytes());
        b.extend(&pl);
        let table_at = b.len();
        b.extend([0u8; 32 + 78]);
        let g1 = b.len();
        let cf = b.len();
        // 1 ECZN record (ref index 1, bit 31, 17-byte payload).
        let mut ez = Vec::new();
        for v in [1u32, 1, 1, 14] {
            ez.extend(v.to_le_bytes());
        }
        ez.push(0x7C);
        b.extend([0, 0, 1]);
        b.extend((1u32 << 31).to_le_bytes());
        b.push(fnv_save::fos::TYPE_ECZN | 0x40);
        b.push(27);
        b.extend((ez.len() as u16).to_le_bytes());
        b.extend(&ez);
        let g2 = b.len();
        b.extend(1000u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        let fid = b.len();
        b.extend(1u32.to_le_bytes());
        b.extend(0x0000_0999u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        let tail = b.len();
        b.extend([5, 0, 0, 0, 0, 0, 0, 0, 0x7C]);
        for (i, v) in [fid, tail, g1, cf, g2, 0, 1, 1].iter().enumerate() {
            b[table_at + 4 * i..table_at + 4 * i + 4].copy_from_slice(&(*v as u32).to_le_bytes());
        }
        std::fs::write(&path, &b).unwrap();

        let val = run(SaveCmd::InspectFos { input: path }).unwrap();
        assert_eq!(val["schema"], "fnv.save.fos_inspect/1");
        assert_eq!(val["decoded_counts"]["encounter_zones"], 1);
        assert_eq!(val["decoded_counts"]["actor_changes"], 0);
        assert_eq!(val["decoded_counts"]["ref_extras"], 0);
        assert_eq!(val["decoded_counts"]["cells"], 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
