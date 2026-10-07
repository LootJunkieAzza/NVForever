//! fnvtool — the parity lab's candidate CLI. Every command prints one JSON
//! document on stdout (schemas in docs/cli-contract.md) and diagnostics on stderr.
//! Exit codes: 0 success, 1 the input could not be processed, 2 usage error.

mod bsa_cmd;
mod dds_cmd;
mod esm_cmd;
mod install_cmd;
mod nif_cmd;
mod source;

use clap::{Args, Parser, Subcommand};
use serde_json::Value;
use std::path::PathBuf;
use std::process::ExitCode;

/// NVForever (FNV engine reimplementation) inspection CLI.
#[derive(Parser)]
#[command(name = "fnvtool", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Record the game install (read-only) and create NVForever's own home.
    Setup {
        /// The Fallout New Vegas folder (or its Data folder).
        #[arg(long)]
        game: PathBuf,
        #[command(flatten)]
        home: HomeArg,
    },
    /// Show where NVForever keeps its files.
    Paths {
        #[command(flatten)]
        home: HomeArg,
    },
    /// BSA archives.
    #[command(subcommand)]
    Bsa(BsaCmd),
    /// ESM/ESP plugins.
    #[command(subcommand)]
    Esm(EsmCmd),
    /// NIF meshes and KF animations.
    #[command(subcommand)]
    Nif(NifCmd),
    /// DDS textures.
    #[command(subcommand)]
    Dds(DdsCmd),
}

#[derive(Subcommand)]
pub enum DdsCmd {
    /// Header, format and mip layout of every .dds in an archive (fnv.dds.listing/1).
    List { archive: PathBuf },
}

#[derive(Args)]
pub struct HomeArg {
    /// NVForever home directory (default: NVFOREVER_HOME or the platform data dir).
    #[arg(long)]
    pub home: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum BsaCmd {
    /// List every file with hashes, sizes and (by default) a content SHA-256.
    List {
        archive: PathBuf,
        /// Skip decoding files (no size/sha256 columns).
        #[arg(long)]
        no_content_hash: bool,
    },
    /// Extract one file.
    Extract {
        archive: PathBuf,
        /// Path inside the archive (case-insensitive, / or \).
        path_in_archive: String,
        out_file: PathBuf,
    },
    /// Byte-identical rewrite check plus decode of every file.
    Verify { archive: PathBuf },
}

#[derive(Subcommand)]
pub enum EsmCmd {
    /// TES4 header.
    Header { plugin: PathBuf },
    /// Record/group counts.
    Stats { plugin: PathBuf },
    /// Every record with FormID, type, EDID and FULL.
    Inventory { plugin: PathBuf },
    /// Byte-identical rewrite and per-record payload checks.
    Verify { plugin: PathBuf },
    /// Schema coverage report (work queue of undecoded subrecords).
    Coverage {
        plugin: PathBuf,
        #[arg(long)]
        schema: PathBuf,
        #[arg(long, default_value_t = 200)]
        max_rows: usize,
    },
    /// Decode records with the schema, selected by FormID or Editor ID.
    #[command(group(clap::ArgGroup::new("select").required(true).args(["formid", "edid"])))]
    Decode {
        plugin: PathBuf,
        #[arg(long)]
        schema: PathBuf,
        /// File-local FormID in hex, with or without 0x.
        #[arg(long, value_parser = parse_formid)]
        formid: Option<u32>,
        /// Editor ID (case-insensitive).
        #[arg(long)]
        edid: Option<String>,
    },
    /// Load-order resolution report.
    Loadorder {
        #[command(flatten)]
        lo: LoadOrderArgs,
        /// Also resolve every FormID-typed field the schema knows about.
        #[arg(long)]
        schema: Option<PathBuf>,
    },
    /// Observational statistics per (record type, subrecord) for schema work.
    Survey {
        #[command(flatten)]
        lo: LoadOrderArgs,
        /// Only survey this plugin (case-insensitive name).
        #[arg(long)]
        plugin: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum NifCmd {
    /// Schema coverage over many files: per block type, how many decode exactly and
    /// re-encode byte-identically (the Layer 2 work queue).
    Coverage {
        /// BSA archives, loose .nif/.kf files or directories.
        #[arg(required = true)]
        sources: Vec<PathBuf>,
        #[arg(long)]
        schema: PathBuf,
        /// Only files whose path contains this text (case-insensitive).
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value_t = 200)]
        max_rows: usize,
        /// Skip the re-encode check (faster).
        #[arg(long)]
        no_reencode: bool,
    },
    /// Per-file header summary of every .nif/.kf in an archive (fnv.nif.stats/1).
    Stats { archive: PathBuf },
    /// Scene objects and geometry summaries of every .nif/.kf in an archive (fnv.nif.scene/1).
    Scene {
        archive: PathBuf,
        #[arg(long)]
        schema: PathBuf,
    },
    /// Animation sequences of every .kf/.nif in an archive, with sanity statistics
    /// for the evaluated tracks (fnv.nif.anim/1).
    Anim {
        archive: PathBuf,
        #[arg(long)]
        schema: PathBuf,
        /// Only files whose path contains this text.
        #[arg(long)]
        filter: Option<String>,
    },
    /// Every texture path referenced by every mesh in the virtual file system (default
    /// archive order + loose files), and whether it resolves (fnv.nif.textures/1).
    Textures {
        data_dir: PathBuf,
        #[arg(long)]
        schema: PathBuf,
        #[arg(long, default_value_t = 200)]
        max_rows: usize,
    },
    /// Decode one file's blocks with the schema.
    Decode {
        /// A loose file, or a BSA archive together with --path.
        source: PathBuf,
        /// Path inside the archive.
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        schema: PathBuf,
        /// Only this block index.
        #[arg(long)]
        block: Option<usize>,
    },
}

#[derive(Args)]
pub struct LoadOrderArgs {
    pub data_dir: PathBuf,
    /// Plugins in load order.
    #[arg(required = true)]
    pub plugins: Vec<String>,
}

fn parse_formid(s: &str) -> Result<u32, String> {
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u32::from_str_radix(hex, 16).map_err(|_| format!("not a hex FormID: {s}"))
}

fn emit(v: &Value) {
    use std::io::Write;
    let mut lock = std::io::stdout().lock();
    let _ = lock.write_all(v.to_string().as_bytes());
    let _ = lock.write_all(b"\n");
}

/// Diagnostics go to stderr. Default level is `warn`; set `NVFOREVER_LOG` (an
/// `EnvFilter` directive such as `debug` or `fnv_esm=trace`) for more.
fn init_logging() {
    use std::io::IsTerminal;
    let filter = tracing_subscriber::EnvFilter::try_from_env("NVFOREVER_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging();
    let res = match cli.cmd {
        Cmd::Setup { game, home } => install_cmd::setup(&game, &home),
        Cmd::Paths { home } => install_cmd::paths(&home),
        Cmd::Bsa(c) => bsa_cmd::run(c),
        Cmd::Esm(c) => esm_cmd::run(c),
        Cmd::Nif(c) => nif_cmd::run(c),
        Cmd::Dds(c) => dds_cmd::run(c),
    };
    match res {
        Ok(v) => {
            emit(&v);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn formid_forms() {
        assert_eq!(parse_formid("0x0001F4A2"), Ok(0x1F4A2));
        assert_eq!(parse_formid("0X1f4a2"), Ok(0x1F4A2));
        assert_eq!(parse_formid("1F4A2"), Ok(0x1F4A2));
        assert!(parse_formid("zz").is_err());
    }
}
